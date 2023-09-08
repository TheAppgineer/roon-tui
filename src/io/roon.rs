use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path};
use std::sync::Arc;
use tokio::{sync::mpsc, select};

use roon_api::{
    info,
    browse::{Action, Browse, BrowseOpts, Hierarchy, LoadOpts},
    CoreEvent,
    Info,
    LogLevel,
    Parsed,
    RoonApi,
    Services,
    Svc,
    transport::{Control, State, Transport, volume, Zone},
};

use super::{IoEvent, QueueMode};

const TUI_BROWSE: &str = "tui_browse";

#[derive(Debug, Default, Deserialize, Serialize)]
struct Settings {
    zone_id: Option<String>,
    profile: Option<String>,
    queue_mode: Option<HashMap<String, QueueMode>>,
}

pub async fn start(config_path: String, to_app: mpsc::Sender<IoEvent>, mut from_app: mpsc::Receiver<IoEvent>) {
    let path = path::Path::new(&config_path);
    let mut info = info!("com.theappgineer", "Roon TUI");

    info.set_log_level(LogLevel::None);
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    let mut roon = RoonApi::new(info);
    let services = vec![
        Services::Browse(Browse::new()),
        Services::Transport(Transport::new()),
    ];
    let provided: HashMap<String, Svc> = HashMap::new();
    let config_path = Arc::new(config_path);
    let config_path_clone = config_path.clone();
    let get_roon_state = move || {
        RoonApi::load_config(&config_path_clone, "roonstate")
    };
    let (_, mut core_rx) = roon
        .start_discovery(Box::new(get_roon_state), provided, Some(services)).await.unwrap();

    tokio::spawn(async move {
        const QUEUE_ITEM_COUNT: u32 = 100;
        let mut settings: Settings = serde_json::from_value(RoonApi::load_config(&config_path, "settings")).unwrap_or_default();
        let mut browse = None;
        let mut transport = None;
        let mut zone_map = HashMap::new();
        let mut pause_on_track_end = false;
        let mut browse_paths: HashMap<String, Vec<&str>> = HashMap::new();
        let mut opts = BrowseOpts {
            multi_session_key: Some(TUI_BROWSE.to_owned()),
            ..Default::default()
        };

        loop {
            select! {
                Some((core_event, msg)) = core_rx.recv() => {
                    match core_event {
                        CoreEvent::Found(mut paired_core) => {
                            browse = paired_core.get_browse().cloned();
                            transport = paired_core.get_transport().cloned();

                            if let Some(browse) = browse.as_ref() {
                                opts.pop_all = true;

                                browse.browse(&opts).await;
                            }

                            if let Some(transport) = transport.as_ref() {
                                transport.subscribe_zones().await;

                                if let Some(zone_id) = settings.zone_id.as_ref() {
                                    transport.subscribe_queue(&zone_id, QUEUE_ITEM_COUNT).await;
                                }
                            }

                            let io_event = IoEvent::CoreName(Some(paired_core.display_name));
                            to_app.send(io_event).await.unwrap();
                        }
                        CoreEvent::Lost(_) => to_app.send(IoEvent::CoreName(None)).await.unwrap(),
                        _ => (),
                    }

                    if let Some((msg, parsed)) = msg {
                        match parsed {
                            Parsed::RoonState => {
                                RoonApi::save_config(&config_path, "roonstate", msg).unwrap();
                            }
                            Parsed::Zones(zones) => {
                                for zone in zones {
                                    zone_map.insert(zone.zone_id.to_owned(), zone);
                                }

                                let mut zones: Vec<(String, String)> = zone_map
                                    .iter()
                                    .map(|(zone_id, zone)| {
                                        (zone_id.to_owned(), zone.display_name.to_owned())
                                    })
                                    .collect();
                                zones.sort_by(|a, b| a.1.cmp(&b.1));

                                to_app.send(IoEvent::Zones(zones)).await.unwrap();

                                if let Some(zone_id) = settings.zone_id.as_ref() {
                                    if let Some(zone) = zone_map.get(zone_id) {
                                        if let Some(queue_mode) = sync_queue_mode(&mut settings, zone.settings.auto_radio) {
                                            to_app.send(IoEvent::QueueModeCurrent(queue_mode.to_owned())).await.unwrap();

                                            let settings = settings.serialize(serde_json::value::Serializer).unwrap();
                                            RoonApi::save_config(&config_path, "settings", settings.to_owned()).unwrap();
                                        }

                                        if zone.state != State::Playing && pause_on_track_end {
                                            pause_on_track_end = false;
                                            to_app.send(IoEvent::PauseOnTrackEndActive(pause_on_track_end)).await.unwrap();
                                        }

                                        to_app.send(IoEvent::ZoneChanged(zone.to_owned())).await.unwrap();
                                    }
                                }
                            }
                            Parsed::ZonesRemoved(zone_ids) => {
                                if let Some(zone_id) = settings.zone_id.as_ref() {
                                    if zone_ids.contains(zone_id) {
                                        to_app.send(IoEvent::ZoneRemoved(zone_id.to_owned())).await.unwrap();
                                    }
                                }

                                for zone_id in zone_ids {
                                    zone_map.remove(&zone_id);
                                }
                            }
                            Parsed::ZonesSeek(seeks) => {
                                if let Some(zone_id) = settings.zone_id.as_ref() {
                                    if let Some(index) = seeks.iter().position(|seek| seek.zone_id == *zone_id) {
                                        let seek = seeks[index].to_owned();

                                        if let Some(seek_position) = seek.seek_position {
                                            if seek_position == 0 && pause_on_track_end {
                                                control(transport.as_ref(), settings.zone_id.as_deref(), &Control::Pause).await;
                                                pause_on_track_end = false;
                                                to_app.send(IoEvent::PauseOnTrackEndActive(pause_on_track_end)).await.unwrap();
                                            }
                                        }

                                        to_app.send(IoEvent::ZoneSeek(seek)).await.unwrap();
                                    }
                                }

                                for seek in seeks {
                                    if seek.queue_time_remaining == 0 {
                                        let zone = zone_map.get(&seek.zone_id);

                                        if let Some(browse_path) = handle_queue_mode(settings.queue_mode.as_ref(), zone, browse.as_ref()).await {
                                            browse_paths.insert(seek.zone_id, browse_path);
                                        }
                                    }
                                };
                            }
                            Parsed::Queue(queue_items) => {
                                to_app.send(IoEvent::QueueList(queue_items)).await.unwrap();
                            },
                            Parsed::QueueChanges(queue_changes) => {
                                to_app.send(IoEvent::QueueListChanges(queue_changes)).await.unwrap();
                            }
                            _ => {
                                handle_browse_response(browse.as_ref(), &mut browse_paths, &to_app, parsed).await;
                            }
                        }
                    }
                }
                Some(event) = from_app.recv() => {
                    // Only one of item_key, pop_all, pop_levels, and refresh_list may be populated
                    opts.item_key = None;
                    opts.pop_all = false;
                    opts.pop_levels = None;
                    opts.refresh_list = false;

                    match event {
                        IoEvent::BrowseSelected(item_key) => {
                            if let Some(browse) = browse.as_ref() {
                                opts.item_key = item_key;
                                opts.zone_or_output_id = settings.zone_id.to_owned();

                                browse.browse(&opts).await;

                                opts.input = None;
                            }
                        }
                        IoEvent::BrowseBack => {
                            if let Some(browse) = browse.as_ref() {
                                opts.pop_levels = Some(1);

                                browse.browse(&opts).await;
                            }
                        }
                        IoEvent::BrowseRefresh => {
                            if let Some(browse) = browse.as_ref() {
                                opts.refresh_list = true;

                                browse.browse(&opts).await;
                            }
                        }
                        IoEvent::BrowseHome => {
                            if let Some(browse) = browse.as_ref() {
                                opts.pop_all = true;

                                browse.browse(&opts).await;
                            }
                        }
                        IoEvent::BrowseInput(input) => {
                            if let Some(browse) = browse.as_ref() {
                                opts.input = Some(input);

                                browse.browse(&opts).await;
                            }
                        }
                        IoEvent::QueueSelected(queue_item_id) => {
                            if let Some(transport) = transport.as_ref() {
                                if let Some(zone_id) = settings.zone_id.as_ref() {
                                    transport.play_from_here(zone_id, queue_item_id).await;
                                }
                            }
                        }
                        IoEvent::QueueModeNext => {
                            if let Some(queue_mode) = select_next_queue_mode(&mut settings) {
                                to_app.send(IoEvent::QueueModeCurrent(queue_mode.to_owned())).await.unwrap();

                                let auto_radio = match queue_mode {
                                    QueueMode::RoonRadio => true,
                                    _ => false,
                                };

                                set_roon_radio(transport.as_ref(), &zone_map, settings.zone_id.as_deref(), auto_radio).await.unwrap();

                                let settings = settings.serialize(serde_json::value::Serializer).unwrap();
                                RoonApi::save_config(&config_path, "settings", settings.to_owned()).unwrap();
                            }
                        }
                        IoEvent::ZoneSelected(zone_id) => {
                            if let Some(transport) = transport.as_ref() {
                                transport.unsubscribe_queue().await;
                                transport.subscribe_queue(&zone_id, QUEUE_ITEM_COUNT).await;

                                let zone = zone_map.get(&zone_id);

                                settings.zone_id = Some(zone_id);

                                if let Some(zone) = zone {
                                    if let Some(queue_mode) = sync_queue_mode(&mut settings, zone.settings.auto_radio) {
                                        to_app.send(IoEvent::QueueModeCurrent(queue_mode.to_owned())).await.unwrap();
                                    }

                                    to_app.send(IoEvent::ZoneChanged(zone.to_owned())).await.unwrap();
                                }

                                let settings = settings.serialize(serde_json::value::Serializer).unwrap();

                                RoonApi::save_config(&config_path, "settings", settings.to_owned()).unwrap();
                            }
                        }
                        IoEvent::Mute(how) => {
                            mute(transport.as_ref(), &zone_map, settings.zone_id.as_deref(), &how).await;
                        }
                        IoEvent::ChangeVolume(steps) => {
                            change_volume(transport.as_ref(), &zone_map, settings.zone_id.as_deref(), steps).await;
                        }
                        IoEvent::Control(how) => {
                            control(transport.as_ref(), settings.zone_id.as_deref(), &how).await;
                        }
                        IoEvent::PauseOnTrackEndReq => {
                            pause_on_track_end = handle_pause_on_track_end_req(&settings, &zone_map).unwrap_or_default();
                            to_app.send(IoEvent::PauseOnTrackEndActive(pause_on_track_end)).await.unwrap();
                        }
                        _ => (),
                    }
                }
            }
        }
    });
}

