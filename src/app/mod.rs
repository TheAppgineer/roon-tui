use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rust_roon_api::{browse, transport::{QueueItem, QueueOperation, QueueChange, Zone, ZoneSeek}};
use tokio::sync::mpsc;

use crate::io::IoEvent;
use crate::app::stateful_list::StatefulList;

pub mod ui;
pub mod stateful_list;

#[derive(Debug, PartialEq, Eq)]
pub enum AppReturn {
    Exit,
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Browse = 0,
    Queue = 1,
    NowPlaying = 2,
    Zones = 3,
}

pub struct App {
    to_roon: mpsc::Sender<IoEvent>,
    from_roon: mpsc::Receiver<IoEvent>,
    core_name: Option<String>,
    selected_view: Option<View>,
    prev_view: Option<View>,
    browse: StatefulList<browse::Item>,
    pending_item_key: Option<String>,
    zones: StatefulList<(String, String)>,
    selected_zone: Option<Zone>,
    zone_seek: Option<ZoneSeek>,
    queue: StatefulList<QueueItem>,
}

impl App {
    pub fn new(to_roon: mpsc::Sender<IoEvent>, from_roon: mpsc::Receiver<IoEvent>) -> Self {
        Self {
            to_roon,
            from_roon,
            core_name: None,
            selected_view: None,
            prev_view: None,
            browse: StatefulList::new(),
            pending_item_key: None,
            zones: StatefulList::new(),
            selected_zone: None,
            zone_seek: None,
            queue: StatefulList::new(),
        }
    }

    pub async fn update_on_event(&mut self) -> AppReturn {
        if let Some(io_event) = self.from_roon.recv().await {
            match io_event {
                IoEvent::Tick => (),
                IoEvent::Input(key) => {
                    return self.do_action(key).await;
                }
                IoEvent::CoreName(name) => {
                    self.core_name = name;
                }
                IoEvent::BrowseTitle(browse_title) => {
                    if self.selected_view.is_none() {
                        self.selected_view = Some(View::Browse);
                    }

                    self.browse.title = Some(browse_title);
                }
                IoEvent::BrowseList(offset, mut items) => {
                    if offset == 0 {
                        self.browse.items = Some(items);

                        if let Some(view) = self.selected_view.as_ref() {
                            if *view == View::Browse {
                                self.browse.select_first();
                            }
                        }
                    } else if let Some(browse_items) = self.browse.items.as_mut() {
                        if offset == browse_items.len() {
                            browse_items.append(&mut items);

                            // Refresh paging
                            self.browse.select_first();
                        } else {
                            self.to_roon.send(IoEvent::BrowseRefresh).await.unwrap();
                        }
                    }
                }
                IoEvent::QueueList(queue_list) => {
                    self.queue.items = Some(queue_list);
                }
                IoEvent::QueueListChanges(changes) => {
                    self.apply_queue_changes(changes);
                }
                IoEvent::Zones(zones) => {
                    self.zones.items = Some(zones);
                }
                IoEvent::ZoneSelect => {
                    self.pending_item_key = self.get_item_key();
                    self.select_view(Some(View::Zones));
                }
                IoEvent::ZoneChanged(zone) => {
                    self.selected_zone = Some(zone);

                    if self.pending_item_key.is_some() {
                        self.to_roon.send(IoEvent::BrowseSelected(self.pending_item_key.take())).await.unwrap();
                    }
                }
                IoEvent::ZoneRemoved(_) => self.selected_zone = None,
                IoEvent::ZoneSeek(seek) => self.zone_seek = Some(seek),
                _ => ()
            }
        }

        AppReturn::Continue
    }

    fn apply_queue_changes(&mut self, changes: Vec<QueueChange>) -> Option<()> {
        let queue = self.queue.items.as_mut()?;

        for change in changes {
            match change.operation {
                QueueOperation::Insert => {
                    for i in 0..change.items.as_ref()?.len() {
                        let item = change.items.as_ref()?.get(i)?;

                        queue.insert(change.index + i, item.to_owned());
                    }
                }
                QueueOperation::Remove => {
                    for _ in 0..change.count? {
                        queue.remove(change.index);
                    }
                }
            }
        }

        Some(())
    }

