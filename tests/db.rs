//! Integration tests for `Database`: opening, table management, the exact
//! on-disk format, and how malformed files are rejected.
//!
//! Ported from the C++ `tests/db_test.cpp`. Only the public API is used.

use std::f64::consts::PI;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use tdb::{Database, Error, Field, FieldType, Table};
use tempfile::TempDir;

/// Gives each test its own scratch directory (and so its own database path).
/// The directory and everything in it is removed when this is dropped.
struct Scratch {
    dir: TempDir,
    path: PathBuf,
}

impl Scratch {
    fn new() -> Scratch {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("test.db");
        Scratch { dir, path }
    }

    /// The scratch directory itself.
    fn dir(&self) -> &Path {
        self.dir.path()
    }

    fn open(&self) -> Result<Database, Error> {
        Database::open(&self.path)
    }

    /// Open the database and fail the test if that does not work.
    fn open_ok(&self) -> Database {
        self.open().expect("open database")
    }

    fn read_file(&self) -> Vec<u8> {
        fs::read(&self.path).expect("read database file")
    }

    fn write_file(&self, bytes: &[u8]) {
        fs::write(&self.path, bytes).expect("write database file");
    }

    fn file_len(&self) -> u64 {
        fs::metadata(&self.path).expect("stat database file").len()
    }

    /// Cuts `n` bytes off the end of the file.
    fn truncate_by(&self, n: u64) {
        let file = OpenOptions::new()
            .write(true)
            .open(&self.path)
            .expect("open database file for truncation");
        let len = file.metadata().expect("stat database file").len();
        file.set_len(len - n).expect("truncate database file");
    }

    /// Opening the file as it is now must fail with `Error::Corrupt`.
    #[track_caller]
    fn assert_open_corrupt(&self) {
        let result = self.open();
        assert!(
            matches!(result, Err(Error::Corrupt(_))),
            "expected Err(Corrupt), got {result:?}"
        );
    }

    /// Writes `bytes` as the database file; opening it must fail with
    /// `Error::Corrupt`.
    #[track_caller]
    fn assert_corrupt(&self, bytes: &[u8]) {
        self.write_file(bytes);
        let result = self.open();
        assert!(
            matches!(result, Err(Error::Corrupt(_))),
            "expected Err(Corrupt) for file {bytes:02x?}, got {result:?}"
        );
    }
}

/// Header bytes shared by hand-built files: "TDB1" + little-endian version.
fn header(version: u32) -> Vec<u8> {
    let mut bytes = b"TDB1".to_vec();
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes
}

fn append(v: &mut Vec<u8>, bytes: &[u8]) {
    v.extend_from_slice(bytes);
}

/// The field values of record `id`, read back through the table's schema.
fn row(table: &Table, id: usize) -> Vec<f64> {
    let record = table.get(id).expect("record id in range");
    (0..table.schema().len())
        .map(|i| record.get(i, table.schema()).expect("field in range"))
        .collect()
}

// --- opening ---

#[test]
fn database_open_missing_file_starts_empty() {
    let scratch = Scratch::new();
    let db = scratch.open_ok();
    assert!(db.tables().is_empty());
    assert_eq!(db.path(), scratch.path);
    assert!(!scratch.path.exists(), "open must not create the file");
}

#[test]
fn database_open_empty_file_is_corrupt() {
    let scratch = Scratch::new();
    scratch.assert_corrupt(&[]);
}

