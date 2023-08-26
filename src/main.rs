use tokio::sync::mpsc;
use eyre::Result;
use roon_tui::app::App;
use roon_tui::io::{events::Events, roon};
use roon_tui::start_ui;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let (to_app, from_roon) = mpsc::channel(10);
    let (to_roon, from_app) = mpsc::channel(10);
    let mut app = App::new(to_roon, from_roon);
    let config_path: String = std::env::args()
        .skip(1)
        .next()
        .unwrap_or("config.json".to_owned());

    Events::start(to_app.clone());

    roon::start(config_path, to_app, from_app).await;

    start_ui(&mut app).await?;

    Ok(())
}
