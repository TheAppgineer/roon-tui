use any_ascii::any_ascii;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use roon_api::{
    browse,
    transport::{Control, QueueItem, QueueOperation, QueueChange, Zone, ZoneSeek, volume}
};
use tokio::sync::mpsc;

use crate::io::{EndPoint, IoEvent, QueueMode};
use crate::app::stateful_list::StatefulList;

pub mod ui;
pub mod stateful_list;

#[derive(Debug, PartialEq, Eq)]
pub enum AppReturn {
    Exit,
    Continue,
}

#[derive(Clone, Debug, PartialEq)]
enum View {
    Browse = 0,
    Queue = 1,
    NowPlaying = 2,
    Prompt = 3,
    Zones = 4,
    Grouping = 5,
    GroupingPreset = 6,
    Help = 7,
}

pub struct App {
    to_roon: mpsc::Sender<IoEvent>,
    from_roon: mpsc::Receiver<IoEvent>,
    no_unicode_symbols: bool,
    core_name: Option<String>,
    selected_view: Option<View>,
    prev_view: Option<View>,
    browse: StatefulList<browse::Item>,
    browse_match_list: Vec<usize>,
    pending_item_key: Option<String>,
    prompt: String,
    input: String,
    cursor_position: usize,
    max_input_len: usize,
    zones: StatefulList<(EndPoint, String)>,
    selected_zone: Option<Zone>,
    zone_seek: Option<ZoneSeek>,
    grouping: StatefulList<(String, String, bool)>,
    queue: StatefulList<QueueItem>,
    pause_on_track_end: bool,
    queue_mode: Option<&'static str>,
    matched_preset: Option<String>,
    matched_draft_preset: Option<String>,
    draft_match: bool,
}

impl App {
    pub fn new(to_roon: mpsc::Sender<IoEvent>, from_roon: mpsc::Receiver<IoEvent>, no_unicode_symbols: bool) -> Self {
        Self {
            to_roon,
            from_roon,
            no_unicode_symbols,
            core_name: None,
            selected_view: None,
            prev_view: None,
            browse: StatefulList::new(),
            browse_match_list: Vec::new(),
            pending_item_key: None,
            prompt: String::new(),
            input: String::new(),
            cursor_position: 0,
            max_input_len: 0,
            zones: StatefulList::new(),
            selected_zone: None,
            zone_seek: None,
            grouping: StatefulList::new(),
            queue: StatefulList::new(),
            pause_on_track_end: false,
            queue_mode: None,
            matched_preset: None,
            matched_draft_preset: None,
            draft_match: false,
        }
    }

