use std::time::Duration;
use tokio::sync::mpsc;

use eyre::Result;
use roon_tui::app::App;
use roon_tui::io::{events::Events, roon};
use roon_tui::start_ui;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let tick_rate = Duration::from_millis(200);
    let (io_tx, io_rx) = mpsc::channel(10);
    let mut events = Events::new(io_tx.clone(), tick_rate);
    let mut app = App::new(io_rx);

    roon::start(io_tx).await;

    start_ui(&mut app).await?;

    events.close();

    Ok(())
}
