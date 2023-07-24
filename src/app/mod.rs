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

    fn select(&mut self) {
        self.select_first();
    }

    fn select_first(&mut self) {
        self.state.select(Some(0));
        self.item_line_count.clear();

        // Refresh paging
        self.page_lines = 0;
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
                    } else {
                        if let Some(browse_items) = self.browse.items.as_mut() {
                            if offset == browse_items.len() {
                                browse_items.append(&mut items);

                                // Refresh paging
                                self.browse.page_lines = 0;
                            } else {
                                self.to_roon.send(IoEvent::BrowseRefresh).await.unwrap();
                            }
                        }
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
            KeyCode::Home => self.browse.select_first(),
            KeyCode::End => self.browse.select_last(),
            KeyCode::PageUp => self.browse.select_prev_page(),
            KeyCode::PageDown => self.browse.select_next_page(),
            KeyCode::Char('h') => self.to_roon.send(IoEvent::BrowseHome).await.unwrap(),
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
    
                if item_line_count.len() != items.len() {
                    panic!();
                }
            }
            self.browse.item_line_count = item_line_count;
            self.browse.page_lines = page_lines;
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