    pub async fn update_on_event(&mut self) -> AppReturn {
        if let Some(io_event) = self.from_roon.recv().await {
            match io_event {
                IoEvent::Input(key) => {
                    return self.do_action(key).await;
                }
                IoEvent::CoreName(name) => {
                    self.core_name = name;
                }
                IoEvent::BrowseTitle(browse_title) => {
                    if self.selected_view.is_none() {
                        self.select_view(Some(View::Browse));
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
                    self.to_roon.send(IoEvent::QueueListLast(queue_list.last().cloned())).await.unwrap();
                    self.queue.items = Some(queue_list);
                }
                IoEvent::QueueListChanges(changes) => {
                    let selected = self.get_queue_select_string();
                    self.apply_queue_changes(&changes, selected);

                    if let Some(items) = self.queue.items.as_ref() {
                        self.to_roon.send(IoEvent::QueueListLast(items.last().cloned())).await.unwrap();
                    }
                }
                IoEvent::QueueModeCurrent(queue_mode) => {
                    let queue_mode = match queue_mode {
                        QueueMode::Manual => None,
                        QueueMode::RoonRadio => Some("Roon Radio"),
                        QueueMode::RandomAlbum => Some("Random Album"),
                        QueueMode::RandomTrack => Some("Random Track"),
                    };
                    self.queue_mode = queue_mode;
                }
                IoEvent::Zones(zones) => {
                    self.zones.items = Some(zones);

                    if self.selected_view == Some(View::Zones) {
                        let index = if let Some(zone) = &self.selected_zone {
                            if let Some(items) = self.zones.items.as_ref() {
                                items
                                    .iter()
                                    .position(|(end_point, _)| {
                                        match end_point {
                                            EndPoint::Zone(zone_id) => *zone_id == zone.zone_id,
                                            _ => false,
                                        }
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        self.zones.select(index);
                    }
                }
                IoEvent::ZoneSelect => {
                    if let Some(items) = self.zones.items.as_ref() {
                        let has_zones = items.iter()
                            .any(|(endpoint, _)| {
                                matches!(endpoint, EndPoint::Zone(_))
                            });

                        if has_zones {
                            // Save item_key as there are active zones
                            self.pending_item_key = self.get_item_key();
                        }
                    }

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
                IoEvent::ZoneGrouping(grouping) => {
                    if let Some(grouping) = grouping.as_ref() {
                        if !grouping.is_empty() {
                            match self.selected_view.as_ref() {
                                Some(View::Prompt) => self.restore_view(),
                                Some(View::Zones) => self.restore_view(),
                                Some(View::Help) => self.restore_view(),
                                _ => (),
                            }

                            self.matched_draft_preset = self.matched_preset.to_owned();
                            self.select_view(Some(View::Grouping));
                        }
                    }

                    self.grouping.items = grouping;
                }
                IoEvent::ZonePresetMatched(matched_preset) => {
                    if self.draft_match {
                        self.draft_match = false;
                        self.matched_draft_preset = matched_preset;
                    } else {
                        self.matched_preset = matched_preset;
                    }
                }
                IoEvent::PauseOnTrackEndActive(pause_on_track_end) => self.pause_on_track_end = pause_on_track_end,
                _ => ()
            }
        }

        AppReturn::Continue
    }

    fn get_queue_select_string(&self) -> Option<String> {
        let index = self.queue.state.selected()?;
        let selected = self.queue.items.as_ref()?.get(index)?.two_line.line1.to_owned();

        Some(selected)
    }

    fn apply_queue_changes(&mut self, changes: &Vec<QueueChange>, selected: Option<String>) -> Option<()> {
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

        if let Some(selected) = selected {
            let index = queue.iter().position(|item| item.two_line.line1 == selected);

            self.queue.select(index);
        };

        Some(())
    }

    fn select_view(&mut self, view: Option<View>) {
        self.prev_view = self.selected_view.take();

        match &view {
            Some(view) => {
                match view {
                    View::Browse => {
                        self.browse.select(None);
                        self.queue.deselect();
                        self.zones.deselect();
                        self.grouping.deselect();
                    }
                    View::Queue => {
                        self.browse.deselect();
                        self.queue.select(None);
                        self.zones.deselect();
                        self.grouping.deselect();
                    }
                    View::Zones => {
                        let index = if let Some(zone) = &self.selected_zone {
                            if let Some(items) = self.zones.items.as_ref() {
                                items
                                    .iter()
                                    .position(|(end_point, _)| {
                                        match end_point {
                                            EndPoint::Zone(zone_id) => *zone_id == zone.zone_id,
                                            _ => false,
                                        }
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        self.zones.select(index);
                        self.queue.deselect();
                        self.browse.deselect();
                        self.grouping.deselect();
                    }
                    View::Grouping => {
                        self.grouping.select(None);
                        self.browse.deselect();
                        self.queue.deselect();
                        self.zones.deselect();
                    }
                    _  => {
                        self.browse.deselect();
                        self.queue.deselect();
                        self.zones.deselect();
                        self.grouping.deselect();
                    }
                };
            }
            None => {
                self.browse.deselect();
                self.queue.deselect();
                self.zones.deselect();
                self.grouping.deselect();
            }
        }

        self.selected_view = view;
    }

    fn select_next_view(&mut self) {
        let view_order = vec![View::Browse, View::Queue, View::NowPlaying];
        let next = match self.selected_view.as_ref() {
            Some(selected_view) => {
                view_order.get(selected_view.to_owned() as usize + 1)
            }
            None => return,
        };
        let next = next.cloned().unwrap_or(View::Browse);

        self.select_view(Some(next));
    }

    fn select_prev_view(&mut self) {
        let view_order = vec![View::Browse, View::Queue, View::NowPlaying];
        let prev = match self.selected_view.as_ref() {
            Some(selected_view) => {
                view_order.get((selected_view.to_owned() as usize).wrapping_sub(1))
            }
            None => return,
        };
        let prev = prev.cloned().unwrap_or(View::NowPlaying);

        self.select_view(Some(prev));
    }

    fn restore_view(&mut self) {
        let prev_view = self.prev_view.take();
        self.select_view(prev_view);
    }

    fn get_selected_view(&self) -> Option<&View> {
        self.selected_view.as_ref()
    }

    fn set_max_input_len(&mut self, max_input_len: usize) {
        self.max_input_len = max_input_len;
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    fn move_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    fn enter_char(&mut self, new_char: char) {
        if self.input.len() < self.max_input_len {
            self.input.insert(self.cursor_position, new_char);
            self.move_cursor_right();
        }
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor_position != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.cursor_position;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.len())
    }

    fn reset_cursor(&mut self) {
        self.cursor_position = 0;
    }

    fn select_by_input(&mut self, key: char) {
        if !key.is_ascii() {
            return;
        }

        if let Some(items) = self.browse.items.as_ref() {
            let key = key.to_ascii_lowercase();
            let input = format!("{}{}", self.input, key);
            let split = if let Some(title) = self.browse.title.as_ref() {
                title == "Artists" || title == "Composers"
            } else {
                false
            };
            let mut order: Vec<char> = ('['..='~').rev().collect();
            order = [order, ('!'..='@').rev().collect()].concat();

            let index = if self.browse_match_list.is_empty() {
                items
                    .iter()
                    .position(|item| {
                        let title = any_ascii(&item.title);

                        while let Some(pop) = order.last() {
                            let mut pop_matched = false;
                            let mut matching = |sub: &str| {
                                if let Some(first_char) = sub.chars().next().unwrap().to_lowercase().next() {
                                    if first_char == *pop {
                                        if first_char == key {
                                            return true;
                                        } else {
                                            pop_matched = true;
                                        }
                                    }
                                }

                                false
                            };
                            let result = if split {
                                title.split(' ')
                                    .any(matching)
                            } else {
                                let title = title
                                    .to_ascii_lowercase()
                                    .replacen("the ", "", 1);
                                matching(&title)
                            };

                            if result {
                                return result;
                            } else if key == *pop || pop_matched {
                                break;
                            }

                            order.pop();
                        }

                        false
                    })
            } else {
                let skip = *self.browse_match_list.last().unwrap();
                let position = items
                    .iter()
                    .skip(skip)
                    .position(|item| {
                        // Find an upcoming item with matching input
                        let title = any_ascii(&item.title)
                            .to_ascii_lowercase()
                            .replacen("the ", "", 1);

                        if split {
                            title.split(' ')
                                .any(|sub| sub.starts_with(input.as_str()))
                        } else {
                            title.starts_with(input.as_str())
                        }
                    });

                position.map(|position| skip + position)
            };

            if index.is_some() {
                self.input = input;
                self.browse_match_list.push(index.unwrap());
                self.browse.state.select(index);
            }
        }
    }

    async fn do_action(&mut self, key: KeyEvent) -> AppReturn {
        if key.kind == KeyEventKind::Press {
            // Create a clone of selected_view to prevent second handle call on updated view
            let selected_view = self.selected_view.clone();

            // Global key codes
            match key.modifiers {
                KeyModifiers::NONE => {
                    match key.code {
                        KeyCode::Tab => {
                            self.input.clear();
                            self.browse_match_list.clear();
                            self.select_next_view();
                        }
                        _ => {
                            // Key codes specific to the active view
                            if let Some(view) = selected_view.as_ref() {
                                match *view {
                                    View::NowPlaying => self.handle_now_playing_key_codes(key).await,
                                    View::Queue => self.handle_queue_key_codes(key).await,
                                    View::Zones => self.handle_zone_key_codes(key).await,
                                    View::Grouping => {
                                        self.handle_grouping_key_codes(key).await;
                                    }
                                    View::Help => self.restore_view(),
                                    _ => (),
                                }
                            }
                        }
                    }
                }
                KeyModifiers::SHIFT => {
                    if key.code == KeyCode::BackTab {
                        self.input.clear();
                        self.browse_match_list.clear();
                        self.select_prev_view();
                    }
                }
                KeyModifiers::CONTROL => {
                    match key.code {
                        KeyCode::Up => self.to_roon.send(IoEvent::ChangeVolume(1)).await.unwrap(),
                        KeyCode::Down => self.to_roon.send(IoEvent::ChangeVolume(-1)).await.unwrap(),
                        KeyCode::Left => self.to_roon.send(IoEvent::Control(Control::Previous)).await.unwrap(),
                        KeyCode::Right => self.to_roon.send(IoEvent::Control(Control::Next)).await.unwrap(),
                        KeyCode::Delete => self.to_roon.send(IoEvent::QueueClear).await.unwrap(),
                        KeyCode::Char('e') => self.to_roon.send(IoEvent::PauseOnTrackEndReq).await.unwrap(),
                        KeyCode::Char('p') | KeyCode::Char(' ') => self.to_roon.send(IoEvent::Control(Control::PlayPause)).await.unwrap(),
                        KeyCode::Char('q') => self.to_roon.send(IoEvent::QueueModeNext).await.unwrap(),
                        KeyCode::Char('a') => self.to_roon.send(IoEvent::QueueModeAppend).await.unwrap(),
                        KeyCode::Char('z') => {
                            if selected_view != Some(View::Zones) {
                                match selected_view {
                                    Some(View::Prompt) => self.restore_view(),
                                    Some(View::Grouping) => self.restore_view(),
                                    Some(View::Help) => self.restore_view(),
                                    _ => (),
                                }

                                self.select_view(Some(View::Zones));
                            }
                        }
                        KeyCode::Char('g') => {
                            if selected_view != Some(View::Grouping) {
                                self.to_roon.send(IoEvent::ZoneGroupReq).await.unwrap();
                            }
                        }
                        KeyCode::Char('h') => {
                            if selected_view != Some(View::Help) {
                                match selected_view {
                                    Some(View::Prompt) => self.restore_view(),
                                    Some(View::Zones) => self.restore_view(),
                                    Some(View::Grouping) => self.restore_view(),
                                    _ => (),
                                }

                                self.select_view(Some(View::Help));
                            }
                        }
                        KeyCode::Char('c') => return AppReturn::Exit,
                        _ => (),
                    }
                }
                _ => (),
            }

            // Key codes specific to the active view (with own modifier handling)
            if let Some(view) = selected_view.as_ref() {
                match *view {
                    View::Browse => self.handle_browse_key_codes(key).await,
                    View::Prompt => self.handle_prompt_key_codes(key).await,
                    View::GroupingPreset => self.handle_preset_key_codes(key).await,
                    _ => (),
                }
            }
        }

        AppReturn::Continue
    }

    async fn handle_browse_key_codes(&mut self, key: KeyEvent) {
        match key.modifiers {
            KeyModifiers::CONTROL => {
                if key.code == KeyCode::Home {
                    self.to_roon.send(IoEvent::BrowseHome).await.unwrap();
                }
            }
            KeyModifiers::SHIFT => {
                if let KeyCode::Char(key) = key.code {
                    self.select_by_input(key);
                }
            }
            KeyModifiers::NONE => {
                match key.code {
                    KeyCode::Char(key) => self.select_by_input(key),
                    KeyCode::Backspace => {
                        self.input.pop();
                        self.browse_match_list.pop();
                        self.browse.select(self.browse_match_list.last().cloned());
                    }
                    KeyCode::Up => self.browse.prev(),
                    KeyCode::Down => self.browse.next(),
                    KeyCode::Enter => {
                        self.input.clear();
                        self.browse_match_list.clear();
                        let item_key = self.get_item_key();

                        if let Some(item) = self.browse.get_selected_item() {
                            if let Some(prompt) = item.input_prompt.as_ref() {
                                self.prompt = prompt.prompt.to_owned();
                                self.pending_item_key = item_key;
                                self.select_view(Some(View::Prompt));
                            } else {
                                self.to_roon.send(IoEvent::BrowseSelected(item_key)).await.unwrap();
                            }
                        }
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.browse_match_list.clear();
                        self.to_roon.send(IoEvent::BrowseBack).await.unwrap();
                    }
                    KeyCode::Home => {
                        if self.input.is_empty() {
                            self.browse.select_first();
                        } else {
                            self.input.clear();
                            self.browse_match_list.clear();
                        }
                    }
                    KeyCode::End => self.browse.select_last(),
                    KeyCode::PageUp => self.browse.select_prev_page(),
                    KeyCode::PageDown => self.browse.select_next_page(),
                    KeyCode::F(5) => self.to_roon.send(IoEvent::BrowseRefresh).await.unwrap(),
                    _ => (),
                }
            }
            _ => (),
        }
    }

    async fn handle_now_playing_key_codes(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('m') => self.to_roon.send(IoEvent::Mute(volume::Mute::Mute)).await.unwrap(),
            KeyCode::Char('u') => self.to_roon.send(IoEvent::Mute(volume::Mute::Unmute)).await.unwrap(),
            KeyCode::Char('+') => self.to_roon.send(IoEvent::ChangeVolume(1)).await.unwrap(),
            KeyCode::Char('-') => self.to_roon.send(IoEvent::ChangeVolume(-1)).await.unwrap(),
            KeyCode::Char('r') => self.to_roon.send(IoEvent::Repeat).await.unwrap(),
            KeyCode::Char('s') => self.to_roon.send(IoEvent::Shuffle).await.unwrap(),
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
                    // Items before the selected one will be removed from the queue
                    // meaning that the selected one will get on top
                    self.queue.select_first();

                    self.to_roon.send(IoEvent::QueueSelected(queue_item_id)).await.unwrap();
                }
            }
            _ => (),
        }
    }

    async fn handle_prompt_key_codes(&mut self, key: KeyEvent) {
        match key.modifiers {
            KeyModifiers::SHIFT => {
                if let KeyCode::Char(to_insert) = key.code {
                    self.enter_char(to_insert);
                }
            }
            KeyModifiers::NONE => {
                match key.code {
                    KeyCode::Enter => {
                        if self.pending_item_key.is_some() {
                            self.to_roon.send(IoEvent::BrowseInput(self.input.clone())).await.unwrap();
                            self.to_roon.send(IoEvent::BrowseSelected(self.pending_item_key.take())).await.unwrap();
                        }

                        self.input.clear();
                        self.reset_cursor();
                        self.restore_view();
                    }
                    KeyCode::Char(to_insert) => self.enter_char(to_insert),
                    KeyCode::Backspace => self.delete_char(),
                    KeyCode::Delete => {
                        self.move_cursor_right();
                        self.delete_char();
                    }
                    KeyCode::Left => self.move_cursor_left(),
                    KeyCode::Right => self.move_cursor_right(),
                    KeyCode::Home => self.move_cursor_home(),
                    KeyCode::End => self.move_cursor_end(),
                    KeyCode::Esc => {
                        self.pending_item_key = None;
                        self.input.clear();
                        self.reset_cursor();
                        self.restore_view();
                    }
                    _ => (),
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
                if let Some((end_point, _)) = self.zones.get_selected_item() {
                    self.to_roon.send(IoEvent::ZoneSelected(end_point.to_owned())).await.unwrap();
                }

                self.restore_view();
            }
            KeyCode::Delete => {
                if let Some((EndPoint::Preset(preset), _)) = self.zones.get_selected_item() {
                    self.to_roon.send(IoEvent::ZoneDeletePreset(preset.to_owned())).await.unwrap();
                }
            }
            KeyCode::Esc => self.restore_view(),
            _ => (),
        }
    }

    async fn handle_grouping_key_codes(&mut self, key: KeyEvent) -> Option<()> {
        match key.code {
            KeyCode::Up => self.grouping.prev(),
            KeyCode::Down => self.grouping.next(),
            KeyCode::Home => self.grouping.select_first(),
            KeyCode::End => self.grouping.select_last(),
            KeyCode::PageUp => self.grouping.select_prev_page(),
            KeyCode::PageDown => self.grouping.select_next_page(),
            KeyCode::Char(' ') => {
                let item = self.grouping.get_selected_item_mut()?;
                item.2 = !item.2;

                let items = self.grouping.items.as_ref()?;
                let output_ids = self.get_included_output_ids(items);

                if output_ids.len() > 1 {
                    self.draft_match = true;
                    self.to_roon.send(IoEvent::ZoneMatchPreset(output_ids)).await.unwrap();
                }
            }
            KeyCode::Enter => {
                self.restore_view();

                let items = self.grouping.items.as_ref()?;
                let output_ids = self.get_included_output_ids(items);

                if !output_ids.is_empty() {
                    if !self.input.is_empty() {
                        if output_ids.len() > 1 {
                            self.to_roon.send(IoEvent::ZoneSavePreset(self.input.to_owned(), output_ids)).await.unwrap();
                        } else {
                            self.to_roon.send(IoEvent::ZoneGrouped(output_ids)).await.unwrap();
                        }

                        self.input.clear();
                        self.reset_cursor();
                    } else {
                        self.to_roon.send(IoEvent::ZoneGrouped(output_ids)).await.unwrap();
                    }
                }
            }
            KeyCode::Char('s') => {
                self.save_preset();
            }
            KeyCode::Esc => {
                self.restore_view();
            }
            _ => (),
        }

        Some(())
    }

    async fn handle_preset_key_codes(&mut self, key: KeyEvent) {
        match key.modifiers {
            KeyModifiers::SHIFT => {
                if let KeyCode::Char(to_insert) = key.code {
                    self.enter_char(to_insert);
                }
            }
            KeyModifiers::NONE => {
                match key.code {
                    KeyCode::Enter => {
                        // Restore the grouping view, keeping the provided input string
                        self.selected_view = Some(View::Grouping);
                        self.grouping.select(Some(0));
                    }
                    KeyCode::Char(to_insert) => self.enter_char(to_insert),
                    KeyCode::Backspace => self.delete_char(),
                    KeyCode::Delete => {
                        self.move_cursor_right();
                        self.delete_char();
                    }
                    KeyCode::Left => self.move_cursor_left(),
                    KeyCode::Right => self.move_cursor_right(),
                    KeyCode::Home => self.move_cursor_home(),
                    KeyCode::End => self.move_cursor_end(),
                    KeyCode::Esc => {
                        self.selected_view = Some(View::Grouping);
                        self.input.clear();
                        self.reset_cursor();
                        self.grouping.select(Some(0));
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }

    fn save_preset(&mut self) -> Option<()> {
        let items = self.grouping.items.as_ref()?;
        let output_ids = self.get_included_output_ids(items);

        if output_ids.len() > 1 {
            if let Some(preset)  = self.matched_draft_preset.as_deref() {
                self.input = preset.to_owned();
                self.cursor_position = self.input.len();
            }

            self.selected_view = Some(View::GroupingPreset);
            self.grouping.deselect();
        }

        Some(())
    }

    fn get_item_key(&self) -> Option<String> {
        self.browse.get_selected_item()?.item_key.to_owned()
    }

    fn get_queue_item_id(&self) -> Option<u32> {
        Some(self.queue.get_selected_item()?.queue_item_id)
    }

    fn get_included_output_ids(&self, items: &[(String, String, bool)]) -> Vec<String> {
        items.iter()
            .filter_map(|(output_id, _, included)| {
                if *included {
                    Some(output_id.to_owned())
                } else {
                    None
                }
            })
            .collect()
    }
}
