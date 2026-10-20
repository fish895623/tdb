#include <gtest/gtest.h>

#include <cmath>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

#include "tdb/record.h"

using namespace tdb;

namespace {

std::vector<std::byte> Bytes(std::initializer_list<int> v) {
  std::vector<std::byte> out;
  for (int b : v) out.push_back(static_cast<std::byte>(b));
  return out;
}

// Round-trips a single value through a one-column record.
double RoundTrip(FieldType type, double value) {
  std::vector<Field> schema = {{"x", type}};
  double vals[1] = {value};
  Record r = Record::FromValues(schema, vals);
  return r.Get(0, schema);
}

}  // namespace

TEST(RecordSizeTest, SumsFieldWidths) {
  std::vector<Field> schema = {{"a", FieldType::kInt64},
                               {"b", FieldType::kBool},
                               {"c", FieldType::kFloat64},
                               {"d", FieldType::kInt32},
                               {"e", FieldType::kFloat32}};
  EXPECT_EQ(Record::RecordSize(schema), 8u + 1u + 8u + 4u + 4u);
}

TEST(RecordSizeTest, EmptySchemaIsZero) {
  EXPECT_EQ(Record::RecordSize({}), 0u);
}

TEST(RecordRoundTripTest, Bool) {
  EXPECT_EQ(RoundTrip(FieldType::kBool, 0.0), 0.0);
  EXPECT_EQ(RoundTrip(FieldType::kBool, 1.0), 1.0);
  // Any non-zero value is stored as true.
  EXPECT_EQ(RoundTrip(FieldType::kBool, 42.0), 1.0);
  EXPECT_EQ(RoundTrip(FieldType::kBool, -3.5), 1.0);
  EXPECT_EQ(RoundTrip(FieldType::kBool, 0.001), 1.0);
}

TEST(RecordRoundTripTest, Int32) {
  for (int32_t v : {0, 1, -1, 123456, -123456,
                    std::numeric_limits<int32_t>::max(),
                    std::numeric_limits<int32_t>::min()})
    EXPECT_EQ(RoundTrip(FieldType::kInt32, v), static_cast<double>(v));
}

TEST(RecordRoundTripTest, Int32TruncatesFraction) {
  EXPECT_EQ(RoundTrip(FieldType::kInt32, 3.9), 3.0);
  EXPECT_EQ(RoundTrip(FieldType::kInt32, -3.9), -3.0);
}

TEST(RecordRoundTripTest, Int64) {
  // Values up to 2^53 are exactly representable as double.
  for (int64_t v : {int64_t{0}, int64_t{1}, int64_t{-1},
                    int64_t{1} << 31, -(int64_t{1} << 31),
                    int64_t{1} << 53, -(int64_t{1} << 53)})
    EXPECT_EQ(RoundTrip(FieldType::kInt64, static_cast<double>(v)),
              static_cast<double>(v));
}

TEST(RecordRoundTripTest, Float32) {
  EXPECT_EQ(RoundTrip(FieldType::kFloat32, 1.5), 1.5);
  EXPECT_EQ(RoundTrip(FieldType::kFloat32, -0.25), -0.25);
  EXPECT_EQ(RoundTrip(FieldType::kFloat32, 0.0), 0.0);
}

TEST(RecordRoundTripTest, Float32LosesDoublePrecision) {
  // 0.1 is not exactly representable; it comes back as the nearest float.
  const double got = RoundTrip(FieldType::kFloat32, 0.1);
  EXPECT_EQ(got, static_cast<double>(0.1f));
  EXPECT_NE(got, 0.1);
}

TEST(RecordRoundTripTest, Float64IsLossless) {
  for (double v : {0.0, 0.1, -0.1, 3.141592653589793, 1e300, -1e-300,
                   std::numeric_limits<double>::max(),
                   std::numeric_limits<double>::min(),
                   std::numeric_limits<double>::denorm_min(),
                   std::numeric_limits<double>::infinity(),
                   -std::numeric_limits<double>::infinity()})
    EXPECT_EQ(RoundTrip(FieldType::kFloat64, v), v);
}

TEST(RecordRoundTripTest, Float64PreservesNaNAndNegativeZero) {
  EXPECT_TRUE(std::isnan(
      RoundTrip(FieldType::kFloat64, std::numeric_limits<double>::quiet_NaN())));
  EXPECT_TRUE(std::signbit(RoundTrip(FieldType::kFloat64, -0.0)));
}

TEST(RecordLayoutTest, ByteSizeMatchesRecordSize) {
  std::vector<Field> schema = {{"a", FieldType::kInt64},
                               {"b", FieldType::kBool},
                               {"c", FieldType::kFloat32}};
  double vals[3] = {1, 1, 1};
  Record r = Record::FromValues(schema, vals);
  EXPECT_EQ(r.bytes().size(), Record::RecordSize(schema));
}

