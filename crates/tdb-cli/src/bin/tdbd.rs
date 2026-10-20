use std::env;
use std::fmt::Display;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use env_logger::{Env, Target};
use tdb_config::{Config, DEFAULT_CONFIG_FILE};
use tdb_core::{Database, Field, FieldType};
use tokio::net::TcpListener;
use tokio::task;

/// The tdb database daemon.
#[derive(Debug, Parser)]
#[command(
    name = "tdbd",
    version,
    after_help = "tdbd keeps running in the background, answering tdb:// clients, until it \
                  receives SIGINT (Ctrl-C) or SIGTERM. It then finishes the requests in \
                  flight, flushes the database and exits.\n\
                  \n\
                  Settings come from the first of: --config <PATH>, $TDB_CONFIG, ./config.toml.\n\
                  With none of those, built-in defaults are used."
)]
struct Args {
    /// Read settings from this file
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Accept tdb:// connections on this address, instead of server.listen
    #[arg(short, long, value_name = "ADDR")]
    listen: Option<SocketAddr>,

    /// Run the startup step once and exit instead of staying up (opens no
    /// listener)
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parses before anything else runs, so --help, --version and a bad
    // argument exit without touching the database.
    let args = Args::parse();
    let mut config = load_config(args.config)?;
    if let Some(listen) = args.listen {
        config.server.listen = listen;
    }

    // The configured level, unless RUST_LOG says otherwise.
    env_logger::Builder::from_env(
        Env::default().default_filter_or(config.log.level.filter().to_string()),
    )
    .target(Target::Stdout)
    .init();

    run(config, args.once).await
}

/// Open the database, then serve `tdb://` clients until asked to stop (or,
/// with `once`, return as soon as the database is open and flushed).
async fn run(config: Config, once: bool) -> Result<()> {
    // Listen for signals, and for clients, before the database is touched. A
    // signal that arrives while it is opening waits in the queue and is
    // handled once the database is safe, instead of killing the process
    // halfway through a write; and an address that is already in use fails
    // here, before anything is written.
    let daemon = if once {
        None
    } else {
        let stop = shutdown_signal()?;
        let listen = config.server.listen;
        let listener = TcpListener::bind(listen)
            .await
            .with_context(|| format!("listening on {listen}"))?;
        Some((stop, listener))
    };

    // The engine does blocking file I/O, so it stays off the runtime's
    // worker threads.
    let db = task::spawn_blocking(move || start(&config))
        .await
        .context("the startup task failed")??;

    let Some((stop, listener)) = daemon else {
        return Ok(());
    };
    println!(
        "tdbd is running (pid {}), listening on {}; stop it with SIGINT or SIGTERM.",
        std::process::id(),
        listener
            .local_addr()
            .context("reading the listening address")?
    );
    tdb_server::serve(listener, async {
        let signal = stop.await;
        println!("Received {signal}, shutting down.");
    })
    .await;

    task::spawn_blocking(move || db.flush())
        .await
        .context("the shutdown task failed")??;
    println!("tdbd stopped.");
    Ok(())
}

/// Open the database and append two records to the demo table, then flush.
fn start(config: &Config) -> Result<Database> {
    let mut db = Database::open(&config.database.path)
        .with_context(|| format!("opening {}", config.database.path.display()))?;

    let users = db.get_or_create_table(
        "users",
        vec![
            Field::new("id", FieldType::Int64),
            Field::new("active", FieldType::Bool),
            Field::new("score", FieldType::Float64),
        ],
    )?;

    // Append two new records on every start.
    for i in 0..2 {
        let id = users.len() as f64 + 1.0;
        users.insert(&[id, 1.0, 3.5 + f64::from(i)])?;
    }

    println!("Table '{}' has {} record(s):", users.name(), users.len());
    let show = |value: Option<f64>| -> Box<dyn Display> {
        match value {
            Some(v) => Box::new(v),
            None => Box::new("?"),
        }
    };
    for (i, record) in users.records().iter().enumerate() {
        let schema = users.schema();
        println!(
            "  [{i}] id={} active={} score={}",
            show(record.get(0, schema)),
            show(record.get(1, schema)),
            show(record.get(2, schema)),
        );
    }

    db.flush()?;
    println!("Database '{}' flushed.", db.path().display());
    Ok(db)
}

/// Start listening for the signals that mean "stop". The returned future
/// resolves, with the signal's name, when one arrives; the listening begins
/// here, not when the future is first polled.
#[cfg(unix)]
fn shutdown_signal() -> io::Result<impl Future<Output = &'static str>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        }
    })
}

/// Only Ctrl-C is available outside Unix.
#[cfg(not(unix))]
fn shutdown_signal() -> io::Result<impl Future<Output = &'static str>> {
    Ok(async {
        let _ = tokio::signal::ctrl_c().await;
        "Ctrl-C"
    })
}

/// Resolve and load the settings: `explicit` (from `--config`), then
/// `$TDB_CONFIG`, then `./config.toml`, then the built-in defaults.
fn load_config(explicit: Option<PathBuf>) -> Result<Config> {
    let from_env = env::var_os("TDB_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let in_cwd = Some(PathBuf::from(DEFAULT_CONFIG_FILE)).filter(|p| p.exists());

    match explicit.or(from_env).or(in_cwd) {
        Some(path) => Ok(Config::load(&path)?),
        None => Ok(Config::default()),
    }
}
