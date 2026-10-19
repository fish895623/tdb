#pragma once

#include <cstring>
#include <span>
#include <stdexcept>
#include <vector>

#include "tdb/types.h"

namespace tdb {

// A stored record: raw fixed-width field bytes laid out per the table schema.
class Record {
 public:
  // Serialize field values (each supplied as a double, cast to the
  // field's declared type) into the record bytes.
  static Record FromValues(const std::vector<Field>& schema,
                           std::span<const double> values) {
    if (values.size() != schema.size())
      throw std::invalid_argument(
          "value count does not match schema (want " +
          std::to_string(schema.size()) + ", got " +
          std::to_string(values.size()) + ")");
    Record r;
    r.bytes_.reserve(RecordSize(schema));
    for (size_t i = 0; i < schema.size(); ++i)
      StoreField(&r.bytes_, schema[i].type, values[i]);
    return r;
  }

  // Wrap pre-read raw bytes (used when loading from disk).
  static Record FromRawBytes(std::vector<std::byte> bytes) {
    Record r;
    r.bytes_ = std::move(bytes);
    return r;
  }

  // Total byte size of a record for this schema.
  static size_t RecordSize(const std::vector<Field>& schema) {
    size_t n = 0;
    for (const auto& f : schema) n += FieldTypeSize(f.type);
    return n;
  }

  // Read field `i` back as a double.
  double Get(size_t i, const std::vector<Field>& schema) const {
    const std::byte* p = bytes_.data() + OffsetOf(i, schema);
    switch (schema[i].type) {
      case FieldType::kBool:
        return (double)*p;
      case FieldType::kInt32:
        return (int32_t)LoadU32LE(p);
      case FieldType::kInt64:
        return (double)(int64_t)LoadU64LE(p);
      case FieldType::kFloat32: {
        uint32_t bits = LoadU32LE(p);
        float f;
        std::memcpy(&f, &bits, sizeof(f));
        return (double)f;
      }
      case FieldType::kFloat64: {
        uint64_t bits = LoadU64LE(p);
        double d;
        std::memcpy(&d, &bits, sizeof(d));
        return d;
      }
    }
    throw std::invalid_argument("invalid field type");
  }

  const std::vector<std::byte>& bytes() const { return bytes_; }

 private:
  Record() = default;

  static size_t OffsetOf(size_t i, const std::vector<Field>& schema) {
    size_t off = 0;
    for (size_t j = 0; j < i; ++j) off += FieldTypeSize(schema[j].type);
    return off;
  }

  static void StoreField(std::vector<std::byte>* out, FieldType type,
                         double v) {
    const size_t sz = FieldTypeSize(type);
    const size_t at = out->size();
    out->resize(at + sz);
    std::byte* p = out->data() + at;
    switch (type) {
      case FieldType::kBool:
        *p = (std::byte)(v != 0.0);
        break;
      case FieldType::kInt32:
        PutU32LE(p, (uint32_t)(int32_t)v);
        break;
      case FieldType::kInt64:
        PutU64LE(p, (uint64_t)(int64_t)v);
        break;
      case FieldType::kFloat32: {
        float f = (float)v;
        uint32_t bits;
        std::memcpy(&bits, &f, sizeof(bits));
        PutU32LE(p, bits);
        break;
      }
      case FieldType::kFloat64: {
        uint64_t bits;
        std::memcpy(&bits, &v, sizeof(bits));
        PutU64LE(p, bits);
        break;
      }
    }
  }

  std::vector<std::byte> bytes_;
};

}  // namespace tdb
