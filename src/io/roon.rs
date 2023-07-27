use tokio::{sync::mpsc, select};

use rust_roon_api::{
    info,
    browse::{Action, Browse, BrowseOpts, LoadOpts},
    CoreEvent,
    Info,
    LogLevel,
    Parsed,
    RoonApi,
    Services,
    Svc,
    transport::Transport,
};
use std::collections::HashMap;

use super::IoEvent;

pub async fn start(to_app: mpsc::Sender<IoEvent>, mut from_app: mpsc::Receiver<IoEvent>) {
    let mut info = info!("com.theappgineer", "Roon TUI");

    info.set_log_level(LogLevel::None);

    let mut roon = RoonApi::new(info);
    let services = vec![
        Services::Browse(Browse::new()),
        Services::Transport(Transport::new()),
    ];
    let provided: HashMap<String, Svc> = HashMap::new();
    let (_, mut core_rx) = roon.start_discovery(provided, Some(services)).await.unwrap();

    tokio::spawn(async move {
        let mut browse = None;
        let mut transport = None;
        let mut opts: BrowseOpts = BrowseOpts::default();
        let mut zone_map = HashMap::new();
        let mut selected_zone_id = None;

        loop {
            select! {
                Some((core_event, msg)) = core_rx.recv() => {
                    match core_event {
                        CoreEvent::Found(mut paired_core) => {
                            browse = paired_core.get_browse().cloned();
                            transport = paired_core.get_transport().cloned();

                            if let Some(browse) = browse.as_ref() {
                                let opts = BrowseOpts {
                                    pop_all: true,
                                    ..Default::default()
                                };

                                browse.browse(&opts).await;
                            }

                            if let Some(transport) = transport.as_ref() {
                                transport.subscribe_zones().await;
                            }

                            let io_event = IoEvent::CoreName(Some(paired_core.display_name));
                            to_app.send(io_event).await.unwrap();
                        }
                        CoreEvent::Lost(_) => to_app.send(IoEvent::CoreName(None)).await.unwrap(),
                        _ => (),
                    }

                    if let Some((_, parsed)) = msg {
                        match parsed {
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

                                if let Some(selected_zone_id) = selected_zone_id.as_ref() {
                                    if let Some(zone) = zone_map.get(selected_zone_id) {
                                        to_app.send(IoEvent::ZoneChanged(zone.to_owned())).await.unwrap();
                                    }
                                }
                            }
                            Parsed::ZonesRemoved(zone_ids) => {
                                if let Some(zone_id) = selected_zone_id.as_ref() {
                                    if zone_ids.contains(zone_id) {
                                        to_app.send(IoEvent::ZoneRemoved(zone_id.to_owned())).await.unwrap();
                                    }
                                }

                                for zone_id in zone_ids {
                                    zone_map.remove(&zone_id);
                                }
                            }
                            _ => {
                                handle_parsed_response(browse.as_ref(), &to_app, parsed).await;
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
                                opts.zone_or_output_id = selected_zone_id.to_owned();

                                browse.browse(&opts).await;
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
                        IoEvent::QueueSelected(queue_item_id) => {
                            if let Some(transport) = transport.as_ref() {
                                let zone_id = opts.zone_or_output_id.as_ref().unwrap();

                                transport.play_from_here(zone_id, queue_item_id).await;
                            }
                        }
                        IoEvent::ZoneSelected(zone_id) => {
                            if let Some(transport) = transport.as_ref() {
                                transport.unsubscribe_queue().await;
                                transport.subscribe_queue(&zone_id, 100).await;

                                if let Some(zone) = zone_map.get(&zone_id) {
                                    to_app.send(IoEvent::ZoneChanged(zone.to_owned())).await.unwrap();
                                }

                                selected_zone_id = Some(zone_id);
                            }
                        },
                        _ => (),
                    }
                }
            }
        }
    });
}

async fn handle_parsed_response(
    browse: Option<&Browse>,
    to_app: &mpsc::Sender<IoEvent>,
    parsed: Parsed,
) {
    if let Some(browse) = browse {
        match parsed {
            Parsed::BrowseResult(result) => {
                match result.action {
                    Action::List => {
                        if let Some(list) = result.list {
                            to_app.send(IoEvent::BrowseTitle(list.title)).await.unwrap();

                            let offset = list.display_offset.unwrap_or_default();
                            let opts = LoadOpts {
                                count: Some(100),
                                offset,
                                set_display_offset: offset,
                                ..Default::default()
                            };

                            browse.load(&opts).await;
                        }
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
            Parsed::LoadResult(result) => {
                let new_offset = result.offset + result.items.len();

                if new_offset < result.list.count {
                    let opts = BrowseOpts {
                        set_display_offset: Some(new_offset),
                        ..Default::default()
                    };

                    browse.browse(&opts).await;
                }

                to_app.send(IoEvent::BrowseList(result.offset, result.items)).await.unwrap();
            }
            Parsed::Queue(queue_items) => {
                to_app.send(IoEvent::QueueList(queue_items)).await.unwrap();
            },
            Parsed::QueueChanges(queue_changes) => {
                to_app.send(IoEvent::QueueListChanges(queue_changes)).await.unwrap();
            }
            _ => (),
        }
    }
}
