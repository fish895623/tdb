use tdb::FieldType;

const ALL: [FieldType; 5] = [
    FieldType::Bool,
    FieldType::Int32,
    FieldType::Int64,
    FieldType::Float32,
    FieldType::Float64,
];

#[test]
fn field_type_size_matches_fixed_widths() {
    assert_eq!(FieldType::Bool.size(), 1);
    assert_eq!(FieldType::Int32.size(), 4);
    assert_eq!(FieldType::Int64.size(), 8);
    assert_eq!(FieldType::Float32.size(), 4);
    assert_eq!(FieldType::Float64.size(), 8);
}

#[test]
fn field_type_size_is_usable_in_constant_expressions() {
    const WIDTH: usize = FieldType::Int64.size();
    assert_eq!(WIDTH, 8);
}

#[test]
fn field_type_on_disk_tag_values_are_stable() {
    // These values are written to disk; changing them breaks the file format.
    assert_eq!(FieldType::Bool.tag(), 1);
    assert_eq!(FieldType::Int32.tag(), 2);
    assert_eq!(FieldType::Int64.tag(), 3);
    assert_eq!(FieldType::Float32.tag(), 4);
    assert_eq!(FieldType::Float64.tag(), 5);
}

#[test]
fn field_type_tags_round_trip() {
    for ty in ALL {
        assert_eq!(FieldType::from_tag(ty.tag()), Some(ty));
    }
}

#[test]
fn unknown_tag_is_not_a_field_type() {
    // C++ had to give an invalid enum value a size of zero; here it cannot exist.
    assert_eq!(FieldType::from_tag(0), None);
    assert_eq!(FieldType::from_tag(6), None);
    assert_eq!(FieldType::from_tag(99), None);
    assert_eq!(FieldType::from_tag(u8::MAX), None);
}
