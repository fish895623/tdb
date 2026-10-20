use crate::error::Error;
use crate::types::{Field, FieldType};

/// A stored record: raw fixed-width field bytes laid out per the table schema,
/// little-endian, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    bytes: Vec<u8>,
}

impl Record {
    /// Serialize field values (each supplied as an `f64`, converted to the
    /// field's declared type) into the record bytes.
    pub fn from_values(schema: &[Field], values: &[f64]) -> Result<Record, Error> {
        if values.len() != schema.len() {
            return Err(Error::Arity {
                expected: schema.len(),
                got: values.len(),
            });
        }
        let mut bytes = Vec::with_capacity(Record::record_size(schema));
        for (field, &value) in schema.iter().zip(values) {
            store_field(&mut bytes, field.ty, value);
        }
        Ok(Record { bytes })
    }

    /// Wrap pre-read raw bytes (used when loading from disk).
    pub fn from_raw_bytes(bytes: Vec<u8>) -> Record {
        Record { bytes }
    }

    /// Total byte size of a record for this schema.
    pub fn record_size(schema: &[Field]) -> usize {
        schema.iter().map(|f| f.ty.size()).sum()
    }

    /// Read field `i` back as an `f64`. `None` if `i` is not in the schema or
    /// the record is too short to hold it.
    pub fn get(&self, i: usize, schema: &[Field]) -> Option<f64> {
        let field = schema.get(i)?;
        let offset: usize = schema[..i].iter().map(|f| f.ty.size()).sum();
        let raw = self
            .bytes
            .get(offset..offset.checked_add(field.ty.size())?)?;
        Some(match field.ty {
            FieldType::Bool => f64::from(raw[0]),
            FieldType::Int32 => f64::from(i32::from_le_bytes(array(raw)?)),
            FieldType::Int64 => i64::from_le_bytes(array(raw)?) as f64,
            FieldType::Float32 => f64::from(f32::from_le_bytes(array(raw)?)),
            FieldType::Float64 => f64::from_le_bytes(array(raw)?),
        })
    }

    /// The raw record bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn array<const N: usize>(raw: &[u8]) -> Option<[u8; N]> {
    raw.try_into().ok()
}

fn store_field(out: &mut Vec<u8>, ty: FieldType, value: f64) {
    match ty {
        FieldType::Bool => out.push(u8::from(value != 0.0)),
        FieldType::Int32 => out.extend_from_slice(&(value as i32).to_le_bytes()),
        FieldType::Int64 => out.extend_from_slice(&(value as i64).to_le_bytes()),
        FieldType::Float32 => out.extend_from_slice(&(value as f32).to_le_bytes()),
        FieldType::Float64 => out.extend_from_slice(&value.to_le_bytes()),
    }
}