#[test]
fn database_open_with_bad_magic_is_corrupt() {
    let scratch = Scratch::new();
    scratch.assert_corrupt(&[b'N', b'O', b'P', b'E', 1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn database_open_with_short_magic_is_corrupt() {
    let scratch = Scratch::new();
    scratch.assert_corrupt(b"TD");
}

#[test]
fn database_open_with_unsupported_version_is_error() {
    let scratch = Scratch::new();
    let mut bytes = header(2);
    append(&mut bytes, &[0, 0, 0, 0]);
    scratch.write_file(&bytes);
    let err = scratch.open().unwrap_err();
    assert!(
        matches!(err, Error::UnsupportedVersion(2)),
        "expected UnsupportedVersion(2), got {err:?}"
    );
    assert!(err.to_string().contains("version"), "{err}");

    // The version is checked right after the magic, before anything else is
    // read, so even a file that ends there reports the version problem
    // rather than a truncation.
    for version in [0, 3, u32::MAX] {
        scratch.write_file(&header(version));
        let err = scratch.open().unwrap_err();
        assert!(
            matches!(err, Error::UnsupportedVersion(v) if v == version),
            "version {version}: got {err:?}"
        );
    }
}

#[test]
fn database_open_directory_is_io_error() {
    // Only a missing file is an empty database; any other failure to read the
    // path is reported, not swallowed.
    let scratch = Scratch::new();
    let result = Database::open(scratch.dir());
    assert!(
        matches!(result, Err(Error::Io(_))),
        "expected Err(Io), got {result:?}"
    );
}

// --- table management ---

#[test]
fn database_create_table_then_get_table() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    let t = db
        .create_table("users", vec![Field::new("id", FieldType::Int64)])
        .unwrap();
    assert_eq!(t.name(), "users");
    assert_eq!(db.tables().len(), 1);
    let found = db.get_table("users").expect("table was just created");
    assert_eq!(found.name(), "users");
    // `get_table` hands back the very table that `tables()` lists.
    assert!(std::ptr::eq(found, &db.tables()[0]));
}

#[test]
fn database_get_table_missing_returns_none() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    assert!(db.get_table("nope").is_none());
    assert!(db.get_table_mut("nope").is_none());
    db.create_table("a", vec![]).unwrap();
    assert!(db.get_table("b").is_none());
    assert!(db.get_table_mut("b").is_none());
}

#[test]
fn database_get_table_is_case_sensitive() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    db.create_table("Users", vec![]).unwrap();
    assert!(db.get_table("users").is_none());
    assert!(db.get_table("Users").is_some());
}

#[test]
fn database_create_duplicate_table_fails() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    db.create_table("users", vec![Field::new("id", FieldType::Int64)])
        .unwrap();
    let err = db
        .create_table("users", vec![Field::new("other", FieldType::Bool)])
        .unwrap_err();
    assert!(
        matches!(&err, Error::TableExists(name) if name == "users"),
        "expected TableExists(\"users\"), got {err:?}"
    );
    assert_eq!(db.tables().len(), 1);
    // The original table is untouched.
    let schema = db.get_table("users").unwrap().schema();
    assert_eq!(schema, [Field::new("id", FieldType::Int64)]);
}

// In C++ this checked that `Table*` stayed valid as the table list grew. A
// Rust reference cannot outlive a mutation of the database, so the
// observable guarantee is different: tables keep their creation order, and
// earlier tables keep their name, schema and data while more are added.
#[test]
fn database_tables_keep_order_and_data_as_more_tables_are_added() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    for i in 0..50 {
        let t = db
            .create_table(&format!("t{i}"), vec![Field::new("n", FieldType::Int32)])
            .unwrap();
        t.insert(&[f64::from(i)]).unwrap();
    }

    let names: Vec<&str> = db.tables().iter().map(Table::name).collect();
    let expected: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
    assert_eq!(names, expected);

    for i in 0..50 {
        let t = db.get_table(&format!("t{i}")).expect("table exists");
        assert_eq!(t.name(), format!("t{i}"));
        assert_eq!(t.schema(), [Field::new("n", FieldType::Int32)]);
        assert_eq!(t.len(), 1);
        assert_eq!(row(t, 0), [f64::from(i)]);
    }

    // The first table is still writable after the others were added.
    let first = db.get_table_mut("t0").unwrap();
    assert_eq!(first.insert(&[100.0]).unwrap(), 1);
    assert_eq!(row(db.get_table("t0").unwrap(), 1), [100.0]);
    assert_eq!(db.get_table("t1").unwrap().len(), 1);
}

