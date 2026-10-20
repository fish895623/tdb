//! The tdatabase storage format (v1), single file, little-endian:
//!
//! ```text
//! "TDB1" | u32 version
//! u32 table_count
//! per table:
//!   u8 name_len | name
//!   u32 field_count
//!   per field: u8 name_len | name | u8 type
//!   u64 record_count
//!   per record: u32 byte_len | bytes
//! ```

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::record::Record;
use crate::table::Table;
use crate::types::{Field, FieldType};

const MAGIC: &[u8; 4] = b"TDB1";
const FORMAT_VERSION: u32 = 1;
const MAX_NAME_LEN: usize = u8::MAX as usize;

/// A set of tables backed by one file. Data is loaded eagerly by [`open`]
/// and written back by [`flush`].
///
/// [`open`]: Database::open
/// [`flush`]: Database::flush
#[derive(Debug)]
pub struct Database {
    path: PathBuf,
    tables: Vec<Table>,
}

impl Database {
    /// Open (or start empty) the database at `path`. A missing file is an
    /// empty database; any other failure to read it is an error. Call
    /// [`flush`](Database::flush) to persist.
    pub fn open(path: impl AsRef<Path>) -> Result<Database, Error> {
        let path = path.as_ref().to_path_buf();
        log::trace!("opening database at {}", path.display());
        let mut db = Database {
            path,
            tables: Vec::new(),
        };
        match File::open(&db.path) {
            Ok(file) => db.load(Reader::new(BufReader::new(file)))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(db)
    }

    /// Create a new table; fails if the name is already taken.
    pub fn create_table(&mut self, name: &str, schema: Vec<Field>) -> Result<&mut Table, Error> {
        if self.get_table(name).is_some() {
            return Err(Error::TableExists(name.to_owned()));
        }
        check_name_len(name, "table name")?;
        for field in &schema {
            check_name_len(&field.name, "field name")?;
        }
        self.tables.push(Table::new(name, schema));
        let last = self.tables.len() - 1;
        Ok(&mut self.tables[last])
    }

    /// The table called `name`, creating it with `schema` if it does not exist
    /// yet. An existing table keeps its own schema.
    pub fn get_or_create_table(
        &mut self,
        name: &str,
        schema: Vec<Field>,
    ) -> Result<&mut Table, Error> {
        match self.tables.iter().position(|t| t.name() == name) {
            Some(i) => Ok(&mut self.tables[i]),
            None => self.create_table(name, schema),
        }
    }

    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name() == name)
    }

    pub fn get_table_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.iter_mut().find(|t| t.name() == name)
    }

    /// Persist the whole database to disk.
    pub fn flush(&self) -> Result<(), Error> {
        let mut out = BufWriter::new(File::create(&self.path)?);
        self.write_to(&mut out)?;
        out.flush()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    fn write_to(&self, out: &mut impl Write) -> Result<(), Error> {
        out.write_all(MAGIC)?;
        out.write_all(&FORMAT_VERSION.to_le_bytes())?;
        write_u32(out, self.tables.len(), "table count")?;
        for table in &self.tables {
            write_name(out, table.name(), "table name")?;
            write_u32(out, table.schema().len(), "field count")?;
            for field in table.schema() {
                write_name(out, &field.name, "field name")?;
                out.write_all(&[field.ty.tag()])?;
            }
            out.write_all(&(table.len() as u64).to_le_bytes())?;
            for record in table.records() {
                write_u32(out, record.bytes().len(), "record")?;
                out.write_all(record.bytes())?;
            }
        }
        Ok(())
    }

    fn load<R: Read>(&mut self, mut r: Reader<R>) -> Result<(), Error> {
        let mut magic = [0u8; 4];
        match r.fill(&mut magic, "magic") {
            Ok(()) if &magic == MAGIC => {}
            Ok(()) | Err(Error::Corrupt(_)) => {
                return Err(Error::Corrupt("not a tdb database file".into()));
            }
            Err(e) => return Err(e),
        }
        let version = r.u32("version")?;
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let table_count = r.u32("table count")?;
        // Counts come from the file, so never pre-allocate from them: a lying
        // header must not be able to demand memory the file does not back up.
        for _ in 0..table_count {
            let name = r.name("table name")?;
            let field_count = r.u32("field count")?;
            let mut schema = Vec::new();
            for _ in 0..field_count {
                let field_name = r.name("field name")?;
                let tag = r.u8("field type")?;
                let ty = FieldType::from_tag(tag)
                    .ok_or_else(|| Error::Corrupt(format!("unknown field type tag {tag}")))?;
                schema.push(Field {
                    name: field_name,
                    ty,
                });
            }
            let record_size = Record::record_size(&schema);
            let record_count = r.u64("record count")?;
            let mut records = Vec::new();
            for _ in 0..record_count {
                let len = r.u32("record length")? as usize;
                if len != record_size {
                    return Err(Error::Corrupt(format!(
                        "record length {len} does not match schema ({record_size})"
                    )));
                }
                records.push(Record::from_raw_bytes(r.bytes(len, "record data")?));
            }
            let mut table = Table::new(name, schema);
            table.set_records(records)?;
            self.tables.push(table);
        }
        Ok(())
    }
}

