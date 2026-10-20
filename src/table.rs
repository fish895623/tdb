use crate::error::Error;
use crate::record::Record;
use crate::types::Field;

/// A table: named schema + append-only list of fixed-width records.
/// Reads are linear scans (MVP; indexes come later).
#[derive(Debug, Clone)]
pub struct Table {
    name: String,
    schema: Vec<Field>,
    records: Vec<Record>,
}

impl Table {
    pub fn new(name: impl Into<String>, schema: Vec<Field>) -> Table {
        Table {
            name: name.into(),
            schema,
            records: Vec::new(),
        }
    }

    /// Append a record; returns its 0-based record id. On error the table is
    /// unchanged.
    pub fn insert(&mut self, values: &[f64]) -> Result<usize, Error> {
        let record = Record::from_values(&self.schema, values)?;
        self.records.push(record);
        Ok(self.records.len() - 1)
    }

    /// The record with this id, or `None` if there is no such record.
    pub fn get(&self, id: usize) -> Option<&Record> {
        self.records.get(id)
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &[Field] {
        &self.schema
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// Replace all records (used when loading from disk). Every record must be
    /// exactly the schema's record size; otherwise nothing is replaced.
    pub fn set_records(&mut self, records: Vec<Record>) -> Result<(), Error> {
        let expected = Record::record_size(&self.schema);
        if let Some(bad) = records.iter().find(|r| r.bytes().len() != expected) {
            return Err(Error::RecordSize {
                expected,
                got: bad.bytes().len(),
            });
        }
        self.records = records;
        Ok(())
    }
}