TEST(RecordLayoutTest, IntegersAreLittleEndianTwosComplement) {
  std::vector<Field> schema = {{"i32", FieldType::kInt32},
                               {"i64", FieldType::kInt64}};
  double vals[2] = {-2, 258};  // 258 = 0x0102
  Record r = Record::FromValues(schema, vals);
  EXPECT_EQ(r.bytes(), Bytes({0xFE, 0xFF, 0xFF, 0xFF,  //
                              0x02, 0x01, 0, 0, 0, 0, 0, 0}));
}

TEST(RecordLayoutTest, BoolIsSingleByte) {
  std::vector<Field> schema = {{"a", FieldType::kBool},
                               {"b", FieldType::kBool}};
  double vals[2] = {1, 0};
  EXPECT_EQ(Record::FromValues(schema, vals).bytes(), Bytes({1, 0}));
}

TEST(RecordLayoutTest, Float32UsesIeee754LittleEndian) {
  std::vector<Field> schema = {{"f", FieldType::kFloat32}};
  double vals[1] = {1.0};  // 0x3F800000
  EXPECT_EQ(Record::FromValues(schema, vals).bytes(),
            Bytes({0x00, 0x00, 0x80, 0x3F}));
}

TEST(RecordLayoutTest, Float64UsesIeee754LittleEndian) {
  std::vector<Field> schema = {{"f", FieldType::kFloat64}};
  double vals[1] = {1.0};  // 0x3FF0000000000000
  EXPECT_EQ(Record::FromValues(schema, vals).bytes(),
            Bytes({0, 0, 0, 0, 0, 0, 0xF0, 0x3F}));
}

TEST(RecordTest, MixedSchemaReadsEachFieldAtItsOffset) {
  std::vector<Field> schema = {{"id", FieldType::kInt64},
                               {"active", FieldType::kBool},
                               {"score", FieldType::kFloat64},
                               {"age", FieldType::kInt32},
                               {"ratio", FieldType::kFloat32}};
  double vals[5] = {1234567890123.0, 1, 3.5, -7, 0.5};
  Record r = Record::FromValues(schema, vals);
  EXPECT_EQ(r.Get(0, schema), 1234567890123.0);
  EXPECT_EQ(r.Get(1, schema), 1.0);
  EXPECT_EQ(r.Get(2, schema), 3.5);
  EXPECT_EQ(r.Get(3, schema), -7.0);
  EXPECT_EQ(r.Get(4, schema), 0.5);
}

TEST(RecordTest, EmptySchemaYieldsEmptyRecord) {
  Record r = Record::FromValues({}, {});
  EXPECT_TRUE(r.bytes().empty());
}

TEST(RecordTest, TooFewValuesThrows) {
  std::vector<Field> schema = {{"a", FieldType::kInt32},
                               {"b", FieldType::kInt32}};
  double vals[1] = {1};
  EXPECT_THROW(Record::FromValues(schema, vals), std::invalid_argument);
}

TEST(RecordTest, TooManyValuesThrows) {
  std::vector<Field> schema = {{"a", FieldType::kInt32}};
  double vals[2] = {1, 2};
  EXPECT_THROW(Record::FromValues(schema, vals), std::invalid_argument);
}

TEST(RecordTest, ArityErrorMessageReportsCounts) {
  std::vector<Field> schema = {{"a", FieldType::kInt32},
                               {"b", FieldType::kInt32}};
  double vals[1] = {1};
  try {
    Record::FromValues(schema, vals);
    FAIL() << "expected std::invalid_argument";
  } catch (const std::invalid_argument& e) {
    EXPECT_NE(std::string(e.what()).find("want 2, got 1"), std::string::npos)
        << e.what();
  }
}

TEST(RecordTest, FromRawBytesPreservesBytesAndIsReadable) {
  std::vector<Field> schema = {{"a", FieldType::kInt32},
                               {"b", FieldType::kBool}};
  Record r = Record::FromRawBytes(Bytes({0x2A, 0, 0, 0, 1}));
  EXPECT_EQ(r.bytes(), Bytes({0x2A, 0, 0, 0, 1}));
  EXPECT_EQ(r.Get(0, schema), 42.0);
  EXPECT_EQ(r.Get(1, schema), 1.0);
}

TEST(RecordTest, FromRawBytesMatchesFromValues) {
  std::vector<Field> schema = {{"a", FieldType::kInt64},
                               {"b", FieldType::kFloat64}};
  double vals[2] = {99, 2.25};
  Record a = Record::FromValues(schema, vals);
  Record b = Record::FromRawBytes(a.bytes());
  EXPECT_EQ(a.bytes(), b.bytes());
  EXPECT_EQ(b.Get(0, schema), 99.0);
  EXPECT_EQ(b.Get(1, schema), 2.25);
}
