use std::fmt::Display;

use env_logger::{Env, Target};
use tdb::{Database, Error, Field, FieldType};

fn main() -> Result<(), Error> {
    // Trace by default; RUST_LOG overrides it.
    env_logger::Builder::from_env(Env::default().default_filter_or("trace"))
        .target(Target::Stdout)
        .init();

    log::info!("Hello, World!");

    let mut db = Database::open("tdatabase.db")?;

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