    fn select_view(&mut self, view: Option<View>) {
        self.prev_view = self.selected_view.take();

        if let Some(view) = &view {
            match view {
                View::Browse => {
                    self.browse.select(None);
                    self.queue.deselect();
                    self.zones.deselect();
                }
                View::Queue => {
                    self.browse.deselect();
                    self.queue.select(None);
                    self.zones.deselect();
                }
                View::Zones => {
                    let index = if let Some(zone) = &self.selected_zone {
                        if let Some(items) = self.zones.items.as_ref() {
                            items
                                .iter()
                                .position(|(zone_id, _)| *zone_id == *zone.zone_id)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    self.zones.select(index);
                    self.queue.deselect();
                    self.browse.deselect();
                }
                _  => {
                    self.browse.deselect();
                    self.queue.deselect();
                    self.zones.deselect();
                }
            };
        }

        self.selected_view = view;
    }

    fn select_next_view(&mut self) {
        let view_order = vec![View::Browse, View::Queue, View::NowPlaying];
        let next = match self.selected_view.as_ref() {
            Some(selected_view) => view_order.get(selected_view.to_owned() as usize + 1),
            None => view_order.get(0),
        };
        let next = next.cloned().unwrap_or(View::Browse);

        self.select_view(Some(next));
    }

    fn restore_view(&mut self) {
        let prev_view = self.prev_view.take();
        self.select_view(prev_view);
    }

    fn get_selected_view(&self) -> Option<&View> {
        self.selected_view.as_ref()
    }

    async fn do_action(&mut self, key: KeyEvent) -> AppReturn {
        if key.kind == KeyEventKind::Press {
            match key.code {
                // Global key codes
                KeyCode::Tab => self.select_next_view(),
                KeyCode::Char('z') => {
                    if key.modifiers == KeyModifiers::CONTROL {
                        self.select_view(Some(View::Zones));
                    }
                }
                KeyCode::Char('q') => return AppReturn::Exit,
                _ => {
                    // Key codes specific to the active view
                    if let Some(view) = self.selected_view.as_ref() {
                        match *view {
                            View::Browse => self.handle_browse_key_codes(key).await,
                            View::Queue => self.handle_queue_key_codes(key).await,
                            View::Zones => self.handle_zone_key_codes(key).await,
                            _ => (),
                        }
                    }
                }
            }
        }

        AppReturn::Continue
    }

    async fn handle_browse_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.browse.prev(),
            KeyCode::Down => self.browse.next(),
            KeyCode::Enter => {
                self.to_roon.send(IoEvent::BrowseSelected(self.get_item_key())).await.unwrap();
            }
            KeyCode::Esc => self.to_roon.send(IoEvent::BrowseBack).await.unwrap(),
            KeyCode::Home => {
                match key.modifiers {
                    KeyModifiers::NONE => self.browse.select_first(),
                    KeyModifiers::CONTROL => self.to_roon.send(IoEvent::BrowseHome).await.unwrap(),
                    _ => (),
                }
            }
            KeyCode::End => self.browse.select_last(),
            KeyCode::PageUp => self.browse.select_prev_page(),
            KeyCode::PageDown => self.browse.select_next_page(),
            KeyCode::F(5) => self.to_roon.send(IoEvent::BrowseRefresh).await.unwrap(),
            _ => (),
        }
    }

    async fn handle_queue_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.queue.prev(),
            KeyCode::Down => self.queue.next(),
            KeyCode::Home => self.queue.select_first(),
            KeyCode::End => self.queue.select_last(),
            KeyCode::PageUp => self.queue.select_prev_page(),
            KeyCode::PageDown => self.queue.select_next_page(),
            KeyCode::Enter => {
                if let Some(queue_item_id) = self.get_queue_item_id() {
                    self.to_roon.send(IoEvent::QueueSelected(queue_item_id)).await.unwrap();
                }
            }
            _ => (),
        }
    }

    async fn handle_zone_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.zones.prev(),
            KeyCode::Down => self.zones.next(),
            KeyCode::Home => self.zones.select_first(),
            KeyCode::End => self.zones.select_last(),
            KeyCode::PageUp => self.zones.select_prev_page(),
            KeyCode::PageDown => self.zones.select_next_page(),
            KeyCode::Enter => {
                let selected_zone_id = self.get_zone_id();
                self.restore_view();

                if let Some(zone_id) = selected_zone_id.as_ref() {
                    self.to_roon.send(IoEvent::ZoneSelected(zone_id.to_owned())).await.unwrap();
                }
            }
            KeyCode::Esc => self.restore_view(),
            _ => (),
        }
    }

    fn get_item_key(&self) -> Option<String> {
        self.browse.get_selected_item()?.item_key.to_owned()
    }

    fn get_queue_item_id(&self) -> Option<u32> {
        Some(self.queue.get_selected_item()?.queue_item_id)
    }

    fn get_zone_id(&self) -> Option<String> {
        self.zones.get_selected_item().map(|(zone_id, _)| zone_id).cloned()
    }
}
