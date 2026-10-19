#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace tdb {

// Fundamental storage types of the tdb engine.
// All types are fixed-width so records have a constant size.
enum class FieldType : uint8_t {
  kBool = 1,
  kInt32 = 2,
  kInt64 = 3,
  kFloat32 = 4,
  kFloat64 = 5,
};

inline constexpr size_t FieldTypeSize(FieldType t) {
  switch (t) {
    case FieldType::kBool:
      return 1;
    case FieldType::kInt32:
      return 4;
    case FieldType::kInt64:
      return 8;
    case FieldType::kFloat32:
      return 4;
    case FieldType::kFloat64:
      return 8;
  }
  return 0;
}

// One column definition.
struct Field {
  std::string name;
  FieldType type;
};

// --- little-endian (de)serialization helpers (portable byte layout) ---

inline uint32_t LoadU32LE(const std::byte* p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) |
         ((uint32_t)p[3] << 24);
}

inline uint64_t LoadU64LE(const std::byte* p) {
  return (uint64_t)LoadU32LE(p) | ((uint64_t)LoadU32LE(p + 4) << 32);
}

inline void PutU32LE(std::byte* p, uint32_t v) {
  p[0] = (std::byte)(v & 0xFF);
  p[1] = (std::byte)((v >> 8) & 0xFF);
  p[2] = (std::byte)((v >> 16) & 0xFF);
  p[3] = (std::byte)((v >> 24) & 0xFF);
}

inline void PutU64LE(std::byte* p, uint64_t v) {
  PutU32LE(p, (uint32_t)(v & 0xFFFFFFFF));
  PutU32LE(p + 4, (uint32_t)(v >> 32));
}

}  // namespace tdb
