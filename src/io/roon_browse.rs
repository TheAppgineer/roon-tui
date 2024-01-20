use rand::Rng;
use roon_api::{
    browse::{Action, Browse, BrowseOpts, LoadOpts},
    Parsed,
};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;

use super::{IoEvent, QueueMode, roon_settings::{RoonSettings, QueueAction}};

const TUI_BROWSE: &str = "tui_browse";

pub struct RoonBrowse {
    to_app: Sender<IoEvent>,
    browse: Browse,
    browse_reached_home: bool,
    browse_paths: HashMap<String, Vec<&'static str>>,
    profiles: Option<Vec<(String, String)>>,
    opts: BrowseOpts,
}

impl RoonBrowse {
    pub async fn new(browse: Browse, to_app: Sender<IoEvent>) -> RoonBrowse {
        let opts = BrowseOpts {
            multi_session_key: Some(TUI_BROWSE.to_owned()),
            pop_all: true,
            ..Default::default()
        };

        browse.browse(&opts).await;

        Self {
            to_app,
            browse,
            browse_reached_home: false,
            browse_paths: HashMap::new(),
            profiles: None,
            opts,
        }
    }

    pub async fn handle_msg_event(
        &mut self,
        parsed: Parsed,
        profile: Option<&str>,
        no_active_zones: bool,
    ) -> Option<()> {
        match parsed {
            Parsed::BrowseResult(result, multi_session_key) => {
                match result.action {
                    Action::List => {
                        let list = result.list?;
                        let multi_session_str = multi_session_key.as_deref()?;
                        let mut opts = LoadOpts::default();

                        if multi_session_str == TUI_BROWSE {
                            let offset = list.display_offset.unwrap_or_default();

                            opts.offset = offset;
                            opts.set_display_offset = offset;

                            self.to_app.send(IoEvent::BrowseTitle(list.title)).await.unwrap();
                        } else if list.title == "Albums" || list.title == "Tracks" {
                            let mut rng = rand::thread_rng();
                            let offset = rng.gen_range(0..list.count);

                            opts.count = Some(1);
                            opts.offset = offset;
                            opts.set_display_offset = offset;
                        }

                        opts.multi_session_key = multi_session_key;

                        self.browse.load(&opts).await;
                    }
                    Action::Message => {
                        let is_error = result.is_error.unwrap();
                        let message = result.message.unwrap();

                        if is_error && message == "Zone is not configured" && no_active_zones {
                            // Drop the saved item_key as there are no active zones
                            self.opts.item_key = None;

                            self.to_app.send(IoEvent::ZoneSelect).await.unwrap();
                        }
                    }
                    _ => (),
                }
            }
            Parsed::LoadResult(result, multi_session_key) => {
                let multi_session_str = multi_session_key.as_deref()?;

                if multi_session_str == TUI_BROWSE {
                    let new_offset = result.offset + result.items.len();

                    if new_offset < result.list.count {
                        // There are more items to load
                        let opts = LoadOpts {
                            offset: new_offset,
                            set_display_offset: new_offset,
                            multi_session_key,
                            ..Default::default()
                        };

                        self.browse.load(&opts).await;
                    }

                    self.profiles = if result.list.title == "Profile" {
                        Some(result.items.iter().filter_map(|item| {
                            Some((item.item_key.as_ref()?.clone(), item.title.clone()))
                        }).collect())
                    } else {
                        None
                    };

                    self.browse_reached_home = result.list.level == 0;
                    self.to_app.send(IoEvent::BrowseList(result.offset, result.items)).await.unwrap();
                } else {
                    let browse_path = self.browse_paths.get_mut(multi_session_str)?;
                    let step = browse_path.pop()?;

                    if browse_path.is_empty() {
                        self.browse_paths.remove(multi_session_str);
                    }

                    let item = if step.is_empty() {
                        if result.list.title == "Profile" {
                            result.items.iter().find_map(|item| if item.title == profile? {Some(item)} else {None})
                        } else {
                            result.items.first()
                        }
                    } else {
                        result.items.iter().find(|item| item.title == step)
                    };

                    let opts = BrowseOpts {
                        zone_or_output_id: multi_session_key.clone(),
                        item_key: item?.item_key.clone(),
                        multi_session_key,
                        ..Default::default()
                    };

                    self.browse.browse(&opts).await;
                }
            }
            _ => (),
        }

        Some(())
    }

