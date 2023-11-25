use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::{collections::HashMap, fs, path};
use std::sync::Arc;
use tokio::{sync::{mpsc::{Receiver, Sender}, Mutex}, time::{Duration, sleep}, select};

use roon_api::{
    info,
    browse::{Action, Browse, BrowseOpts, LoadOpts},
    CoreEvent,
    Info,
    Parsed,
    RoonApi,
    Services,
    Svc,
    transport::{Control, Output, QueueItem, Seek, State, Transport, volume, Zone},
};

use super::{IoEvent, QueueMode};

const TUI_BROWSE: &str = "tui_browse";
const QUEUE_ITEM_COUNT: u32 = 100;

pub struct Options {
    pub config: String,
    pub ip: Option<String>,
    pub port: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Settings {
    zone_id: Option<String>,
    profile: Option<String>,
    queue_modes: Option<HashMap<String, QueueMode>>,
}

struct RoonHandler {
    to_app: Sender<IoEvent>,
    config_path: Arc<String>,
    primary_browse_key: &'static str,
    settings: Settings,
    browse: Option<Browse>,
    transport: Option<Transport>,
    zone_map: HashMap<String, Zone>,
    zone_output_ids: Option<Vec<String>>,
    pause_on_track_end: bool,
    browse_paths: HashMap<String, Vec<&'static str>>,
    profiles: Option<Vec<(String, String)>>,
    queue_end: Option<QueueItem>,
    seek_seconds: Option<i32>,
    opts: BrowseOpts,
}

pub async fn start(options: Options, to_app: Sender<IoEvent>, from_app: Receiver<IoEvent>) {
    let config_path = options.config;
    let ip = options.ip;
    let port = options.port;
    let path = path::Path::new(&config_path);

    fs::create_dir_all(path.parent().unwrap()).unwrap();

    let config_path = Arc::new(config_path);
    let from_app = Arc::new(Mutex::new(from_app));
    let info = info!("com.theappgineer", "Roon TUI");
    let mut roon = RoonApi::new(info);

    tokio::spawn(async move {
        loop {
            let services = Some(vec![
                Services::Browse(Browse::new()),
                Services::Transport(Transport::new()),
            ]);
            let provided: HashMap<String, Svc> = HashMap::new();
            let config_path_clone = config_path.clone();
            let get_roon_state = move || {
                RoonApi::load_config(&config_path_clone, "roonstate")
            };
            let result = match ip.as_deref() {
                Some(ip) => {
                    let ip = &IpAddr::V4(Ipv4Addr::from_str(ip).unwrap());
                    let port = &port;

                    roon.ws_connect(Box::new(get_roon_state), provided, services, ip, port).await
                }
                None => {
                    roon.start_discovery(Box::new(get_roon_state), provided, services).await
                }
            };

            if let Some((mut handlers, mut core_rx)) = result {
                let config_path = config_path.clone();
                let to_app = to_app.clone();
                let from_app = from_app.clone();

                handlers.spawn(async move {
                    let mut roon_handler = RoonHandler::new(to_app, config_path, TUI_BROWSE);

                    loop {
                        let mut from_app = from_app.lock().await;

                        select! {
                            Some((core_event, msg)) = core_rx.recv() => {
                                roon_handler.handle_core_event(core_event).await;

                                if let Some((msg, parsed)) = msg {
                                    roon_handler.handle_msg_event(msg, parsed).await;
                                }
                            }
                            Some(io_event) = from_app.recv() => {
                                roon_handler.handle_io_event(io_event).await;
                            }
                        };
                    }
                });

                handlers.join_next().await;
            }

            sleep(Duration::from_secs(10)).await;
        }
    });
}

impl RoonHandler {
    fn new(to_app: Sender<IoEvent>, config_path: Arc<String>, primary_browse_key: &'static str) -> Self {
        let settings: Settings = serde_json::from_value(RoonApi::load_config(&config_path, "settings")).unwrap_or_default();
        let opts = BrowseOpts {
            multi_session_key: Some(primary_browse_key.to_owned()),
            ..Default::default()
        };

        Self {
            to_app,
            config_path,
            primary_browse_key,
            settings,
            browse: None,
            transport: None,
            zone_map: HashMap::new(),
            zone_output_ids: None,
            pause_on_track_end: false,
            browse_paths: HashMap::new(),
            profiles: None,
            queue_end: None,
            seek_seconds: None,
            opts,
        }
    }

