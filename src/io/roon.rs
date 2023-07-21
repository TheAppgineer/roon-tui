use tokio::sync::mpsc;

use rust_roon_api::{
    info,
    browse::{Browse, BrowseOpts, Action},
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
        let mut browse: Option<Browse> = None;

        loop {
            if let Some((core, msg)) = core_rx.recv().await {
                match core {
                    CoreEvent::Found(mut core) => {
                        let _ = io_tx.send(super::IoEvent::Initialize).await;

                        browse = core.get_browse().cloned();

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
                    if let Some(_) = browse.as_ref() {
                        match parsed {
                            Parsed::BrowseResult(result) => {
                                match result.action {
                                    Action::List => {
                                        if let Some(list) = result.list {
                                            let io_event = super::IoEvent::BrowseTitle(list.title);
                                            let _ = io_tx.send(io_event).await;
                                        }
                                    }
                                    Action::Message => {
                                    }
                                    _ => (),
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    });
}