fn check_name_len(name: &str, what: &'static str) -> Result<(), Error> {
    if name.len() > MAX_NAME_LEN {
        return Err(Error::TooLong {
            what,
            len: name.len(),
            max: MAX_NAME_LEN,
        });
    }
    Ok(())
}

fn write_u32(out: &mut impl Write, value: usize, what: &'static str) -> Result<(), Error> {
    let value = u32::try_from(value).map_err(|_| Error::TooLong {
        what,
        len: value,
        max: u32::MAX as usize,
    })?;
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_name(out: &mut impl Write, name: &str, what: &'static str) -> Result<(), Error> {
    check_name_len(name, what)?;
    out.write_all(&[name.len() as u8])?;
    out.write_all(name.as_bytes())?;
    Ok(())
}

/// Bounds-checked little-endian reads. Running out of input is a
/// [`Error::Corrupt`], never a panic or a silent zero.
struct Reader<R> {
    inner: R,
}

impl<R: Read> Reader<R> {
    fn new(inner: R) -> Self {
        Reader { inner }
    }

    fn fill(&mut self, buf: &mut [u8], what: &str) -> Result<(), Error> {
        self.inner.read_exact(buf).map_err(|e| match e.kind() {
            io::ErrorKind::UnexpectedEof => {
                Error::Corrupt(format!("unexpected end of file reading {what}"))
            }
            _ => Error::Io(e),
        })
    }

    fn u8(&mut self, what: &str) -> Result<u8, Error> {
        let mut buf = [0u8; 1];
        self.fill(&mut buf, what)?;
        Ok(buf[0])
    }

    fn u32(&mut self, what: &str) -> Result<u32, Error> {
        let mut buf = [0u8; 4];
        self.fill(&mut buf, what)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn u64(&mut self, what: &str) -> Result<u64, Error> {
        let mut buf = [0u8; 8];
        self.fill(&mut buf, what)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read exactly `len` bytes. The buffer grows with the bytes actually
    /// read, so a huge `len` in a short file costs nothing.
    fn bytes(&mut self, len: usize, what: &str) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        let read = (&mut self.inner).take(len as u64).read_to_end(&mut buf)?;
        if read != len {
            return Err(Error::Corrupt(format!(
                "unexpected end of file reading {what}"
            )));
        }
        Ok(buf)
    }

    /// A u8-length-prefixed UTF-8 name.
    fn name(&mut self, what: &str) -> Result<String, Error> {
        let len = usize::from(self.u8(what)?);
        let raw = self.bytes(len, what)?;
        String::from_utf8(raw).map_err(|_| Error::Corrupt(format!("{what} is not valid UTF-8")))
    }
}