fn handle_pause_on_track_end_req(settings: &Settings, zone_map: &HashMap<String, Zone>) -> Option<bool> {
    let zone_id = settings.zone_id.as_ref()?;
    let zone = zone_map.get(zone_id)?;
    let now_playing_length = zone.now_playing.as_ref()?.length?;

    Some(zone.state == State::Playing && now_playing_length > 0)
}

async fn handle_browse_response(
    browse: Option<&Browse>,
    browse_paths: &mut HashMap<String, Vec<&str>>,
    to_app: &mpsc::Sender<IoEvent>,
    parsed: Parsed,
) -> Option<()> {
    let browse = browse?;

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

                        to_app.send(IoEvent::BrowseTitle(list.title)).await.unwrap();
                    } else if list.title == "Albums" {
                        let mut rng = rand::thread_rng();
                        let offset = rng.gen_range(0..list.count);

                        opts.hierarchy = Hierarchy::Albums;
                        opts.count = Some(1);
                        opts.offset = offset;
                        opts.set_display_offset = offset;
                    } else {
                        let browse_path = browse_paths.get(multi_session_str)?;

                        if !browse_path.is_empty() {
                            opts.hierarchy = Hierarchy::Albums;
                        }
                    }

                    opts.multi_session_key = multi_session_key;

                    browse.load(&opts).await;
                }
                Action::Message => {
                    let is_error = result.is_error.unwrap();
                    let message = result.message.unwrap();

                    if is_error && message == "Zone is not configured" {
                        to_app.send(IoEvent::ZoneSelect).await.unwrap();
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

                    browse.load(&opts).await;
                }

                to_app.send(IoEvent::BrowseList(result.offset, result.items)).await.unwrap();
            } else {
                let item = result.items.iter().next()?;
                let browse_path = browse_paths.get_mut(multi_session_str)?;

                if !browse_path.is_empty() {
                    if result.list.title != "Albums" {
                        browse_path.pop();

                        if browse_path.is_empty() {
                            browse_paths.remove(multi_session_str);
                        }
                    }

                    let opts = BrowseOpts {
                        hierarchy: Hierarchy::Albums,
                        zone_or_output_id: multi_session_key.clone(),
                        item_key: item.item_key.clone(),
                        multi_session_key,
                        ..Default::default()
                    };

                    browse.browse(&opts).await;
                }
            }
        }
        _ => (),
    }

    Some(())
}

