#include <gtest/gtest.h>

#include <cstdint>
#include <limits>

#include "tdb/types.h"

using namespace tdb;

TEST(FieldTypeSizeTest, MatchesFixedWidths) {
  EXPECT_EQ(FieldTypeSize(FieldType::kBool), 1u);
  EXPECT_EQ(FieldTypeSize(FieldType::kInt32), 4u);
  EXPECT_EQ(FieldTypeSize(FieldType::kInt64), 8u);
  EXPECT_EQ(FieldTypeSize(FieldType::kFloat32), 4u);
  EXPECT_EQ(FieldTypeSize(FieldType::kFloat64), 8u);
}

TEST(FieldTypeSizeTest, UnknownTypeHasZeroSize) {
  EXPECT_EQ(FieldTypeSize(static_cast<FieldType>(0)), 0u);
  EXPECT_EQ(FieldTypeSize(static_cast<FieldType>(99)), 0u);
}

TEST(FieldTypeSizeTest, UsableInConstantExpressions) {
  static_assert(FieldTypeSize(FieldType::kInt64) == 8);
  SUCCEED();
}

TEST(FieldTypeTest, OnDiskTagValuesAreStable) {
  // These values are written to disk; changing them breaks the file format.
  EXPECT_EQ(static_cast<uint8_t>(FieldType::kBool), 1);
  EXPECT_EQ(static_cast<uint8_t>(FieldType::kInt32), 2);
  EXPECT_EQ(static_cast<uint8_t>(FieldType::kInt64), 3);
  EXPECT_EQ(static_cast<uint8_t>(FieldType::kFloat32), 4);
  EXPECT_EQ(static_cast<uint8_t>(FieldType::kFloat64), 5);
}

TEST(EndianTest, PutU32WritesLittleEndian) {
  std::byte buf[4];
  PutU32LE(buf, 0x01020304u);
  EXPECT_EQ(buf[0], std::byte{0x04});
  EXPECT_EQ(buf[1], std::byte{0x03});
  EXPECT_EQ(buf[2], std::byte{0x02});
  EXPECT_EQ(buf[3], std::byte{0x01});
}

TEST(EndianTest, PutU64WritesLittleEndian) {
  std::byte buf[8];
  PutU64LE(buf, 0x0102030405060708ull);
  for (int i = 0; i < 8; ++i)
    EXPECT_EQ(buf[i], std::byte(8 - i)) << "byte " << i;
}

TEST(EndianTest, LoadU32ReadsLittleEndian) {
  const std::byte buf[4] = {std::byte{0x78}, std::byte{0x56}, std::byte{0x34},
                            std::byte{0x12}};
  EXPECT_EQ(LoadU32LE(buf), 0x12345678u);
}

TEST(EndianTest, LoadU64ReadsLittleEndian) {
  const std::byte buf[8] = {std::byte{0xEF}, std::byte{0xCD}, std::byte{0xAB},
                            std::byte{0x89}, std::byte{0x67}, std::byte{0x45},
                            std::byte{0x23}, std::byte{0x01}};
  EXPECT_EQ(LoadU64LE(buf), 0x0123456789ABCDEFull);
}

TEST(EndianTest, U32RoundTripsEdgeValues) {
  for (uint32_t v : {0u, 1u, 0xFFu, 0x100u, 0x80000000u,
                     std::numeric_limits<uint32_t>::max()}) {
    std::byte buf[4];
    PutU32LE(buf, v);
    EXPECT_EQ(LoadU32LE(buf), v);
  }
}

TEST(EndianTest, U64RoundTripsEdgeValues) {
  for (uint64_t v : {0ull, 1ull, 0xFFFFFFFFull, 0x100000000ull,
                     0x8000000000000000ull,
                     std::numeric_limits<uint64_t>::max()}) {
    std::byte buf[8];
    PutU64LE(buf, v);
    EXPECT_EQ(LoadU64LE(buf), v);
  }
}

TEST(EndianTest, PutDoesNotTouchNeighbouringBytes) {
  std::byte buf[12];
  for (auto& b : buf) b = std::byte{0xAA};
  PutU32LE(buf + 4, 0xFFFFFFFFu);
  for (int i = 0; i < 4; ++i) EXPECT_EQ(buf[i], std::byte{0xAA});
  for (int i = 8; i < 12; ++i) EXPECT_EQ(buf[i], std::byte{0xAA});
}
