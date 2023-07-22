use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
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
}

struct StatefulList<T> {
    title: Option<String>,
    state: ListState,
    items: Option<Vec<T>>
}

impl<T> StatefulList<T> {
    fn new() -> StatefulList<T> {
        StatefulList {
            title: None,
            state: ListState::default(),
            items: None
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

    fn select(&mut self) {
        self.state.select(Some(0));
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
    browse: StatefulList<browse::Item>,
}

impl App {
    pub fn new(to_roon: mpsc::Sender<IoEvent>, from_roon: mpsc::Receiver<IoEvent>) -> Self {
        Self {
            to_roon,
            from_roon,
            core_name: None,
            selected_view: None,
            browse: StatefulList::new(),
        }
    }

    /// Handle a user action
    pub async fn do_action(&mut self, key: KeyEvent) -> AppReturn {
        if key.kind == KeyEventKind::Press {
            match key.code {
                // Global key codes
                KeyCode::Tab => self.select_next_view(),
                KeyCode::Char('q') => return AppReturn::Exit,
                _ => {
                    // View specific key codes
                    if let Some(view) = self.selected_view.as_ref() {
                        match *view {
                            View::Browse => self.handle_browse_key_codes(key.code).await,
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
                    self.browse.title = Some(browse_title);
                }
                IoEvent::BrowseList(items) => {
                    self.browse.items = Some(items);

                    if let Some(view) = self.selected_view.as_ref() {
                        if *view == View::Browse {
                            self.browse.state.select(Some(0));
                        }
                    }
                }
                IoEvent::BrowseAppend(mut append_items) => {
                    if let Some(items) = self.browse.items.as_mut() {
                        items.append(&mut append_items);
                    }
                }
                _ => ()
            }
        }

        AppReturn::Continue
    }

    pub fn select_next_view(&mut self) {
        let view_order = vec![View::Browse, View::Queue, View::NowPlaying];
        let next = match self.selected_view.as_ref() {
            Some(selected_view) => view_order.get(selected_view.to_owned() as usize + 1),
            None => view_order.get(0),
        };

        let next = next.cloned().unwrap_or(View::Browse);

        match next {
            View::Browse => self.browse.select(),
            _  => self.browse.deselect(),
        };

        self.selected_view = Some(next);
    }

    async fn handle_browse_key_codes(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Up => self.browse.prev(),
            KeyCode::Down => self.browse.next(),
            KeyCode::Enter => {
                let item_key = self.get_item_key();
    
                self.to_roon.send(IoEvent::BrowseSelected(item_key)).await.unwrap();
            }
            KeyCode::Esc => {
                self.to_roon.send(IoEvent::BrowseBack).await.unwrap();
            }
            KeyCode::Home => self.browse.state.select(Some(0)),
            KeyCode::End => {
                if let Some(items) = self.browse.items.as_ref() {
                    let last = items.len() - 1;
    
                    self.browse.state.select(Some(last));
                }
            }
            KeyCode::Char('h') => self.to_roon.send(IoEvent::BrowseHome).await.unwrap(),
            _ => (),
        }
    }

    fn get_selected_view(&self) -> Option<&View> {
        self.selected_view.as_ref()
    }

    fn get_item_key(&self) -> Option<String> {
        let index = self.browse.state.selected()?;
        let item = self.browse.items.as_ref()?.get(index)?;

        item.item_key.to_owned()
    }
}
