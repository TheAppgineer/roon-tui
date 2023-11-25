use std::{fs, panic, path};
use time::UtcOffset;
use tokio::sync::mpsc;
use eyre::Result;
use clap::Parser;
use roon_tui::app::App;
use roon_tui::io::{events::Events, roon::{self, Options}};
use roon_tui::start_ui;
use simplelog::{ColorChoice, ConfigBuilder, TerminalMode, TermLogger, WriteLogger, format_description};

const LOG_FILE: &str = concat!(env!("CARGO_PKG_NAME"), ".log");

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

    /// Path to the log file
    #[arg(short, long, default_value = LOG_FILE)]
    log: String,

    /// Enable verbose logging to file
    #[arg(short, long)]
    verbose: bool,

    /// Disable the use of Unicode symbols
    #[arg(short='u', long)]
    no_unicode_symbols: bool,
}

fn init_logger(log: String, max_log_level: log::LevelFilter) -> Result<()> {
    let log_path = path::Path::new(&log);
    let _ = fs::create_dir_all(log_path.parent().unwrap());
    let time_format = format_description!("[hour]:[minute]:[second].[subsecond]");
    let seconds = chrono::Local::now().offset().local_minus_utc();
    let utc_offset = UtcOffset::from_whole_seconds(seconds).unwrap_or(UtcOffset::UTC);
    let config = ConfigBuilder::new()
        .set_time_format_custom(time_format)
        .set_time_offset(utc_offset)
        .build();

    panic::set_hook(Box::new(|info| {
        log::error!("{}", info);
    }));

    match fs::File::create(log) {
        Ok(log) => {
            WriteLogger::init(max_log_level, config, log)?;
        }
        Err(_) => {
            TermLogger::init(
                log::LevelFilter::Warn,
                config,
                TerminalMode::Stderr,
                ColorChoice::Never
            )?;
            log::warn!("Logging to stderr");
        }
    }

    if utc_offset == UtcOffset::UTC {
        log::warn!("Timestamps are UTC");
    }
    else {
        log::info!("Timestamps are local time");
    }

    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let (to_app, from_roon) = mpsc::channel(10);
    let (to_roon, from_app) = mpsc::channel(10);
    let args = Args::parse();
    let mut app = App::new(to_roon, from_roon, args.no_unicode_symbols);
    let options = Options {
        config: args.config,
        ip: args.ip,
        port: args.port,
    };
    let max_log_level = if args.verbose {
        log::LevelFilter::Info
    } else {
        log::LevelFilter::Warn
    };

    let _ = init_logger(args.log, max_log_level);

    Events::start(to_app.clone());

    roon::start(options, to_app, from_app).await;

    start_ui(&mut app).await
}