#[test]
fn database_tables_are_independent() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();
    db.create_table("a", vec![Field::new("n", FieldType::Int32)])
        .unwrap();
    db.create_table("b", vec![Field::new("n", FieldType::Int32)])
        .unwrap();
    let a = db.get_table_mut("a").unwrap();
    a.insert(&[1.0]).unwrap();
    a.insert(&[1.0]).unwrap();
    assert_eq!(db.get_table("a").unwrap().len(), 2);
    assert_eq!(db.get_table("b").unwrap().len(), 0);
}

// The name length is stored in a single byte. C++ silently truncated longer
// names (writing a file that could not be read back); Rust refuses them.
#[test]
fn database_create_table_with_too_long_name_fails() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();

    let too_long = "x".repeat(256);
    let err = db.create_table(&too_long, vec![]).unwrap_err();
    assert!(
        matches!(
            err,
            Error::TooLong {
                what: "table name",
                len: 256,
                max: 255
            }
        ),
        "got {err:?}"
    );

    let err = db
        .create_table("ok", vec![Field::new(&too_long, FieldType::Bool)])
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::TooLong {
                what: "field name",
                len: 256,
                max: 255
            }
        ),
        "got {err:?}"
    );

    // The limit is in bytes, not characters: 86 three-byte characters are
    // 258 bytes even though there are only 86 of them.
    let err = db.create_table(&"사".repeat(86), vec![]).unwrap_err();
    assert!(
        matches!(
            err,
            Error::TooLong {
                len: 258,
                max: 255,
                ..
            }
        ),
        "got {err:?}"
    );

    // Nothing was created by the failed calls, and the database still writes
    // a valid (empty) file.
    assert!(db.tables().is_empty());
    db.flush().unwrap();
    let mut expected = header(1);
    append(&mut expected, &[0, 0, 0, 0]);
    assert_eq!(scratch.read_file(), expected);
}

#[test]
fn database_get_or_create_table_keeps_existing_schema() {
    let scratch = Scratch::new();
    let mut db = scratch.open_ok();

    let t = db
        .get_or_create_table("log", vec![Field::new("a", FieldType::Int32)])
        .unwrap();
    t.insert(&[1.0]).unwrap();

    // The second call finds the table and ignores the schema it is given.
    let t = db
        .get_or_create_table("log", vec![Field::new("other", FieldType::Bool)])
        .unwrap();
    assert_eq!(t.schema(), [Field::new("a", FieldType::Int32)]);
    assert_eq!(t.len(), 1);
    assert_eq!(db.tables().len(), 1);

    let err = db
        .get_or_create_table(&"x".repeat(256), vec![])
        .unwrap_err();
    assert!(matches!(err, Error::TooLong { .. }), "got {err:?}");
}

// --- persistence ---

#[test]
fn database_flush_creates_file_with_magic() {
    let scratch = Scratch::new();
    let db = scratch.open_ok();
    db.flush().unwrap();
    let bytes = scratch.read_file();
    assert!(bytes.len() >= 4);
    assert_eq!(&bytes[..4], b"TDB1");
}

#[test]
fn database_empty_database_round_trips() {
    let scratch = Scratch::new();
    scratch.open_ok().flush().unwrap();
    let db = scratch.open_ok();
    assert!(db.tables().is_empty());
}

#[test]
fn database_empty_database_has_exact_bytes() {
    let scratch = Scratch::new();
    scratch.open_ok().flush().unwrap();
    let mut expected = header(1);
    append(&mut expected, &[0, 0, 0, 0]); // table_count = 0
    assert_eq!(scratch.read_file(), expected);
}

#[test]
fn database_flush_writes_documented_format() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db
            .create_table("t", vec![Field::new("a", FieldType::Int32)])
            .unwrap();
        t.insert(&[7.0]).unwrap();
        db.flush().unwrap();
    }

    let mut expected = header(1);
    append(&mut expected, &[1, 0, 0, 0]); // table_count
    append(&mut expected, &[1, b't']); // name_len, name
    append(&mut expected, &[1, 0, 0, 0]); // field_count
    append(&mut expected, &[1, b'a', 2]); // field: len, name, Int32
    append(&mut expected, &[1, 0, 0, 0, 0, 0, 0, 0]); // record_count (u64)
    append(&mut expected, &[4, 0, 0, 0]); // record byte_len
    append(&mut expected, &[7, 0, 0, 0]); // record bytes
    assert_eq!(scratch.read_file(), expected);
}