fn select_next_queue_mode<'a>(settings: &'a mut Settings) -> Option<&'a QueueMode> {
    let zone_id = settings.zone_id.as_ref()?;
    let queue_mode = settings.queue_mode.as_mut()?.get_mut(zone_id)?;
    let index = queue_mode.to_owned() as usize + 1;
    let seq = if settings.profile.is_none() {
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

    Some(queue_mode)
}

fn sync_queue_mode<'a>(settings: &'a mut Settings, auto_radio: bool) -> Option<&'a QueueMode> {
    let zone_id = settings.zone_id.as_ref()?;
    let queue_mode = if auto_radio {
        QueueMode::RoonRadio
    } else if let Some(queue_mode) = settings.queue_mode.as_ref()?.get(zone_id) {
        if *queue_mode == QueueMode::RoonRadio {
            QueueMode::default()
        } else {
            queue_mode.to_owned()
        }
    } else {
        QueueMode::default()
    };

    settings.queue_mode.as_mut()?.insert(zone_id.to_owned(), queue_mode);

    settings.queue_mode.as_ref()?.get(zone_id)
}

async fn handle_queue_mode(
    queue_modes: Option<&HashMap<String, QueueMode>>,
    zone: Option<&Zone>,
    browse: Option<&Browse>
) -> Option<Vec<&'static str>> {
    let zone = zone?;
    let zone_id = zone.zone_id.as_str();
    let queue_mode = queue_modes?.get(zone_id)?;

    zone.now_playing.as_ref()?.length?;

    match queue_mode {
        QueueMode::RandomAlbum => {
            let opts = BrowseOpts {
                hierarchy: Hierarchy::Albums,
                pop_all: true,
                multi_session_key: Some(zone_id.to_owned()),
                ..Default::default()
            };

            browse?.browse(&opts).await;

            Some(vec!["Play Now", "Play Album"])
        }
        _ => None,
    }
}

