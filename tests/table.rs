use tdb::{Error, Field, FieldType, Record, Table};

fn user_schema() -> Vec<Field> {
    vec![
        Field::new("id", FieldType::Int64),
        Field::new("active", FieldType::Bool),
        Field::new("score", FieldType::Float64),
    ]
}

#[test]
fn table_new_table_is_empty() {
    let t = Table::new("users", user_schema());
    assert_eq!(t.len(), 0);
    assert!(t.is_empty());
    assert!(t.records().is_empty());
}

#[test]
fn table_exposes_name_and_schema() {
    let t = Table::new("users", user_schema());
    assert_eq!(t.name(), "users");
    assert_eq!(t.schema(), user_schema().as_slice());
    assert_eq!(t.schema()[0].name, "id");
    assert_eq!(t.schema()[0].ty, FieldType::Int64);
    assert_eq!(t.schema()[1].name, "active");
    assert_eq!(t.schema()[1].ty, FieldType::Bool);
    assert_eq!(t.schema()[2].name, "score");
    assert_eq!(t.schema()[2].ty, FieldType::Float64);
}

#[test]
fn table_insert_returns_sequential_ids() {
    let mut t = Table::new("users", user_schema());
    assert_eq!(t.insert(&[1.0, 1.0, 1.5]).unwrap(), 0);
    assert_eq!(t.insert(&[2.0, 0.0, 2.5]).unwrap(), 1);
    assert_eq!(t.insert(&[3.0, 1.0, 3.5]).unwrap(), 2);
    assert_eq!(t.len(), 3);
}

#[test]
fn table_get_returns_inserted_values() {
    let mut t = Table::new("users", user_schema());
    t.insert(&[10.0, 1.0, 1.5]).unwrap();
    t.insert(&[20.0, 0.0, 2.5]).unwrap();

    let schema = t.schema();
    let first = t.get(0).unwrap();
    assert_eq!(first.get(0, schema), Some(10.0));
    assert_eq!(first.get(1, schema), Some(1.0));
    assert_eq!(first.get(2, schema), Some(1.5));
    let second = t.get(1).unwrap();
    assert_eq!(second.get(0, schema), Some(20.0));
    assert_eq!(second.get(1, schema), Some(0.0));
    assert_eq!(second.get(2, schema), Some(2.5));
}

#[test]
fn table_get_out_of_range_is_none() {
    let mut t = Table::new("users", user_schema());
    assert!(t.get(0).is_none()); // empty table

    t.insert(&[1.0, 1.0, 1.0]).unwrap();
    assert!(t.get(0).is_some());
    assert!(t.get(1).is_none()); // one past the end
    assert!(t.get(usize::MAX).is_none());
}

#[test]
fn table_insert_with_wrong_arity_fails_and_leaves_table_unchanged() {
    let mut t = Table::new("users", user_schema());
    t.insert(&[1.0, 1.0, 1.0]).unwrap();

    let err = t.insert(&[1.0, 1.0]).unwrap_err();
    assert!(matches!(
        err,
        Error::Arity {
            expected: 3,
            got: 2
        }
    ));
    assert_eq!(t.len(), 1);
}

#[test]
fn table_records_match_insertion_order() {
    let mut t = Table::new("t", vec![Field::new("n", FieldType::Int32)]);
    for i in 0..100 {
        t.insert(&[f64::from(i)]).unwrap();
    }
    assert_eq!(t.records().len(), 100);
    for (i, record) in t.records().iter().enumerate() {
        assert_eq!(record.get(0, t.schema()), Some(i as f64));
    }
}

#[test]
fn table_set_records_replaces_existing_records() {
    let mut t = Table::new("t", vec![Field::new("n", FieldType::Int32)]);
    t.insert(&[1.0]).unwrap();
    t.insert(&[1.0]).unwrap();

    let replacement = vec![Record::from_values(t.schema(), &[7.0]).unwrap()];
    t.set_records(replacement).unwrap();

    assert_eq!(t.len(), 1);
    assert_eq!(t.get(0).unwrap().get(0, t.schema()), Some(7.0));
}

#[test]
fn table_insert_after_set_records_continues_ids() {
    let mut t = Table::new("t", vec![Field::new("n", FieldType::Int32)]);
    let loaded = vec![
        Record::from_values(t.schema(), &[5.0]).unwrap(),
        Record::from_values(t.schema(), &[5.0]).unwrap(),
    ];
    t.set_records(loaded).unwrap();

    assert_eq!(t.insert(&[9.0]).unwrap(), 2);
}

#[test]
fn table_zero_field_table_accepts_empty_records() {
    let mut t = Table::new("empty", Vec::new());
    assert_eq!(t.insert(&[]).unwrap(), 0);
    assert_eq!(t.insert(&[]).unwrap(), 1);
    assert_eq!(t.len(), 2);
    assert!(t.get(0).unwrap().bytes().is_empty());
}

#[test]
fn table_set_records_rejects_records_of_the_wrong_size_and_changes_nothing() {
    let mut t = Table::new("t", vec![Field::new("n", FieldType::Int32)]);
    t.insert(&[1.0]).unwrap();

    // A record for a different schema: 8 bytes instead of 4.
    let wrong = vec![Record::from_raw_bytes(vec![0; 8])];
    let err = t.set_records(wrong).unwrap_err();
    assert!(matches!(
        err,
        Error::RecordSize {
            expected: 4,
            got: 8
        }
    ));
    assert_eq!(err.to_string(), "record is 8 bytes but the schema needs 4");

    // One bad record among good ones also rejects the whole batch.
    let good = Record::from_values(t.schema(), &[2.0]).unwrap();
    let mixed = vec![good, Record::from_raw_bytes(Vec::new())];
    assert!(t.set_records(mixed).is_err());

    assert_eq!(t.len(), 1);
    assert_eq!(t.get(0).unwrap().get(0, t.schema()), Some(1.0));
}