#[test]
fn database_loads_hand_built_file() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b'a', 2]);
    append(&mut bytes, &[2, 0, 0, 0, 0, 0, 0, 0]); // two records
    append(&mut bytes, &[4, 0, 0, 0, 0xFE, 0xFF, 0xFF, 0xFF]); // -2
    append(&mut bytes, &[4, 0, 0, 0, 0x2A, 0, 0, 0]); // 42
    scratch.write_file(&bytes);

    let db = scratch.open_ok();
    let t = db.get_table("t").expect("table t");
    assert_eq!(t.len(), 2);
    assert_eq!(t.get(0).unwrap().get(0, t.schema()), Some(-2.0));
    assert_eq!(t.get(1).unwrap().get(0, t.schema()), Some(42.0));
    assert_eq!(t.get(0).unwrap().bytes(), [0xFE, 0xFF, 0xFF, 0xFF]);
    assert_eq!(t.get(1).unwrap().bytes(), [0x2A, 0, 0, 0]);
    assert!(t.get(2).is_none());
}

// The on-disk type tags (Bool=1 .. Float64=5) are part of the format. Load a
// hand-built file that uses all five, then write it back: the bytes must
// come out identical.
#[test]
fn database_field_type_tags_load_and_flush_byte_exact() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]); // table_count
    append(&mut bytes, &[4, b't', b'a', b'g', b's']);
    append(&mut bytes, &[5, 0, 0, 0]); // field_count
    append(&mut bytes, &[1, b'b', 1]); // Bool
    append(&mut bytes, &[1, b'i', 2]); // Int32
    append(&mut bytes, &[1, b'l', 3]); // Int64
    append(&mut bytes, &[1, b'f', 4]); // Float32
    append(&mut bytes, &[1, b'd', 5]); // Float64
    append(&mut bytes, &[1, 0, 0, 0, 0, 0, 0, 0]); // record_count
    append(&mut bytes, &[25, 0, 0, 0]); // record byte_len: 1 + 4 + 8 + 4 + 8
    append(&mut bytes, &[1]); // Bool true
    append(&mut bytes, &[7, 0, 0, 0]); // Int32 7
    append(&mut bytes, &[0xFF; 8]); // Int64 -1
    append(&mut bytes, &[0, 0, 0xC0, 0x3F]); // Float32 1.5
    append(&mut bytes, &[0, 0, 0, 0, 0, 0, 0, 0xC0]); // Float64 -2.0
    scratch.write_file(&bytes);

    let db = scratch.open_ok();
    let t = db.get_table("tags").expect("table tags");
    let types: Vec<FieldType> = t.schema().iter().map(|f| f.ty).collect();
    assert_eq!(
        types,
        [
            FieldType::Bool,
            FieldType::Int32,
            FieldType::Int64,
            FieldType::Float32,
            FieldType::Float64
        ]
    );
    assert_eq!(row(t, 0), [1.0, 7.0, -1.0, 1.5, -2.0]);

    db.flush().unwrap();
    assert_eq!(scratch.read_file(), bytes);
}

#[test]
fn database_round_trips_all_field_types() {
    let scratch = Scratch::new();
    let schema = vec![
        Field::new("flag", FieldType::Bool),
        Field::new("i32", FieldType::Int32),
        Field::new("i64", FieldType::Int64),
        Field::new("f32", FieldType::Float32),
        Field::new("f64", FieldType::Float64),
    ];
    {
        let mut db = scratch.open_ok();
        let t = db.create_table("all", schema.clone()).unwrap();
        t.insert(&[1.0, -123456.0, 1234567890123.0, 0.5, PI])
            .unwrap();
        t.insert(&[0.0, 2147483647.0, -9007199254740992.0, -2.25, -1e-300])
            .unwrap();
        db.flush().unwrap();
    }

    let db = scratch.open_ok();
    let t = db.get_table("all").expect("table all");

    assert_eq!(t.schema(), schema.as_slice());

    assert_eq!(t.len(), 2);
    assert_eq!(row(t, 0), [1.0, -123456.0, 1234567890123.0, 0.5, PI]);
    assert_eq!(
        row(t, 1),
        [0.0, 2147483647.0, -9007199254740992.0, -2.25, -1e-300]
    );
    // 1 (bool) + 4 + 8 + 4 + 8, no padding.
    assert_eq!(t.get(0).unwrap().bytes().len(), 25);
}