async fn mute(
    transport: Option<&Transport>,
    zone_map: &HashMap<String, Zone>,
    zone_id: Option<&str>,
    how: &volume::Mute,
) -> Option<Vec<usize>> {
    let zone = zone_map.get(zone_id?)?;
    let mut req_ids = Vec::new();

    for output in &zone.outputs {
        req_ids.push(transport?.mute(&output.output_id, how).await?);
    }

    Some(req_ids)
}

async fn change_volume(
    transport: Option<&Transport>,
    zone_map: &HashMap<String, Zone>,
    zone_id: Option<&str>,
    steps: i32
) -> Option<Vec<usize>> {
    let zone = zone_map.get(zone_id?)?;
    let mut req_ids = Vec::new();

    for output in &zone.outputs {
        req_ids.push(transport?.change_volume(
            &output.output_id,
            &volume::ChangeMode::RelativeStep, steps
        ).await?);
    }

    Some(req_ids)
}

async fn control(
    transport: Option<&Transport>,
    zone_id: Option<&str>,
    how: &Control,
) -> Option<usize> {
    transport?.control(zone_id?, how).await
}

async fn set_roon_radio(
    transport: Option<&Transport>,
    zone_map: &HashMap<String, Zone>,
    zone_id: Option<&str>,
    auto_radio: bool,
) -> Option<usize> {
    let mut settings = zone_map.get(zone_id?)?.settings.clone();

    settings.auto_radio = auto_radio;
    transport?.change_settings(zone_id?, settings).await
}
