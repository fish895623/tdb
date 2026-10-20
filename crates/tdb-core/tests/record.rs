//! Ported from tests/record_test.cpp. Uses only the public API.

use tdb_core::{Error, Field, FieldType, Record};

/// Round-trips a single value through a one-column record.
fn round_trip(ty: FieldType, value: f64) -> f64 {
    let schema = [Field::new("x", ty)];
    let r = Record::from_values(&schema, &[value]).expect("one value for one column");
    r.get(0, &schema).expect("field 0 is in the schema")
}

// --- RecordSizeTest ---------------------------------------------------------

#[test]
fn record_size_sums_field_widths() {
    let schema = [
        Field::new("a", FieldType::Int64),
        Field::new("b", FieldType::Bool),
        Field::new("c", FieldType::Float64),
        Field::new("d", FieldType::Int32),
        Field::new("e", FieldType::Float32),
    ];
    assert_eq!(Record::record_size(&schema), 8 + 1 + 8 + 4 + 4);
}

#[test]
fn record_size_empty_schema_is_zero() {
    assert_eq!(Record::record_size(&[]), 0);
}

// --- RecordRoundTripTest ----------------------------------------------------

#[test]
fn record_round_trip_bool() {
    assert_eq!(round_trip(FieldType::Bool, 0.0), 0.0);
    assert_eq!(round_trip(FieldType::Bool, 1.0), 1.0);
    // Any non-zero value is stored as true.
    assert_eq!(round_trip(FieldType::Bool, 42.0), 1.0);
    assert_eq!(round_trip(FieldType::Bool, -3.5), 1.0);
    assert_eq!(round_trip(FieldType::Bool, 0.001), 1.0);
}

#[test]
fn record_round_trip_int32() {
    for v in [0, 1, -1, 123456, -123456, i32::MAX, i32::MIN] {
        assert_eq!(round_trip(FieldType::Int32, f64::from(v)), f64::from(v));
    }
}

#[test]
fn record_round_trip_int32_truncates_fraction() {
    assert_eq!(round_trip(FieldType::Int32, 3.9), 3.0);
    assert_eq!(round_trip(FieldType::Int32, -3.9), -3.0);
}

#[test]
fn record_round_trip_int64() {
    // Values up to 2^53 are exactly representable as double.
    for v in [
        0_i64,
        1,
        -1,
        1_i64 << 31,
        -(1_i64 << 31),
        1_i64 << 53,
        -(1_i64 << 53),
    ] {
        let d = v as f64;
        assert_eq!(round_trip(FieldType::Int64, d), d);
    }
}

#[test]
fn record_round_trip_float32() {
    assert_eq!(round_trip(FieldType::Float32, 1.5), 1.5);
    assert_eq!(round_trip(FieldType::Float32, -0.25), -0.25);
    assert_eq!(round_trip(FieldType::Float32, 0.0), 0.0);
}

#[test]
fn record_round_trip_float32_loses_double_precision() {
    // 0.1 is not exactly representable; it comes back as the nearest float.
    let got = round_trip(FieldType::Float32, 0.1);
    assert_eq!(got, f64::from(0.1_f32));
    assert_ne!(got, 0.1);
}

#[test]
fn record_round_trip_float64_is_lossless() {
    for v in [
        0.0,
        0.1,
        -0.1,
        std::f64::consts::PI,
        1e300,
        -1e-300,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1), // denorm_min
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_eq!(round_trip(FieldType::Float64, v), v);
    }
}

#[test]
fn record_round_trip_float64_preserves_nan_and_negative_zero() {
    assert!(round_trip(FieldType::Float64, f64::NAN).is_nan());
    // -0.0 == 0.0, so compare the bit patterns to check the sign survives.
    assert_eq!(
        round_trip(FieldType::Float64, -0.0).to_bits(),
        (-0.0_f64).to_bits()
    );
}

// --- RecordLayoutTest -------------------------------------------------------

#[test]
fn record_layout_byte_size_matches_record_size() {
    let schema = [
        Field::new("a", FieldType::Int64),
        Field::new("b", FieldType::Bool),
        Field::new("c", FieldType::Float32),
    ];
    let r = Record::from_values(&schema, &[1.0, 1.0, 1.0]).unwrap();
    assert_eq!(r.bytes().len(), Record::record_size(&schema));
}

#[test]
fn record_layout_integers_are_little_endian_twos_complement() {
    let schema = [
        Field::new("i32", FieldType::Int32),
        Field::new("i64", FieldType::Int64),
    ];
    let vals = [-2.0, 258.0]; // 258 = 0x0102
    let r = Record::from_values(&schema, &vals).unwrap();
    assert_eq!(
        r.bytes(),
        [
            0xFE, 0xFF, 0xFF, 0xFF, //
            0x02, 0x01, 0, 0, 0, 0, 0, 0,
        ]
    );
}

#[test]
fn record_layout_bool_is_single_byte() {
    let schema = [
        Field::new("a", FieldType::Bool),
        Field::new("b", FieldType::Bool),
    ];
    let r = Record::from_values(&schema, &[1.0, 0.0]).unwrap();
    assert_eq!(r.bytes(), [1, 0]);
}

#[test]
fn record_layout_float32_uses_ieee754_little_endian() {
    let schema = [Field::new("f", FieldType::Float32)];
    let vals = [1.0]; // 0x3F800000
    let r = Record::from_values(&schema, &vals).unwrap();
    assert_eq!(r.bytes(), [0x00, 0x00, 0x80, 0x3F]);
}