#[test]
fn database_round_trips_multiple_tables_in_order() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let a = db
            .create_table("alpha", vec![Field::new("x", FieldType::Int32)])
            .unwrap();
        a.insert(&[11.0]).unwrap();
        let b = db
            .create_table(
                "beta",
                vec![
                    Field::new("y", FieldType::Float64),
                    Field::new("z", FieldType::Bool),
                ],
            )
            .unwrap();
        b.insert(&[2.5, 1.0]).unwrap();
        b.insert(&[2.5, 1.0]).unwrap();
        db.flush().unwrap();
    }

    let db = scratch.open_ok();
    assert_eq!(db.tables().len(), 2);
    assert_eq!(db.tables()[0].name(), "alpha");
    assert_eq!(db.tables()[1].name(), "beta");
    assert_eq!(db.get_table("alpha").unwrap().len(), 1);
    assert_eq!(db.get_table("beta").unwrap().len(), 2);
    assert_eq!(row(db.get_table("alpha").unwrap(), 0), [11.0]);
    assert_eq!(row(db.get_table("beta").unwrap(), 1), [2.5, 1.0]);
}

#[test]
fn database_round_trips_table_with_no_records() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        db.create_table("empty", vec![Field::new("n", FieldType::Int64)])
            .unwrap();
        db.flush().unwrap();
    }
    let db = scratch.open_ok();
    let t = db.get_table("empty").expect("table empty");
    assert_eq!(t.len(), 0);
    assert!(t.is_empty());
    assert_eq!(t.schema(), [Field::new("n", FieldType::Int64)]);
}

#[test]
fn database_round_trips_zero_field_table_with_records() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db.create_table("nofields", vec![]).unwrap();
        assert_eq!(t.insert(&[]).unwrap(), 0);
        assert_eq!(t.insert(&[]).unwrap(), 1);
        assert_eq!(t.insert(&[]).unwrap(), 2);
        db.flush().unwrap();
    }
    let db = scratch.open_ok();
    let t = db.get_table("nofields").expect("table nofields");
    assert!(t.schema().is_empty());
    assert_eq!(t.len(), 3);
    assert!(t.records().iter().all(|r| r.bytes().is_empty()));
}

#[test]
fn database_round_trips_many_records() {
    const N: usize = 5000;
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db
            .create_table(
                "big",
                vec![
                    Field::new("i", FieldType::Int64),
                    Field::new("sq", FieldType::Float64),
                ],
            )
            .unwrap();
        for i in 0..N {
            let x = i as f64;
            assert_eq!(t.insert(&[x, x * x]).unwrap(), i);
        }
        db.flush().unwrap();
    }
    let db = scratch.open_ok();
    let t = db.get_table("big").expect("table big");
    assert_eq!(t.len(), N);
    for i in 0..N {
        let x = i as f64;
        assert_eq!(row(t, i), [x, x * x], "record {i}");
    }
}