    async fn handle_core_event(&mut self, core_event: CoreEvent) -> Option<()> {
        match core_event {
            CoreEvent::Found(mut core) => {
                log::info!("Roon Server found: {}, version {}", core.display_name, core.display_version);

                self.browse = core.get_browse().cloned();
                self.transport = core.get_transport().cloned();

                let browse = self.browse.as_ref()?;
                let transport = self.transport.as_ref()?;

                self.opts.pop_all = true;

                browse.browse(&self.opts).await;

                transport.subscribe_zones().await;

                if let Some(zone_id) = self.settings.zone_id.as_ref() {
                    transport.subscribe_queue(&zone_id, QUEUE_ITEM_COUNT).await;
                }

                self.to_app.send(IoEvent::CoreName(Some(core.display_name))).await.unwrap();
            }
            CoreEvent::Lost(core) => {
                log::warn!("Roon Server lost: {}, version {}", core.display_name, core.display_version);
                self.to_app.send(IoEvent::CoreName(None)).await.unwrap();
            }
            _ => ()
        }

        Some(())
    }

    async fn handle_msg_event(&mut self, msg: Value, parsed: Parsed) -> Option<()> {
        match parsed {
            Parsed::RoonState => {
                RoonApi::save_config(&self.config_path, "roonstate", msg).unwrap();
            }
            Parsed::Zones(zones) => {
                if let Some(output_ids) = self.zone_output_ids.take() {
                    self.settings.zone_id = zones.iter()
                        .find_map(|zone| {
                            let count = zone.outputs.iter()
                                .filter(|output| output_ids.contains(&output.output_id))
                                .count();

                            if count == output_ids.len() {
                                Some(zone.zone_id.to_owned())
                            } else {
                                None
                            }
                        });

                    let settings = self.settings.serialize(serde_json::value::Serializer).unwrap();
                    RoonApi::save_config(&self.config_path, "settings", settings).unwrap();

                    if let Some(transport) = self.transport.as_ref() {
                        // Force full refresh of zone data
                        transport.get_zones().await;
                    }
                }

                for zone in zones {
                    self.zone_map.insert(zone.zone_id.to_owned(), zone);
                }

                let mut zones: Vec<(String, String)> = self.zone_map
                    .iter()
                    .map(|(zone_id, zone)| {
                        (zone_id.to_owned(), zone.display_name.to_owned())
                    })
                    .collect();
                zones.sort_by(|a, b| a.1.cmp(&b.1));

                self.to_app.send(IoEvent::Zones(zones)).await.unwrap();

                self.sync_queue_mode().await;

                if let Some(zone_id) = self.settings.zone_id.as_deref() {
                    if let Some(browse_path) = self.browse_profile(zone_id).await {
                        self.browse_paths.insert(zone_id.to_owned(), browse_path);
                    }

                    if let Some(zone) = self.zone_map.get(zone_id).cloned() {
                        if zone.state != State::Playing {
                            if self.pause_on_track_end {
                                self.pause_on_track_end = false;
                                self.to_app.send(IoEvent::PauseOnTrackEndActive(self.pause_on_track_end)).await.unwrap();
                            }
                        } else {
                            let seek_seconds = self.seek_seconds.take();
                            self.seek_to_end(Some(zone_id), seek_seconds).await;
                        }

                        let settings = self.settings.serialize(serde_json::value::Serializer).unwrap();
                        RoonApi::save_config(&self.config_path, "settings", settings).unwrap();

                        self.to_app.send(IoEvent::ZoneChanged(zone)).await.unwrap();
                    }
                }
            }
            Parsed::ZonesRemoved(zone_ids) => {
                if let Some(zone_id) = self.settings.zone_id.as_ref() {
                    if zone_ids.contains(zone_id) {
                        self.to_app.send(IoEvent::ZoneRemoved(zone_id.to_owned())).await.unwrap();
                    }
                }

                for zone_id in zone_ids {
                    self.zone_map.remove(&zone_id);
                }
            }
            Parsed::ZonesSeek(seeks) => {
                if let Some(zone_id) = self.settings.zone_id.as_deref() {
                    if let Some(index) = seeks.iter().position(|seek| seek.zone_id == *zone_id) {
                        let seek = seeks[index].to_owned();

                        if let Some(seek_position) = seek.seek_position {
                            if seek_position == 0 && self.pause_on_track_end {
                                self.control(zone_id, &Control::Pause).await;
                                self.pause_on_track_end = false;
                                self.to_app.send(IoEvent::PauseOnTrackEndActive(self.pause_on_track_end)).await.unwrap();
                            }
                        }

                        self.to_app.send(IoEvent::ZoneSeek(seek)).await.unwrap();
                    }
                }

                for seek in seeks {
                    if seek.queue_time_remaining >= 0 && seek.queue_time_remaining <= 3 {
                        let zone = self.zone_map.get(&seek.zone_id);

                        if let Some(browse_path) = self.handle_queue_mode(zone, true).await {
                            self.browse_paths.insert(seek.zone_id, browse_path);
                        }
                    }
                };
            }
            Parsed::Queue(queue_items) => {
                self.to_app.send(IoEvent::QueueList(queue_items)).await.unwrap();
            },
            Parsed::QueueChanges(queue_changes) => {
                self.to_app.send(IoEvent::QueueListChanges(queue_changes)).await.unwrap();
            }
            Parsed::Outputs(outputs) => {
                if let Some(zone_id) = self.settings.zone_id.as_deref() {
                    let zone = self.zone_map.get(zone_id);
                    let grouping = Self::get_grouping(zone, &outputs);

                    self.to_app.send(IoEvent::ZoneGrouping(grouping)).await.unwrap();
                }
            }
            Parsed::BrowseResult(result, multi_session_key) => {
                match result.action {
                    Action::List => {
                        let list = result.list?;
                        let multi_session_str = multi_session_key.as_deref()?;
                        let mut opts = LoadOpts::default();

                        if multi_session_str == self.primary_browse_key {
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

                        self.browse.as_ref()?.load(&opts).await;
                    }
                    Action::Message => {
                        let is_error = result.is_error.unwrap();
                        let message = result.message.unwrap();

                        if is_error && message == "Zone is not configured" {
                            self.to_app.send(IoEvent::ZoneSelect).await.unwrap();
                        }
                    }
                    _ => (),
                }
            }
            Parsed::LoadResult(result, multi_session_key) => {
                let multi_session_str = multi_session_key.as_deref()?;

                if multi_session_str == self.primary_browse_key {
                    let new_offset = result.offset + result.items.len();

                    if new_offset < result.list.count {
                        // There are more items to load
                        let opts = LoadOpts {
                            offset: new_offset,
                            set_display_offset: new_offset,
                            multi_session_key,
                            ..Default::default()
                        };

                        self.browse.as_ref()?.load(&opts).await;
                    }

                    self.profiles = if result.list.title == "Profile" {
                        Some(result.items.iter().filter_map(|item| {
                            Some((item.item_key.as_ref()?.clone(), item.title.clone()))
                        }).collect())
                    } else {
                        None
                    };

                    self.to_app.send(IoEvent::BrowseList(result.offset, result.items)).await.unwrap();
                } else {
                    let browse_path = self.browse_paths.get_mut(multi_session_str)?;
                    let step = browse_path.pop()?;

                    if browse_path.is_empty() {
                        self.browse_paths.remove(multi_session_str);
                    }

                    let item = if step.is_empty() {
                        if result.list.title == "Profile" {
                            let profile = self.settings.profile.as_deref();

                            result.items.iter().find_map(|item| if item.title == profile? {Some(item)} else {None})
                        } else {
                            result.items.iter().next()
                        }
                    } else {
                        result.items.iter().find_map(|item| if item.title == step {Some(item)} else {None})
                    };

                    let opts = BrowseOpts {
                        zone_or_output_id: multi_session_key.clone(),
                        item_key: item?.item_key.clone(),
                        multi_session_key,
                        ..Default::default()
                    };

                    self.browse.as_ref()?.browse(&opts).await;
                }
            }
            _ => (),
        }

        Some(())
    }

    async fn handle_io_event(&mut self, io_event: IoEvent) -> Option<()> {
        let browse = self.browse.as_ref()?;

        // Only one of item_key, pop_all, pop_levels, and refresh_list may be populated
        self.opts.item_key = None;
        self.opts.pop_all = false;
        self.opts.pop_levels = None;
        self.opts.refresh_list = false;

        match io_event {
            IoEvent::BrowseSelected(item_key) => {
                let profile = self.get_profile_name(item_key.as_deref());

                if profile.is_some() {
                    if let Some(zone_id) = self.settings.zone_id.as_ref() {
                        self.settings.profile = profile;

                        let settings = self.settings.serialize(serde_json::value::Serializer).unwrap();
                        RoonApi::save_config(&self.config_path, "settings", settings).unwrap();

                        if let Some(browse_path) = self.browse_profile(zone_id).await {
                            self.browse_paths.insert(zone_id.to_owned(), browse_path);
                        }
                    }
                }

                self.opts.item_key = item_key;
                self.opts.zone_or_output_id = self.settings.zone_id.to_owned();

                browse.browse(&self.opts).await;

                self.opts.input = None;
            }
            IoEvent::BrowseBack => {
                self.opts.pop_levels = Some(1);

                browse.browse(&self.opts).await;
            }
            IoEvent::BrowseRefresh => {
                self.opts.refresh_list = true;

                browse.browse(&self.opts).await;
            }
            IoEvent::BrowseHome => {
                self.opts.pop_all = true;

                browse.browse(&self.opts).await;
            }
            IoEvent::BrowseInput(input) => {
                self.opts.input = Some(input);

                browse.browse(&self.opts).await;
            }
            IoEvent::QueueListLast(item) => self.queue_end = item,
            IoEvent::QueueSelected(queue_item_id) => {
                if let Some(transport) = self.transport.as_ref() {
                    if let Some(zone_id) = self.settings.zone_id.as_ref() {
                        transport.play_from_here(zone_id, queue_item_id).await;
                    }
                }
            }
            IoEvent::QueueClear => {
                self.seek_seconds = self.play_queue_end().await;
            }
            IoEvent::QueueModeNext => {
                if let Some(queue_mode) = self.select_next_queue_mode().await {
                    let auto_radio = match queue_mode {
                        QueueMode::RoonRadio => true,
                        _ => false,
                    };

                    self.set_roon_radio(auto_radio).await.unwrap();

                    let settings = self.settings.serialize(serde_json::value::Serializer).unwrap();
                    RoonApi::save_config(&self.config_path, "settings", settings).unwrap();
                }
            }
            IoEvent::QueueModeAppend => {
                if let Some(zone_id) = self.settings.zone_id.as_deref() {
                    let zone = self.zone_map.get(zone_id);

                    if let Some(browse_path) = self.handle_queue_mode(zone, false).await {
                        self.browse_paths.insert(zone_id.to_owned(), browse_path);
                    }
                }
            }
            IoEvent::ZoneSelected(zone_id) => {
                if let Some(transport) = self.transport.as_ref() {
                    transport.unsubscribe_queue().await;
                    transport.subscribe_queue(&zone_id, QUEUE_ITEM_COUNT).await;

                    if let Some(browse_path) = self.browse_profile(&zone_id).await {
                        self.browse_paths.insert(zone_id.to_owned(), browse_path);
                    }

                    if let Some(zone) = self.zone_map.get(&zone_id) {
                        self.to_app.send(IoEvent::ZoneChanged(zone.to_owned())).await.unwrap();
                    }

                    // Store the zone_id in settings before it is used again in sync_queue_mode
                    self.settings.zone_id = Some(zone_id);

                    self.sync_queue_mode().await;

                    let settings = self.settings.serialize(serde_json::value::Serializer).unwrap();
                    RoonApi::save_config(&self.config_path, "settings", settings).unwrap();
                }
            }
            IoEvent::ZoneGroupReq => {
                if let Some(transport) = self.transport.as_ref() {
                    transport.get_outputs().await;
                }
            }
            IoEvent::ZoneGrouped(output_ids) => {
                self.zone_output_ids = self.update_grouping(output_ids).await;
            }
            IoEvent::Mute(how) => {
                self.mute(&how).await;
            }
            IoEvent::ChangeVolume(steps) => {
                self.change_volume(steps).await;
            }
            IoEvent::Control(how) => {
                if let Some(zone_id) = self.settings.zone_id.as_deref() {
                    let zone_option = self.zone_map.get(zone_id);

                    if let Some(zone) = zone_option {
                        if zone.now_playing.is_some() {
                            self.control(zone_id, &how).await;
                        } else if how == Control::PlayPause {
                            if let Some(browse_path) = self.handle_queue_mode(
                                zone_option,
                                true,
                            ).await {
                                self.browse_paths.insert(zone_id.to_owned(), browse_path);
                            }
                        }
                    }
                }
            }
            IoEvent::PauseOnTrackEndReq => {
                self.pause_on_track_end = self.handle_pause_on_track_end_req().unwrap_or_default();
                self.to_app.send(IoEvent::PauseOnTrackEndActive(self.pause_on_track_end)).await.unwrap();
            }
            _ => (),
        }

        Some(())
    }

    fn get_grouping<'a>(zone: Option<&Zone>, outputs: &Vec<Output>) -> Option<Vec<(String, String, bool)>> {
        let mut grouping: Vec<(String, String, bool)> = zone?.outputs.iter()
            .map(|output| (output.output_id.to_owned(), output.display_name.to_owned(), true))
            .collect();
        let can_group_with_output_ids = &zone?.outputs.get(0)?.can_group_with_output_ids;

        for output in outputs {
            if can_group_with_output_ids.contains(&output.output_id) {
                let is_not_in = grouping.iter()
                    .position(|(output_id, _, _)| *output_id == output.output_id)
                    .is_none();

                if is_not_in {
                    grouping.push((output.output_id.to_owned(), output.display_name.to_owned(), false));
                }
            }
        }

        Some(grouping)
    }

