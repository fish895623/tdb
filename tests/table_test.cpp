#include <gtest/gtest.h>

#include <stdexcept>
#include <vector>

#include "tdb/table.h"

using namespace tdb;

namespace {

std::vector<Field> UserSchema() {
  return {{"id", FieldType::kInt64},
          {"active", FieldType::kBool},
          {"score", FieldType::kFloat64}};
}

}  // namespace

TEST(TableTest, NewTableIsEmpty) {
  Table t("users", UserSchema());
  EXPECT_EQ(t.Count(), 0u);
  EXPECT_TRUE(t.Records().empty());
}

TEST(TableTest, ExposesNameAndSchema) {
  Table t("users", UserSchema());
  EXPECT_EQ(t.Name(), "users");
  ASSERT_EQ(t.Schema().size(), 3u);
  EXPECT_EQ(t.Schema()[0].name, "id");
  EXPECT_EQ(t.Schema()[0].type, FieldType::kInt64);
  EXPECT_EQ(t.Schema()[1].name, "active");
  EXPECT_EQ(t.Schema()[1].type, FieldType::kBool);
  EXPECT_EQ(t.Schema()[2].name, "score");
  EXPECT_EQ(t.Schema()[2].type, FieldType::kFloat64);
}

TEST(TableTest, InsertReturnsSequentialIds) {
  Table t("users", UserSchema());
  double a[3] = {1, 1, 1.5};
  double b[3] = {2, 0, 2.5};
  double c[3] = {3, 1, 3.5};
  EXPECT_EQ(t.Insert(a), 0u);
  EXPECT_EQ(t.Insert(b), 1u);
  EXPECT_EQ(t.Insert(c), 2u);
  EXPECT_EQ(t.Count(), 3u);
}

TEST(TableTest, AtReturnsInsertedValues) {
  Table t("users", UserSchema());
  double a[3] = {10, 1, 1.5};
  double b[3] = {20, 0, 2.5};
  t.Insert(a);
  t.Insert(b);

  const auto& schema = t.Schema();
  EXPECT_EQ(t.At(0).Get(0, schema), 10.0);
  EXPECT_EQ(t.At(0).Get(1, schema), 1.0);
  EXPECT_EQ(t.At(0).Get(2, schema), 1.5);
  EXPECT_EQ(t.At(1).Get(0, schema), 20.0);
  EXPECT_EQ(t.At(1).Get(1, schema), 0.0);
  EXPECT_EQ(t.At(1).Get(2, schema), 2.5);
}

TEST(TableTest, AtOutOfRangeThrows) {
  Table t("users", UserSchema());
  EXPECT_THROW(t.At(0), std::out_of_range);  // empty table

  double a[3] = {1, 1, 1};
  t.Insert(a);
  EXPECT_NO_THROW(t.At(0));
  EXPECT_THROW(t.At(1), std::out_of_range);  // one past the end
  EXPECT_THROW(t.At(UINT64_MAX), std::out_of_range);
}

TEST(TableTest, InsertWithWrongArityThrowsAndLeavesTableUnchanged) {
  Table t("users", UserSchema());
  double ok[3] = {1, 1, 1};
  double bad[2] = {1, 1};
  t.Insert(ok);

  EXPECT_THROW(t.Insert(bad), std::invalid_argument);
  EXPECT_EQ(t.Count(), 1u);
}

TEST(TableTest, RecordsMatchInsertionOrder) {
  Table t("t", {{"n", FieldType::kInt32}});
  for (int i = 0; i < 100; ++i) {
    double v[1] = {static_cast<double>(i)};
    t.Insert(v);
  }
  ASSERT_EQ(t.Records().size(), 100u);
  for (int i = 0; i < 100; ++i)
    EXPECT_EQ(t.Records()[i].Get(0, t.Schema()), i);
}

TEST(TableTest, SetRecordsReplacesExistingRecords) {
  Table t("t", {{"n", FieldType::kInt32}});
  double v[1] = {1};
  t.Insert(v);
  t.Insert(v);

  std::vector<Record> replacement;
  double w[1] = {7};
  replacement.push_back(Record::FromValues(t.Schema(), w));
  t.SetRecords(std::move(replacement));

  ASSERT_EQ(t.Count(), 1u);
  EXPECT_EQ(t.At(0).Get(0, t.Schema()), 7.0);
}

TEST(TableTest, InsertAfterSetRecordsContinuesIds) {
  Table t("t", {{"n", FieldType::kInt32}});
  std::vector<Record> loaded;
  double w[1] = {5};
  loaded.push_back(Record::FromValues(t.Schema(), w));
  loaded.push_back(Record::FromValues(t.Schema(), w));
  t.SetRecords(std::move(loaded));

  double v[1] = {9};
  EXPECT_EQ(t.Insert(v), 2u);
}

TEST(TableTest, ZeroFieldTableAcceptsEmptyRecords) {
  Table t("empty", {});
  EXPECT_EQ(t.Insert({}), 0u);
  EXPECT_EQ(t.Insert({}), 1u);
  EXPECT_EQ(t.Count(), 2u);
  EXPECT_TRUE(t.At(0).Bytes().empty());
}
