#pragma once

#include <cstdint>
#include <span>
#include <stdexcept>
#include <string>
#include <vector>

#include "tdb/record.h"

namespace tdb {

// A table: named schema + append-only list of fixed-width records.
// Reads are linear scans (MVP; indexes come later).
class Table {
 public:
  Table(std::string name, std::vector<Field> schema)
      : name_(std::move(name)), schema_(std::move(schema)) {}

  // Append a record; returns its 0-based record id.
  uint64_t Insert(std::span<const double> values) {
    records_.push_back(Record::FromValues(schema_, values));
    return records_.size() - 1;
  }

  const Record& At(uint64_t id) const {
    if (id >= records_.size())
      throw std::out_of_range("record id out of range");
    return records_[id];
  }

  uint64_t Count() const { return records_.size(); }

  const std::string& Name() const { return name_; }
  const std::vector<Field>& Schema() const { return schema_; }
  const std::vector<Record>& Records() const { return records_; }

  void SetRecords(std::vector<Record> records) {
    records_ = std::move(records);
  }

 private:
  std::string name_;
  std::vector<Field> schema_;
  std::vector<Record> records_;
};

}  // namespace tdb