#[test]
fn record_layout_float64_uses_ieee754_little_endian() {
    let schema = [Field::new("f", FieldType::Float64)];
    let vals = [1.0]; // 0x3FF0000000000000
    let r = Record::from_values(&schema, &vals).unwrap();
    assert_eq!(r.bytes(), [0, 0, 0, 0, 0, 0, 0xF0, 0x3F]);
}

// --- RecordTest -------------------------------------------------------------

#[test]
fn record_mixed_schema_reads_each_field_at_its_offset() {
    let schema = [
        Field::new("id", FieldType::Int64),
        Field::new("active", FieldType::Bool),
        Field::new("score", FieldType::Float64),
        Field::new("age", FieldType::Int32),
        Field::new("ratio", FieldType::Float32),
    ];
    let vals = [1234567890123.0, 1.0, 3.5, -7.0, 0.5];
    let r = Record::from_values(&schema, &vals).unwrap();
    assert_eq!(r.get(0, &schema), Some(1234567890123.0));
    assert_eq!(r.get(1, &schema), Some(1.0));
    assert_eq!(r.get(2, &schema), Some(3.5));
    assert_eq!(r.get(3, &schema), Some(-7.0));
    assert_eq!(r.get(4, &schema), Some(0.5));
}

#[test]
fn record_empty_schema_yields_empty_record() {
    let r = Record::from_values(&[], &[]).unwrap();
    assert!(r.bytes().is_empty());
}

#[test]
fn record_too_few_values_errors() {
    let schema = [
        Field::new("a", FieldType::Int32),
        Field::new("b", FieldType::Int32),
    ];
    let err = Record::from_values(&schema, &[1.0]).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Arity {
                expected: 2,
                got: 1
            }
        ),
        "unexpected error: {err:?}"
    );
}

#[test]
fn record_too_many_values_errors() {
    let schema = [Field::new("a", FieldType::Int32)];
    let err = Record::from_values(&schema, &[1.0, 2.0]).unwrap_err();
    assert!(
        matches!(
            err,
            Error::Arity {
                expected: 1,
                got: 2
            }
        ),
        "unexpected error: {err:?}"
    );
}

#[test]
fn record_arity_error_message_reports_counts() {
    let schema = [
        Field::new("a", FieldType::Int32),
        Field::new("b", FieldType::Int32),
    ];
    let err = Record::from_values(&schema, &[1.0]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("want 2, got 1"), "message was: {msg}");
}

#[test]
fn record_from_raw_bytes_preserves_bytes_and_is_readable() {
    let schema = [
        Field::new("a", FieldType::Int32),
        Field::new("b", FieldType::Bool),
    ];
    let r = Record::from_raw_bytes(vec![0x2A, 0, 0, 0, 1]);
    assert_eq!(r.bytes(), [0x2A, 0, 0, 0, 1]);
    assert_eq!(r.get(0, &schema), Some(42.0));
    assert_eq!(r.get(1, &schema), Some(1.0));
}

#[test]
fn record_from_raw_bytes_matches_from_values() {
    let schema = [
        Field::new("a", FieldType::Int64),
        Field::new("b", FieldType::Float64),
    ];
    let a = Record::from_values(&schema, &[99.0, 2.25]).unwrap();
    let b = Record::from_raw_bytes(a.bytes().to_vec());
    assert_eq!(a.bytes(), b.bytes());
    assert_eq!(b.get(0, &schema), Some(99.0));
    assert_eq!(b.get(1, &schema), Some(2.25));
}

// --- Memory-safety behaviour (new in the Rust port) -------------------------
// The C++ `Get` read out of bounds (UB) for a bad index or a short record;
// the Rust `get` returns `None` instead.

#[test]
fn record_get_out_of_range_index_is_none() {
    let schema = [
        Field::new("a", FieldType::Int32),
        Field::new("b", FieldType::Bool),
    ];
    let r = Record::from_values(&schema, &[7.0, 1.0]).unwrap();
    assert_eq!(r.get(0, &schema), Some(7.0));
    assert_eq!(r.get(1, &schema), Some(1.0));
    assert_eq!(r.get(2, &schema), None);
    assert_eq!(r.get(usize::MAX, &schema), None);
    // An empty schema has no valid index at all.
    assert_eq!(r.get(0, &[]), None);
}

#[test]
fn record_get_on_too_short_raw_bytes_is_none() {
    let schema = [
        Field::new("a", FieldType::Int32),
        Field::new("b", FieldType::Bool),
    ];
    // Needs 5 bytes; supply only 3, so even field 0 is truncated.
    let short = Record::from_raw_bytes(vec![0x2A, 0, 0]);
    assert_eq!(short.get(0, &schema), None);
    assert_eq!(short.get(1, &schema), None);
    // 4 bytes: field 0 fits, field 1 (at offset 4) does not.
    let partial = Record::from_raw_bytes(vec![0x2A, 0, 0, 0]);
    assert_eq!(partial.get(0, &schema), Some(42.0));
    assert_eq!(partial.get(1, &schema), None);
    // An empty record can't hold any field.
    let empty = Record::from_raw_bytes(Vec::new());
    assert_eq!(empty.get(0, &schema), None);
}
