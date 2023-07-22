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
    io_rx: mpsc::Receiver<IoEvent>,
    selected_view: Option<View>,
    browse: StatefulList<browse::Item>,
}

impl App {
    pub fn new(io_rx: mpsc::Receiver<IoEvent>) -> Self {
        Self {
            io_rx,
            selected_view: None,
            browse: StatefulList::new(),
        }
    }

    /// Handle a user action
    pub async fn do_action(&mut self, key: KeyEvent) -> AppReturn {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Tab => self.select_next_view(),
                KeyCode::Up => self.browse.prev(),
                KeyCode::Down => self.browse.next(),
                KeyCode::Char('q') => return AppReturn::Exit,
                _ => ()
            }
        }

        AppReturn::Continue
    }

    pub async fn update_on_event(&mut self) -> AppReturn {
        if let Some(io_event) = self.io_rx.recv().await {
            match io_event {
                IoEvent::Tick => (),
                IoEvent::Input(key) => {
                    return self.do_action(key).await;
                }
                IoEvent::BrowseTitle(browse_title) => {
                    self.browse.title = Some(browse_title);
                }
                IoEvent::BrowseItems(items) => {
                    self.browse.items = Some(items);
                }
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

    pub fn get_selected_view(&self) -> Option<&View> {
        self.selected_view.as_ref()
    }
}