#[test]
fn database_round_trips_max_length_and_utf8_names() {
    let scratch = Scratch::new();
    let long_name = "x".repeat(255); // name_len is a single byte
    let utf8 = "사용자"; // length counted in bytes
    let utf8_max = "사".repeat(85); // 85 characters, exactly 255 bytes
    assert_eq!(utf8_max.len(), 255);
    {
        let mut db = scratch.open_ok();
        db.create_table(&long_name, vec![Field::new(&long_name, FieldType::Bool)])
            .unwrap();
        db.create_table(utf8, vec![Field::new(utf8, FieldType::Int32)])
            .unwrap();
        db.create_table(&utf8_max, vec![Field::new(&utf8_max, FieldType::Int64)])
            .unwrap();
        db.flush().unwrap();
    }

    // The first table's name_len byte (right after the 8-byte header and the
    // 4-byte table_count) must be 255, not a truncated value.
    assert_eq!(scratch.read_file()[12], 0xFF);

    let db = scratch.open_ok();
    let a = db.get_table(&long_name).expect("long-named table");
    assert_eq!(a.schema()[0].name, long_name);
    let b = db.get_table(utf8).expect("utf-8 named table");
    assert_eq!(b.schema()[0].name, utf8);
    let c = db.get_table(&utf8_max).expect("255-byte utf-8 table");
    assert_eq!(c.schema()[0].name, utf8_max);
    assert_eq!(db.tables().len(), 3);
}

#[test]
fn database_records_accumulate_across_sessions() {
    // Mirrors how bin/tdbd uses the database: reopen, append, flush.
    let scratch = Scratch::new();
    for session in 0..3 {
        let mut db = scratch.open_ok();
        let t = db
            .get_or_create_table("log", vec![Field::new("session", FieldType::Int32)])
            .unwrap();
        t.insert(&[f64::from(session)]).unwrap();
        t.insert(&[f64::from(session)]).unwrap();
        db.flush().unwrap();
    }
    let db = scratch.open_ok();
    let t = db.get_table("log").expect("table log");
    assert_eq!(t.len(), 6);
    for i in 0..6 {
        assert_eq!(row(t, i), [(i / 2) as f64], "record {i}");
    }
}

#[test]
fn database_flush_overwrites_previous_contents() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db
            .create_table("t", vec![Field::new("n", FieldType::Int64)])
            .unwrap();
        for i in 0..100 {
            t.insert(&[f64::from(i)]).unwrap();
        }
        db.flush().unwrap();
    }
    let big = scratch.file_len();

    {
        let mut db = scratch.open_ok();
        assert_eq!(db.get_table("t").unwrap().len(), 100);
        db.get_table_mut("t")
            .unwrap()
            .set_records(Vec::new())
            .unwrap();
        db.flush().unwrap();
    }
    // The file is truncated on flush, so no stale tail bytes survive.
    assert!(scratch.file_len() < big);
    let db = scratch.open_ok();
    assert_eq!(db.get_table("t").expect("table t").len(), 0);
}

#[test]
fn database_flush_to_unwritable_path_is_io_error() {
    let scratch = Scratch::new();
    let bad = scratch.dir().join("no_such_dir").join("x.db");
    // Opening a path whose directory is missing is fine: no file, no tables.
    let db = Database::open(&bad).unwrap();
    let err = db.flush().unwrap_err();
    assert!(matches!(err, Error::Io(_)), "expected Io, got {err:?}");
    assert!(!scratch.dir().join("no_such_dir").exists());
}

// --- corruption ---

#[test]
fn database_truncated_record_data_is_corrupt() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db
            .create_table("t", vec![Field::new("n", FieldType::Int64)])
            .unwrap();
        t.insert(&[1.0]).unwrap();
        t.insert(&[1.0]).unwrap();
        db.flush().unwrap();
    }
    scratch.truncate_by(3); // cut into the last record's bytes
    scratch.assert_open_corrupt();
}

#[test]
fn database_truncated_string_is_corrupt() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]); // one table
    append(&mut bytes, &[5, b'a', b'b']); // name_len=5 but only 2 bytes follow
    scratch.assert_corrupt(&bytes);
}

#[test]
fn database_truncated_field_name_is_corrupt() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[1, 0, 0, 0]); // one field
    append(&mut bytes, &[9, b'a']); // field name_len=9, 1 byte present
    scratch.assert_corrupt(&bytes);
}

// The record length matches the schema (an Int32 field is 4 bytes) but the
// file ends before, or part-way through, the record bytes.
#[test]
fn database_record_length_past_end_of_file_is_corrupt() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b'a', 2]);
    append(&mut bytes, &[1, 0, 0, 0, 0, 0, 0, 0]);
    append(&mut bytes, &[4, 0, 0, 0]); // claims 4 bytes, none follow
    scratch.assert_corrupt(&bytes);

    append(&mut bytes, &[7, 0]); // ... or only 2 of the 4 follow
    scratch.assert_corrupt(&bytes);
}

