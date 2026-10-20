//! Fundamental storage types of the tdb engine.
//! All types are fixed-width so records have a constant size.

/// A column's storage type. The discriminants are the on-disk tags and must
/// never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FieldType {
    Bool = 1,
    Int32 = 2,
    Int64 = 3,
    Float32 = 4,
    Float64 = 5,
}

impl FieldType {
    /// Width in bytes of a value of this type.
    pub const fn size(self) -> usize {
        match self {
            FieldType::Bool => 1,
            FieldType::Int32 => 4,
            FieldType::Int64 => 8,
            FieldType::Float32 => 4,
            FieldType::Float64 => 8,
        }
    }

    /// The tag stored in the database file.
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// The type for an on-disk tag, or `None` if the tag is unknown.
    pub const fn from_tag(tag: u8) -> Option<FieldType> {
        match tag {
            1 => Some(FieldType::Bool),
            2 => Some(FieldType::Int32),
            3 => Some(FieldType::Int64),
            4 => Some(FieldType::Float32),
            5 => Some(FieldType::Float64),
            _ => None,
        }
    }
}

/// One column definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
}

impl Field {
    pub fn new(name: impl Into<String>, ty: FieldType) -> Field {
        Field {
            name: name.into(),
            ty,
        }
    }
}
