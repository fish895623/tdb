use std::env;
use std::fmt::Display;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use env_logger::{Env, Target};
use tdb::config::DEFAULT_CONFIG_FILE;
use tdb::{Config, Database, Field, FieldType};

const USAGE: &str = "usage: tdbd [--config <path>]

Settings come from the first of: --config <path>, $TDB_CONFIG, ./config.toml.
With none of those, built-in defaults are used.";

fn main() -> Result<()> {
    let Some(config) = load_config()? else {
        return Ok(()); // --help
    };

    // The configured level, unless RUST_LOG says otherwise.
    env_logger::Builder::from_env(
        Env::default().default_filter_or(config.log.level.filter().to_string()),
    )
    .target(Target::Stdout)
    .init();

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

    // Append two new records on every run.
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
    Ok(())
}

/// Resolve and load the settings. `None` means `--help` was printed.
fn load_config() -> Result<Option<Config>> {
    let mut explicit: Option<PathBuf> = None;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config" | "-c") => {
                let Some(path) = args.next() else {
                    bail!("--config needs a path\n\n{USAGE}");
                };
                explicit = Some(PathBuf::from(path));
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return Ok(None);
            }
            _ => bail!("unexpected argument {arg:?}\n\n{USAGE}"),
        }
    }

    let from_env = env::var_os("TDB_CONFIG")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let in_cwd = Some(PathBuf::from(DEFAULT_CONFIG_FILE)).filter(|p| p.exists());

    match explicit.or(from_env).or(in_cwd) {
        Some(path) => Ok(Some(Config::load(&path)?)),
        None => Ok(Some(Config::default())),
    }
}