// Same fixture as the C++ test: the record claims 255 bytes and none follow.
// C++ treated that as "past end of file"; Rust additionally rejects any
// record whose stored length differs from the schema's record size (4 for a
// single Int32), whether or not the bytes are there.
#[test]
fn database_record_length_not_matching_schema_is_corrupt() {
    let scratch = Scratch::new();
    let mut prefix = header(1);
    append(&mut prefix, &[1, 0, 0, 0]);
    append(&mut prefix, &[1, b't']);
    append(&mut prefix, &[1, 0, 0, 0]);
    append(&mut prefix, &[1, b'a', 2]);
    append(&mut prefix, &[1, 0, 0, 0, 0, 0, 0, 0]);

    let mut bytes = prefix.clone();
    append(&mut bytes, &[0xFF, 0, 0, 0]); // claims 255 bytes, none follow
    scratch.assert_corrupt(&bytes);

    // Wrong length with all the claimed bytes actually present.
    for len in [0u32, 1, 3, 5, 8, 255] {
        let mut bytes = prefix.clone();
        append(&mut bytes, &len.to_le_bytes());
        bytes.resize(bytes.len() + len as usize, 0);
        scratch.assert_corrupt(&bytes);
    }

    // A zero-field table has record size 0, so any non-zero length is wrong.
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[0, 0, 0, 0]); // no fields
    append(&mut bytes, &[1, 0, 0, 0, 0, 0, 0, 0]);
    append(&mut bytes, &[1, 0, 0, 0, 0xAA]);
    scratch.assert_corrupt(&bytes);
}

#[test]
fn database_unknown_field_type_tag_is_corrupt() {
    let scratch = Scratch::new();
    for tag in [0u8, 6, 7, 0x80, 0xFF] {
        let mut bytes = header(1);
        append(&mut bytes, &[1, 0, 0, 0]);
        append(&mut bytes, &[1, b't']);
        append(&mut bytes, &[1, 0, 0, 0]);
        append(&mut bytes, &[1, b'a', tag]);
        append(&mut bytes, &[0, 0, 0, 0, 0, 0, 0, 0]); // no records
        scratch.assert_corrupt(&bytes);
    }
}

#[test]
fn database_non_utf8_names_are_corrupt() {
    let scratch = Scratch::new();

    // Table name: 0xFF 0xFE is not UTF-8. Everything else is well-formed.
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[2, 0xFF, 0xFE]);
    append(&mut bytes, &[0, 0, 0, 0]); // no fields
    append(&mut bytes, &[0, 0, 0, 0, 0, 0, 0, 0]); // no records
    scratch.assert_corrupt(&bytes);

    // Field name: 0xC3 0x28 is an invalid two-byte sequence.
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[2, 0xC3, 0x28, 1]);
    append(&mut bytes, &[0, 0, 0, 0, 0, 0, 0, 0]);
    scratch.assert_corrupt(&bytes);
}

// The C++ reader silently read zeros past the end of a short file in places.
// Cutting a valid file at any length (including 0) must be reported.
#[test]
fn database_every_truncation_of_a_valid_file_is_corrupt() {
    let scratch = Scratch::new();
    {
        let mut db = scratch.open_ok();
        let t = db
            .create_table(
                "mixed",
                vec![
                    Field::new("flag", FieldType::Bool),
                    Field::new("n", FieldType::Int64),
                    Field::new("x", FieldType::Float32),
                ],
            )
            .unwrap();
        t.insert(&[1.0, -5.0, 0.25]).unwrap();
        t.insert(&[0.0, 6.0, -0.5]).unwrap();
        db.create_table("empty", vec![Field::new("사용자", FieldType::Int32)])
            .unwrap();
        let z = db.create_table("nofields", vec![]).unwrap();
        z.insert(&[]).unwrap();
        db.flush().unwrap();
    }
    let full = scratch.read_file();

    // Sanity check: the untruncated file is fine.
    assert_eq!(scratch.open_ok().tables().len(), 3);

    for len in 0..full.len() {
        scratch.write_file(&full[..len]);
        let result = scratch.open();
        assert!(
            matches!(result, Err(Error::Corrupt(_))),
            "file cut to {len} of {} bytes: expected Err(Corrupt), got {result:?}",
            full.len()
        );
    }
}

