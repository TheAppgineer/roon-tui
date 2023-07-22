use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use log::error;

use crate::io::IoEvent;

/// A small event handler that wrap crossterm input and tick event. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
    // To stop the loop
    stop_capture: Arc<AtomicBool>,
}

impl Events {
    /// Constructs an new instance of `Events` with the default config.
    pub fn new(io_tx: mpsc::Sender<IoEvent>, tick_rate: Duration) -> Events {
        let stop_capture = Arc::new(AtomicBool::new(false));

        let event_stop_capture = stop_capture.clone();
        tokio::spawn(async move {
            loop {
                // poll for tick rate duration, if no event, sent tick event.
                if crossterm::event::poll(tick_rate).unwrap() {
                    if let crossterm::event::Event::Key(key) = crossterm::event::read().unwrap() {
                        if let Err(err) = io_tx.send(IoEvent::Input(key)).await {
                            error!("Oops!, {}", err);
                        }
                    }
                }
                if let Err(err) = io_tx.send(IoEvent::Tick).await {
                    error!("Oops!, {}", err);
                }
                if event_stop_capture.load(Ordering::Relaxed) {
                    break;
                }
            }
        });

        Events {
            stop_capture,
        }
    }

    pub fn close(&mut self) {
        self.stop_capture.store(true, Ordering::Relaxed)
    }
}
