use std::time::Duration;
use tokio::sync::mpsc;

use eyre::Result;
use roon_tui::app::App;
use roon_tui::io::{events::Events, roon};
use roon_tui::start_ui;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let tick_rate = Duration::from_millis(200);
    let (to_app, from_roon) = mpsc::channel(10);
    let (to_roon, from_app) = mpsc::channel(10);
    let mut events = Events::new(to_app.clone(), tick_rate);
    let mut app = App::new(to_roon, from_roon);

    roon::start(to_app, from_app).await;

    start_ui(&mut app).await?;

    events.close();

    Ok(())
}