// --- memory safety: counts and lengths come from the file ---
//
// None of these may allocate based on the lying header value (a Vec sized from
// u64::MAX or u32::MAX would abort the process); each must just run out of
// file and report `Corrupt`.

#[test]
fn database_huge_table_count_without_data_is_corrupt() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &u32::MAX.to_le_bytes()); // table_count, no tables
    scratch.assert_corrupt(&bytes);

    // Same lie, with one real (empty) table before the file ends.
    let mut bytes = header(1);
    append(&mut bytes, &u32::MAX.to_le_bytes());
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[0, 0, 0, 0]);
    append(&mut bytes, &[0, 0, 0, 0, 0, 0, 0, 0]);
    scratch.assert_corrupt(&bytes);
}

#[test]
fn database_huge_field_count_without_data_is_corrupt() {
    let scratch = Scratch::new();
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &u32::MAX.to_le_bytes()); // field_count, no fields
    scratch.assert_corrupt(&bytes);

    // One real field, then the file ends.
    append(&mut bytes, &[1, b'a', 2]);
    scratch.assert_corrupt(&bytes);
}

#[test]
fn database_huge_record_count_without_data_is_corrupt() {
    let scratch = Scratch::new();

    // A normal schema, record_count = u64::MAX, no records.
    let mut with_field = header(1);
    append(&mut with_field, &[1, 0, 0, 0]);
    append(&mut with_field, &[1, b't']);
    append(&mut with_field, &[1, 0, 0, 0]);
    append(&mut with_field, &[1, b'a', 2]);
    append(&mut with_field, &u64::MAX.to_le_bytes());
    scratch.assert_corrupt(&with_field);

    // ... with two real records, then the file ends.
    let mut bytes = with_field.clone();
    append(&mut bytes, &[4, 0, 0, 0, 1, 0, 0, 0]);
    append(&mut bytes, &[4, 0, 0, 0, 2, 0, 0, 0]);
    scratch.assert_corrupt(&bytes);

    // A zero-field table has zero-byte records, the cheapest possible lie.
    let mut zero_field = header(1);
    append(&mut zero_field, &[1, 0, 0, 0]);
    append(&mut zero_field, &[1, b't']);
    append(&mut zero_field, &[0, 0, 0, 0]);
    append(&mut zero_field, &u64::MAX.to_le_bytes());
    scratch.assert_corrupt(&zero_field);

    let mut bytes = zero_field.clone();
    append(&mut bytes, &[0, 0, 0, 0]);
    append(&mut bytes, &[0, 0, 0, 0]);
    scratch.assert_corrupt(&bytes);
}

#[test]
fn database_record_length_u32_max_in_short_file_is_corrupt() {
    let scratch = Scratch::new();

    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b'a', 2]);
    append(&mut bytes, &[1, 0, 0, 0, 0, 0, 0, 0]);
    append(&mut bytes, &u32::MAX.to_le_bytes()); // byte_len, nothing follows
    scratch.assert_corrupt(&bytes);

    append(&mut bytes, &[1, 2, 3, 4]); // a few bytes, still nowhere near 4 GiB
    scratch.assert_corrupt(&bytes);

    // Same with a zero-field schema.
    let mut bytes = header(1);
    append(&mut bytes, &[1, 0, 0, 0]);
    append(&mut bytes, &[1, b't']);
    append(&mut bytes, &[0, 0, 0, 0]);
    append(&mut bytes, &[1, 0, 0, 0, 0, 0, 0, 0]);
    append(&mut bytes, &u32::MAX.to_le_bytes());
    scratch.assert_corrupt(&bytes);
}
