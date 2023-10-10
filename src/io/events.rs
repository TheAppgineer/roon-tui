use crossterm::event::{self, KeyCode, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::io::IoEvent;

pub struct Events;

impl Events {
    pub fn start(to_app: mpsc::Sender<IoEvent>) {
        tokio::spawn(async move {
            loop {
                match event::read().unwrap() {
                    event::Event::Key(key) => {
                        to_app.send(IoEvent::Input(key)).await.unwrap();

                        if key.kind == KeyEventKind::Press
                            && key.modifiers == KeyModifiers::CONTROL
                            && key.code == KeyCode::Char('c')
                        {
                            break;
                        }
                    }
                    event::Event::Resize(_, _) => to_app.send(IoEvent::Redraw).await.unwrap(),
                    _ => (),
                }
            }
        });
    }
}