    pub async fn handle_io_event(&mut self, io_event: IoEvent, settings: &mut RoonSettings) -> Option<bool> {
        let mut has_changed = false;

        // Only one of item_key, pop_all, pop_levels, and refresh_list may be populated
        self.opts.item_key = None;
        self.opts.pop_all = false;
        self.opts.pop_levels = None;
        self.opts.refresh_list = false;

        match io_event {
            IoEvent::BrowseSelected(item_key) => {
                let profile = self.get_profile_name(item_key.as_deref());

                if profile.is_some() {
                    if let Some(zone_id) = settings.get_zone_id() {
                        let zone_id = zone_id.to_owned();

                        settings.set_profile(profile);
                        has_changed = true;

                        self.browse_profile(&zone_id).await;
                    }
                }

                self.opts.item_key = item_key;
                self.browse.browse(&self.opts).await;

                self.opts.input = None;
            }
            IoEvent::BrowseBack => {
                if !self.browse_reached_home {
                    self.opts.pop_levels = Some(1);

                    self.browse.browse(&self.opts).await;
                }
            }
            IoEvent::BrowseRefresh => {
                self.opts.refresh_list = true;

                self.browse.browse(&self.opts).await;
            }
            IoEvent::BrowseHome => {
                self.opts.pop_all = true;

                self.browse.browse(&self.opts).await;
            }
            IoEvent::BrowseInput(input) => {
                self.opts.input = Some(input);

                self.browse.browse(&self.opts).await;
            }
            _ => (),
        }

        Some(has_changed)
    }

    pub fn set_zone_id(&mut self, zone_id: Option<String>) {
        self.opts.zone_or_output_id = zone_id;
    }

    pub async fn browse_profile(&mut self, zone_id: &str) {
        let opts = BrowseOpts {
            multi_session_key: Some(zone_id.to_owned()),
            ..Default::default()
        };

        self.browse.browse(&opts).await;
        self.browse_paths.insert(zone_id.to_owned(), vec!["", "Profile", "Settings"]);
    }

    pub async fn handle_queue_mode(&mut self, zone_id: &str, queue_mode: &QueueMode, queue_action: QueueAction) {
        let queue_action = match queue_action {
            QueueAction::PlayNow => "Play Now",
            QueueAction::AddNext => "Add Next",
            QueueAction::Queue => "Queue",
        };

        match queue_mode {
            QueueMode::RandomAlbum => {
                let browse_path = vec![queue_action, "Play Album", "", "Albums", "Library"];
                let opts = BrowseOpts {
                    pop_all: true,
                    multi_session_key: Some(zone_id.to_owned()),
                    ..Default::default()
                };

                self.browse.browse(&opts).await;

                self.browse_paths.insert(zone_id.to_owned(), browse_path);
            }
            QueueMode::RandomTrack => {
                let browse_path = vec![queue_action, "", "Tracks", "Library"];
                let opts = BrowseOpts {
                    pop_all: true,
                    multi_session_key: Some(zone_id.to_owned()),
                    ..Default::default()
                };

                self.browse.browse(&opts).await;

                self.browse_paths.insert(zone_id.to_owned(), browse_path);
            }
            _ => (),
        }
    }

    fn get_profile_name(&self, item_key: Option<&str>) -> Option<String> {
        let profiles = self.profiles.as_ref()?;

        profiles.iter().find_map(|(key, title)| {
            if key == item_key? {
                Some(title.to_owned())
            } else {
                None
            }
        })
    }
}
