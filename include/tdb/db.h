#pragma once

#include <cstdint>
#include <fstream>
#include <memory>
#include <spdlog/spdlog.h>
#include <stdexcept>
#include <string>
#include <vector>

#include "tdb/table.h"

namespace tdb {

// The tdatabase storage format (v1), single file, little-endian:
//   u32 "TDB1" | u32 version
//   u32 table_count
//   per table:
//     u8 name_len | name
//     u32 field_count
//     per field: u8 name_len | name | u8 type
//     u64 record_count
//     per record: u32 byte_len | bytes
class Database {
 public:
  // Open (or start empty) the database at `path`. Data is loaded
  // eagerly; call Flush() to persist.
  static Database Open(const std::string& path) {
    SPDLOG_TRACE("Opening database at {}", path);
    Database db(path);
    db.Load();
    return db;
  }

  // Create a new table; fails if the name is already taken.
  Table* CreateTable(const std::string& name, std::vector<Field> schema) {
    if (GetTable(name))
      throw std::invalid_argument("table '" + name + "' already exists");
    tables_.push_back(std::make_unique<Table>(name, std::move(schema)));
    return tables_.back().get();
  }

  Table* GetTable(const std::string& name) const {
    for (const auto& t : tables_)
      if (t->Name() == name) return t.get();
    return nullptr;
  }

  // Persist the whole database to disk.
  void Flush() const {
    std::ofstream f(path_, std::ios::binary | std::ios::trunc);
    if (!f)
      throw std::runtime_error("cannot open " + path_ + " for writing");
    WriteString(f, "TDB1");
    WriteU32(f, 1);  // format version
    WriteU32(f, static_cast<uint32_t>(tables_.size()));
    for (const auto& t : tables_) {
      WriteU8(f, static_cast<uint8_t>(t->Name().size()));
      WriteBytes(f, t->Name().data(), t->Name().size());
      const auto& schema = t->Schema();
      WriteU32(f, static_cast<uint32_t>(schema.size()));
      for (const auto &[name, type] : schema) {
        WriteU8(f, static_cast<uint8_t>(name.size()));
        WriteBytes(f, name.data(), name.size());
        WriteU8(f, static_cast<uint8_t>(type));
      }
      WriteU64(f, t->Count());
      for (const auto& r : t->Records()) {
        WriteU32(f, static_cast<uint32_t>(r.bytes().size()));
        WriteBytes(f, r.bytes().data(), r.bytes().size());
      }
    }
  }

  const std::string& Path() const { return path_; }
  const std::vector<std::unique_ptr<Table>>& Tables() const {
    return tables_;
  }

 private:
  explicit Database(std::string path) : path_(std::move(path)) {}

  void Load() {
    std::ifstream f(path_, std::ios::binary);
    if (!f) return;  // no file yet: start with an empty database
    char magic[4];
    f.read(magic, 4);
    if (f.gcount() != 4 || std::string(magic, 4) != "TDB1")
      throw std::runtime_error("not a tdb database file: " + path_);
    uint32_t version;
    ReadU32(f, &version);
    if (version != 1)
      throw std::runtime_error("unsupported tdb format version: " +
                               std::to_string(version));
    uint32_t ntables;
    ReadU32(f, &ntables);
    for (uint32_t ti = 0; ti < ntables; ++ti) {
      std::string name = ReadString(f);
      uint32_t nfields;
      ReadU32(f, &nfields);
      std::vector<Field> schema;
      for (uint32_t fi = 0; fi < nfields; ++fi) {
        Field field;
        field.name = ReadString(f);
        uint8_t type;
        ReadU8(f, &type);
        field.type = (FieldType)type;
        schema.push_back(std::move(field));
      }
      uint64_t nrecs;
      ReadU64(f, &nrecs);
      std::vector<Record> records;
      records.reserve(nrecs);
      for (uint64_t ri = 0; ri < nrecs; ++ri) {
        uint32_t len;
        ReadU32(f, &len);
        std::vector<std::byte> bytes(len);
        if (len > 0) {
          f.read(reinterpret_cast<char*>(bytes.data()), len);
          if (f.gcount() != (std::streamsize)len)
            throw std::runtime_error("corrupt record data in " + path_);
        }
        records.push_back(Record::FromRawBytes(std::move(bytes)));
      }
      tables_.push_back(std::make_unique<Table>(name, std::move(schema)));
      tables_.back()->SetRecords(std::move(records));
    }
  }

  // --- small binary stream helpers ---
  static void WriteString(std::ofstream& f, const char* s) {
    f.write(s, 4);
  }
  static void WriteU8(std::ofstream& f, uint8_t v) { f.put((char)v); }
  static void WriteU32(std::ofstream& f, uint32_t v) {
    std::byte p[4];
    PutU32LE(p, v);
    f.write(reinterpret_cast<char*>(p), 4);
  }
  static void WriteU64(std::ofstream& f, uint64_t v) {
    std::byte p[8];
    PutU64LE(p, v);
    f.write(reinterpret_cast<char*>(p), 8);
  }
  static void WriteBytes(std::ofstream& f, const void* p, size_t n) {
    if (n > 0) f.write(reinterpret_cast<const char*>(p), (std::streamsize)n);
  }

  static void ReadU8(std::ifstream& f, uint8_t* v) {
    int c = f.get();
    *v = c == EOF ? 0 : (uint8_t)c;
  }
  static void ReadU32(std::ifstream& f, uint32_t* v) {
    std::byte p[4];
    f.read(reinterpret_cast<char*>(p), 4);
    *v = f.gcount() == 4 ? LoadU32LE(p) : 0;
  }
  static void ReadU64(std::ifstream& f, uint64_t* v) {
    std::byte p[8];
    f.read(reinterpret_cast<char*>(p), 8);
    *v = f.gcount() == 8 ? LoadU64LE(p) : 0;
  }
  static std::string ReadString(std::ifstream& f) {
    uint8_t len = 0;
    ReadU8(f, &len);
    std::string s(len, '\0');
    if (len > 0) {
      f.read(s.data(), len);
      if (f.gcount() != (std::streamsize)len)
        throw std::runtime_error("corrupt string in database file");
    }
    return s;
  }

  std::string path_;
  std::vector<std::unique_ptr<Table>> tables_;
};

}  // namespace tdb
