use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use rust_roon_api::browse;
use tokio::sync::mpsc;

use crate::io::IoEvent;

pub mod ui;

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

struct StatefulList<T> {
    title: Option<String>,
    state: ListState,
    items: Option<Vec<T>>,
    item_line_count: Vec<usize>,
    page_lines: usize,
}

impl<T> StatefulList<T> {
    fn new() -> StatefulList<T> {
        StatefulList {
            title: None,
            state: ListState::default(),
            items: None,
            item_line_count: Vec::new(),
            page_lines: 0,
        }
    }

    fn next(&mut self) {
        if let Some(item_count) = self.items.as_ref().map(|items| items.len()) {
            let next = self.state.selected()
                .map(|i| if item_count > i + 1 { i + 1 } else { i });

            self.state.select(next);
        }
    }

    fn prev(&mut self) {
        if let Some(_) = self.items {
            let prev = self.state.selected()
                .map(|i| if i > 0 { i - 1 } else { 0 });

            self.state.select(prev);
        }
    }

    fn select(&mut self, index: Option<usize>) {
        if index.is_some() {
            self.state.select(index);
        } else {
            self.state.select(Some(0));
        }

        self.item_line_count.clear();

        // Refresh paging
        self.page_lines = 0;
    }

    fn select_first(&mut self) {
        self.select(Some(0));
    }

    fn select_last(&mut self) {
        if let Some(items) = self.items.as_ref() {
            let last = items.len() - 1;

            self.state.select(Some(last));
        }
    }

    fn select_next_page(&mut self) {
        if let Some(selected) = self.state.selected() {
            let offset = self.state.offset();
            let item_count = self.items.as_ref().unwrap().len();
            let mut counted_lines: usize = 0;

            if offset < selected {
                for i in offset..selected {
                    counted_lines += self.item_line_count[i];
                }

                if counted_lines >= self.page_lines {
                    *self.state.offset_mut() = selected;
                }
            }

            counted_lines = 0;

            for i in selected..item_count {
                counted_lines += self.item_line_count[i];

                if counted_lines == self.page_lines {
                    self.state.select(Some(i));
                    break;
                } else if counted_lines > self.page_lines {
                    // Skip the incomplete item at the end
                    self.state.select(Some(i - 1));
                    break;
                }
            }

            if counted_lines < self.page_lines {
                self.select_last();
            }
        }
    }

    fn select_prev_page(&mut self) {
        if let Some(selected) = self.state.selected() {
            let mut offset = self.state.offset();
            let mut counted_lines: usize = 0;

            if offset != selected {
                offset = selected;
            }

            for i in 0..=selected {
                counted_lines += self.item_line_count[selected - i];

                if offset == 0 {
                    self.select_first();

                    return;
                } else if counted_lines >= self.page_lines {
                    *self.state.offset_mut() = offset;
                    self.state.select(Some(offset));

                    return;
                }

                offset -= 1;
            }
        }
    }

    fn deselect(&mut self) {
        self.state.select(None);
    }
}

/// The main application, containing the state
pub struct App {
    to_roon: mpsc::Sender<IoEvent>,
    from_roon: mpsc::Receiver<IoEvent>,
    core_name: Option<String>,
    selected_view: Option<View>,
    prev_view: Option<View>,
    browse: StatefulList<browse::Item>,
    pending_item_key: Option<String>,
    zones: StatefulList<(String, String)>,
    selected_zone: Option<(String, String)>,
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
        }
    }

    /// Handle a user action
    pub async fn do_action(&mut self, key: KeyEvent) -> AppReturn {
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
                            View::Zones => self.handle_zone_key_codes(key).await,
                            _ => (),
                        }
                    }
                }
            }
        }

        AppReturn::Continue
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
                            self.browse.page_lines = 0;
                        } else {
                            self.to_roon.send(IoEvent::BrowseRefresh).await.unwrap();
                        }
                    }
                }
                IoEvent::Zones(zones) => {
                    self.zones.items = Some(zones);
                }
                IoEvent::ZoneSelect => {
                    self.pending_item_key = self.get_item_key();
                    self.select_view(Some(View::Zones));
                }
                _ => ()
            }
        }

        AppReturn::Continue
    }

    fn select_view(&mut self, view: Option<View>) {
        self.prev_view = self.selected_view.take();

        if let Some(view) = &view {
            match view {
                View::Browse => {
                    self.browse.select(None);
                    self.zones.deselect();
                }
                View::Zones => {
                    let index = if let Some((sel_zone_id, _)) = &self.selected_zone {
                        if let Some(items) = self.zones.items.as_ref() {
                            items
                                .iter()
                                .position(|(zone_id, _)| *zone_id == *sel_zone_id)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    self.zones.select(index);
                    self.browse.deselect();
                }
                _  => {
                    self.browse.deselect();
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

    async fn handle_browse_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.browse.prev(),
            KeyCode::Down => self.browse.next(),
            KeyCode::Enter => {
                let zone_id = self.selected_zone
                    .as_ref()
                    .map(|(zone_id, _)| zone_id.to_owned());
                let opts = (self.get_item_key(), zone_id);
    
                self.to_roon.send(IoEvent::BrowseSelected(opts)).await.unwrap();
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

    async fn handle_zone_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.zones.prev(),
            KeyCode::Down => self.zones.next(),
            KeyCode::Home => self.zones.select_first(),
            KeyCode::End => self.zones.select_last(),
            KeyCode::Enter => {
                self.selected_zone = self.get_zone_id();
                self.restore_view();

                if let Some((zone_id, _)) = self.selected_zone.as_ref() {
                    let opts = (self.pending_item_key.take(), Some(zone_id.to_owned()));
    
                    self.to_roon.send(IoEvent::BrowseSelected(opts)).await.unwrap();
                }
            }
            KeyCode::Esc => self.restore_view(),
            _ => (),
        }
    }

    fn prepare_browse_paging(&mut self, page_lines: usize) {
        if page_lines != self.browse.page_lines {
            let mut item_line_count = Vec::new();

            if let Some(items) = self.browse.items.as_ref() {
                for i in 0..items.len() {
                    let line_count = if items[i].subtitle.is_some() {2usize} else {1usize};
    
                    item_line_count.push(line_count);
                }
            }

            self.browse.item_line_count = item_line_count;
            self.browse.page_lines = page_lines;
        }
    }

    fn get_item_key(&self) -> Option<String> {
        let index = self.browse.state.selected()?;
        let item = self.browse.items.as_ref()?.get(index)?;

        item.item_key.to_owned()
    }

    fn get_zone_id(&self) -> Option<(String, String)> {
        let index = self.zones.state.selected()?;
        let item = self.zones.items.as_ref()?.get(index)?;

        Some(item.to_owned())
    }
}
