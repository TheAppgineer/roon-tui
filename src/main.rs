use clap::Parser;
use eyre::Result;
use roon_tui::app::App;
use roon_tui::io::{
    events::Events,
    roon::{self, Options},
};
use roon_tui::start_ui;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path to the config.json file
    #[arg(short, long, default_value = "config.json")]
    config: String,

    /// IP address of the Server, disables server discovery
    #[arg(short, long)]
    ip: Option<String>,

    /// Port number of the Server
    #[arg(short, long, default_value = "9330")]
    port: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let (to_app, from_roon) = mpsc::channel(10);
    let (to_roon, from_app) = mpsc::channel(10);
    let mut app = App::new(to_roon, from_roon);
    let args = Args::parse();
    let options = Options {
        config: args.config,
        ip: args.ip,
        port: args.port,
    };

    Events::start(to_app.clone());

    roon::start(options, to_app, from_app).await;

    start_ui(&mut app).await?;

    Ok(())
}