    fn handle_pause_on_track_end_req(&self) -> Option<bool> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let zone = self.zone_map.get(zone_id)?;
        let now_playing_length = zone.now_playing.as_ref()?.length?;

        Some(zone.state == State::Playing && now_playing_length > 0)
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

    async fn browse_profile(&self, zone_id: &str) -> Option<Vec<&'static str>> {
        let opts = BrowseOpts {
            multi_session_key: Some(zone_id.to_owned()),
            ..Default::default()
        };

        self.browse.as_ref()?.browse(&opts).await;

        Some(vec!["", "Profile", "Settings"])
    }

    async fn sync_queue_mode(&mut self) -> Option<()> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let zone = self.zone_map.get(zone_id)?;
        let output_id = zone.outputs.get(0)?.output_id.as_str();

        if let Some(queue_modes) = self.settings.queue_modes.as_mut() {
            if let Some(queue_mode) = queue_modes.remove(zone_id) {
                queue_modes.insert(output_id.to_owned(), queue_mode);
            }
        }

        let queue_mode = if zone.settings.auto_radio {
            QueueMode::RoonRadio
        } else if let Some(queue_mode) = self.settings.queue_modes.as_ref()?.get(output_id) {
            if *queue_mode == QueueMode::RoonRadio {
                QueueMode::default()
            } else {
                queue_mode.to_owned()
            }
        } else {
            QueueMode::default()
        };

