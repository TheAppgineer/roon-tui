use tokio::sync::mpsc;

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

pub async fn start(io_tx: mpsc::Sender<IoEvent>) {
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

        loop {
            if let Some((core_event, msg)) = core_rx.recv().await {
                match core_event {
                    CoreEvent::Found(mut paired_core) => {
                        browse = paired_core.get_browse().cloned();

                        if let Some(browse) = browse.as_ref() {
                            let opts = BrowseOpts {
                                pop_all: true,
                                ..Default::default()
                            };

                            browse.browse(&opts).await;
                        }
                    }
                    CoreEvent::Lost(_) => (),
                    _ => (),
                }

                if let Some((_, parsed)) = msg {
                    handle_parsed_response(browse.as_ref(), &io_tx, parsed).await;
                }
            }
        }
    });
}

async fn handle_parsed_response(
    browse: Option<&Browse>,
    io_tx: &mpsc::Sender<IoEvent>,
    parsed: Parsed
) {
    if let Some(browse) = browse {
        match parsed {
            Parsed::BrowseResult(result) => {
                match result.action {
                    Action::List => {
                        if let Some(list) = result.list {
                            io_tx.send(IoEvent::BrowseTitle(list.title)).await.unwrap();

                            let offset = list.display_offset.unwrap_or_default();
                            let opts = LoadOpts {
                                count: Some(10),
                                offset,
                                set_display_offset: offset,
                                ..Default::default()
                            };

                            browse.load(&opts).await;
                        }
                    }
                    Action::Message => {
                    }
                    _ => (),
                }
            }
            Parsed::LoadResult(result) => {
                let io_event = IoEvent::BrowseItems(result.items);
                io_tx.send(io_event).await.unwrap();
            }
            _ => (),
        }
    }
}
