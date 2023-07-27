use ratatui::widgets::ListState;

pub struct StatefulList<T> {
    pub title: Option<String>,
    pub state: ListState,
    pub items: Option<Vec<T>>,
    item_line_count: Vec<usize>,
    page_lines: usize,
}

impl<T> StatefulList<T> {
    pub fn new() -> StatefulList<T> {
        StatefulList {
            title: None,
            state: ListState::default(),
            items: None,
            item_line_count: Vec::new(),
            page_lines: 0,
        }
    }

    pub fn next(&mut self) {
        if let Some(item_count) = self.items.as_ref().map(|items| items.len()) {
            let next = self.state.selected()
                .map(|i| if item_count > i + 1 { i + 1 } else { i });

            self.state.select(next);
        }
    }

    pub fn prev(&mut self) {
        if let Some(_) = self.items {
            let prev = self.state.selected()
                .map(|i| if i > 0 { i - 1 } else { 0 });

            self.state.select(prev);
        }
    }

    pub fn select(&mut self, index: Option<usize>) {
        if index.is_some() {
            self.state.select(index);
        } else {
            self.state.select(Some(0));
        }

        self.item_line_count.clear();

        // Refresh paging
        self.page_lines = 0;
    }

    pub fn select_first(&mut self) {
        self.select(Some(0));
    }

    pub fn select_last(&mut self) {
        if let Some(items) = self.items.as_ref() {
            let last = items.len() - 1;

            self.state.select(Some(last));
        }
    }

    pub fn select_next_page(&mut self) {
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

    pub fn select_prev_page(&mut self) {
        if let Some(selected) = self.state.selected() {
            let mut offset = self.state.offset();
            let mut counted_lines: usize = 0;

            if offset != selected {
                offset = selected;
            }

            for i in (0..=selected).rev() {
                counted_lines += self.item_line_count[i];

                if offset == 0 {
                    self.select_first();

                    break;
                } else if counted_lines == self.page_lines {
                    *self.state.offset_mut() = offset;
                    self.state.select(Some(offset));

                    break;
                } else if counted_lines > self.page_lines {
                    // Skip the incomplete item at the end
                    *self.state.offset_mut() = offset + 1;
                    self.state.select(Some(offset + 1));

                    break;
                }

                offset -= 1;
            }
        }
    }

    pub fn deselect(&mut self) {
        self.state.select(None);
    }

    pub fn is_selected(&self) -> bool {
        self.state.selected().is_some()
    }

    pub fn prepare_paging(&mut self, page_lines: usize, f: fn(&T) -> usize) {
        if page_lines != self.page_lines {
            let mut item_line_count = Vec::new();

            if let Some(items) = self.items.as_ref() {
                for item in items.iter() {
                    let line_count = f(item);
    
                    item_line_count.push(line_count);
                }
            }

            self.item_line_count = item_line_count;
            self.page_lines = page_lines;
        }
    }

    pub fn get_selected_item(&self) -> Option<&T> {
        let index = self.state.selected()?;
        let item = self.items.as_ref()?.get(index);

        item
    }
}