        self.to_app.send(IoEvent::QueueModeCurrent(queue_mode.to_owned())).await.unwrap();
        self.settings.queue_modes.as_mut()?.insert(output_id.to_owned(), queue_mode);

        Some(())
    }

    async fn select_next_queue_mode<'a>(&'a mut self) -> Option<&'a QueueMode> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let output_id = self.zone_map.get(zone_id)?.outputs.get(0)?.output_id.as_str();

        if self.settings.queue_modes.is_none() {
            self.settings.queue_modes = Some(HashMap::new());
        }

        let queue_modes = self.settings.queue_modes.as_mut()?;

        let queue_mode = if queue_modes.get(output_id).is_none() {
            queue_modes.insert(output_id.to_owned(), QueueMode::Manual);

            &QueueMode::Manual
        } else {
            let queue_mode = queue_modes.get_mut(output_id)?;
            let index = queue_mode.to_owned() as usize + 1;
            let seq = if self.settings.profile.is_none() {
                vec![
                    QueueMode::Manual,
                    QueueMode::RoonRadio,
                ]
            } else {
                vec![
                    QueueMode::Manual,
                    QueueMode::RoonRadio,
                    QueueMode::RandomAlbum,
                    QueueMode::RandomTrack,
                ]
            };

            *queue_mode = match seq.get(index) {
                None => QueueMode::Manual,
                Some(queue_mode) => queue_mode.to_owned(),
            };

            queue_mode
        };

        self.to_app.send(IoEvent::QueueModeCurrent(queue_mode.to_owned())).await.unwrap();

        Some(queue_mode)
    }

    async fn handle_queue_mode(
        &self,
        zone: Option<&Zone>,
        play: bool,
    ) -> Option<Vec<&'static str>> {
        let zone = zone?;
        let zone_id = zone.zone_id.as_str();
        let output_id = zone.outputs.get(0)?.output_id.as_str();
        let queue_mode = self.settings.queue_modes.as_ref()?.get(output_id)?;

        if play {
            if let Some(now_playing) = zone.now_playing.as_ref() {
                now_playing.length?;
            }
        }

        let play_action = if play {"Play Now"} else {"Queue"};

        match queue_mode {
            QueueMode::RandomAlbum => {
                let opts = BrowseOpts {
                    pop_all: true,
                    multi_session_key: Some(zone_id.to_owned()),
                    ..Default::default()
                };

                self.browse.as_ref()?.browse(&opts).await;

                Some(vec![play_action, "Play Album", "", "Albums", "Library"])
            }
            QueueMode::RandomTrack => {
                let opts = BrowseOpts {
                    pop_all: true,
                    multi_session_key: Some(zone_id.to_owned()),
                    ..Default::default()
                };

                self.browse.as_ref()?.browse(&opts).await;

                Some(vec![play_action, "", "Tracks", "Library"])
            }
            _ => None,
        }
    }

    async fn mute(&self, how: &volume::Mute) -> Option<Vec<usize>> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let zone = self.zone_map.get(zone_id)?;
        let mut req_ids = Vec::new();

        for output in &zone.outputs {
            req_ids.push(self.transport.as_ref()?.mute(&output.output_id, how).await?);
        }

        Some(req_ids)
    }

    async fn change_volume(&self, steps: i32) -> Option<Vec<usize>> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let zone = self.zone_map.get(zone_id)?;
        let mut req_ids = Vec::new();

        for output in &zone.outputs {
            req_ids.push(self.transport.as_ref()?.change_volume(
                &output.output_id,
                &volume::ChangeMode::RelativeStep, steps
            ).await?);
        }

        Some(req_ids)
    }

    async fn control(&self, zone_id: &str, how: &Control) -> Option<usize> {
        let zone = self.zone_map.get(zone_id)?;

        match how {
            Control::Next => zone.is_next_allowed.then_some(())?,
            Control::Previous => zone.is_previous_allowed.then_some(())?,
            _ => ()
        }

        self.transport.as_ref()?.control(&zone.zone_id, how).await
    }

    async fn seek_to_end(&self, zone_id: Option<&str>, seek_seconds: Option<i32>) -> Option<()> {
        self.transport.as_ref()?.seek(zone_id?, &Seek::Absolute, seek_seconds?).await;

        Some(())
    }

    async fn play_queue_end(&self) -> Option<i32> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let queue_end = self.queue_end.as_ref()?;

        self.transport.as_ref()?.play_from_here(zone_id, queue_end.queue_item_id).await;

        Some(queue_end.length as i32)
    }

    async fn set_roon_radio(&self, auto_radio: bool) -> Option<usize> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let mut settings = self.zone_map.get(zone_id)?.settings.clone();

        settings.auto_radio = auto_radio;
        self.transport.as_ref()?.change_settings(zone_id, settings).await
    }

    async fn update_grouping(&self, new_ids: Vec<String>) -> Option<Vec<String>> {
        let zone_id = self.settings.zone_id.as_deref()?;
        let zone = self.zone_map.get(zone_id)?;
        let current_ids: Vec<&str> = zone.outputs.iter()
            .map(|output| output.output_id.as_str())
            .collect();
        let output_ids: Vec<&str> = new_ids.iter()
            .map(|output_id| output_id.as_str())
            .collect();
        let matches_all = output_ids.len() == current_ids.len()
            && output_ids.get(0) == current_ids.get(0)
            && output_ids.iter()
                .all(|output_id| current_ids.contains(output_id));

        if matches_all {
            None
        } else {
            self.transport.as_ref()?.ungroup_outputs(current_ids).await;

            if output_ids.len() > 1 {
                self.transport.as_ref()?.group_outputs(output_ids).await;
            }

            Some(new_ids)
        }
    }
}
