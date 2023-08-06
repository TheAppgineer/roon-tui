use crossterm::event::{KeyEventKind, KeyModifiers, KeyCode};
use tokio::sync::mpsc;
use log::error;

use crate::io::IoEvent;

pub struct Events;

impl Events {
    pub fn start(to_app: mpsc::Sender<IoEvent>) {
        tokio::spawn(async move {
            loop {
                if let crossterm::event::Event::Key(key) = crossterm::event::read().unwrap() {
                    if let Err(err) = to_app.send(IoEvent::Input(key)).await {
                        error!("Oops!, {}", err);
                    }

                    if key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::CONTROL
                        && key.code == KeyCode::Char('c')
                    {
                        break;
                    }
                }
            }
        });
    }
}
