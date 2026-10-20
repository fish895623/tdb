#include <gtest/gtest.h>

#include <atomic>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <stdexcept>
#include <string>
#include <vector>

#include "tdb/db.h"

using namespace tdb;
namespace fs = std::filesystem;

namespace {

// Gives each test its own scratch file and removes it afterwards.
class DatabaseTest : public ::testing::Test {
 protected:
  void SetUp() override {
    static std::atomic<int> counter{0};
    const auto* info = ::testing::UnitTest::GetInstance()->current_test_info();
    path_ = (fs::temp_directory_path() /
             ("tdb_test_" + std::string(info->name()) + "_" +
              std::to_string(counter++) + ".db"))
                .string();
    fs::remove(path_);
  }

  void TearDown() override { fs::remove(path_); }

  std::vector<uint8_t> ReadFile() const {
    std::ifstream f(path_, std::ios::binary);
    return {std::istreambuf_iterator<char>(f), std::istreambuf_iterator<char>()};
  }

  void WriteFile(const std::vector<uint8_t>& bytes) const {
    std::ofstream f(path_, std::ios::binary | std::ios::trunc);
    f.write(reinterpret_cast<const char*>(bytes.data()),
            static_cast<std::streamsize>(bytes.size()));
  }

  // Cuts `n` bytes off the end of the file.
  void TruncateBy(size_t n) const {
    fs::resize_file(path_, fs::file_size(path_) - n);
  }

  std::string path_;
};

// Header bytes shared by hand-built files: "TDB1" + version 1.
std::vector<uint8_t> Header(uint32_t version = 1) {
  return {'T', 'D', 'B', '1',
          static_cast<uint8_t>(version), 0, 0, 0};
}

void Append(std::vector<uint8_t>* v, std::initializer_list<uint8_t> bytes) {
  v->insert(v->end(), bytes.begin(), bytes.end());
}

}  // namespace

// --- opening ---

TEST_F(DatabaseTest, OpenMissingFileStartsEmpty) {
  Database db = Database::Open(path_);
  EXPECT_TRUE(db.Tables().empty());
  EXPECT_EQ(db.Path(), path_);
  EXPECT_FALSE(fs::exists(path_)) << "Open must not create the file";
}

TEST_F(DatabaseTest, OpenEmptyFileThrows) {
  WriteFile({});
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, OpenWithBadMagicThrows) {
  WriteFile({'N', 'O', 'P', 'E', 1, 0, 0, 0, 0, 0, 0, 0});
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, OpenWithShortMagicThrows) {
  WriteFile({'T', 'D'});
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, OpenWithUnsupportedVersionThrows) {
  auto bytes = Header(2);
  Append(&bytes, {0, 0, 0, 0});
  WriteFile(bytes);
  try {
    Database::Open(path_);
    FAIL() << "expected std::runtime_error";
  } catch (const std::runtime_error& e) {
    EXPECT_NE(std::string(e.what()).find("version"), std::string::npos)
        << e.what();
  }
}

// --- table management ---

TEST_F(DatabaseTest, CreateTableThenGetTable) {
  Database db = Database::Open(path_);
  Table* t = db.CreateTable("users", {{"id", FieldType::kInt64}});
  ASSERT_NE(t, nullptr);
  EXPECT_EQ(t->Name(), "users");
  EXPECT_EQ(db.GetTable("users"), t);
  EXPECT_EQ(db.Tables().size(), 1u);
}

TEST_F(DatabaseTest, GetTableMissingReturnsNull) {
  Database db = Database::Open(path_);
  EXPECT_EQ(db.GetTable("nope"), nullptr);
  db.CreateTable("a", {});
  EXPECT_EQ(db.GetTable("b"), nullptr);
}

TEST_F(DatabaseTest, GetTableIsCaseSensitive) {
  Database db = Database::Open(path_);
  db.CreateTable("Users", {});
  EXPECT_EQ(db.GetTable("users"), nullptr);
  EXPECT_NE(db.GetTable("Users"), nullptr);
}

TEST_F(DatabaseTest, CreateDuplicateTableThrows) {
  Database db = Database::Open(path_);
  db.CreateTable("users", {{"id", FieldType::kInt64}});
  EXPECT_THROW(db.CreateTable("users", {{"other", FieldType::kBool}}),
               std::invalid_argument);
  EXPECT_EQ(db.Tables().size(), 1u);
  // The original table is untouched.
  EXPECT_EQ(db.GetTable("users")->Schema()[0].name, "id");
}

TEST_F(DatabaseTest, TablePointersStayValidAsMoreTablesAreAdded) {
  Database db = Database::Open(path_);
  Table* first = db.CreateTable("t0", {{"n", FieldType::kInt32}});
  for (int i = 1; i < 50; ++i)
    db.CreateTable("t" + std::to_string(i), {{"n", FieldType::kInt32}});
  EXPECT_EQ(db.GetTable("t0"), first);
  EXPECT_EQ(first->Name(), "t0");
}

TEST_F(DatabaseTest, TablesAreIndependent) {
  Database db = Database::Open(path_);
  Table* a = db.CreateTable("a", {{"n", FieldType::kInt32}});
  Table* b = db.CreateTable("b", {{"n", FieldType::kInt32}});
  double v[1] = {1};
  a->Insert(v);
  a->Insert(v);
  EXPECT_EQ(a->Count(), 2u);
  EXPECT_EQ(b->Count(), 0u);
}

// --- persistence ---

TEST_F(DatabaseTest, FlushCreatesFileWithMagic) {
  Database db = Database::Open(path_);
  db.Flush();
  auto bytes = ReadFile();
  ASSERT_GE(bytes.size(), 4u);
  EXPECT_EQ(std::string(bytes.begin(), bytes.begin() + 4), "TDB1");
}

TEST_F(DatabaseTest, EmptyDatabaseRoundTrips) {
  Database::Open(path_).Flush();
  Database db = Database::Open(path_);
  EXPECT_TRUE(db.Tables().empty());
}

TEST_F(DatabaseTest, EmptyDatabaseHasExactBytes) {
  Database::Open(path_).Flush();
  auto expected = Header();
  Append(&expected, {0, 0, 0, 0});  // table_count = 0
  EXPECT_EQ(ReadFile(), expected);
}

TEST_F(DatabaseTest, FlushWritesDocumentedFormat) {
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("t", {{"a", FieldType::kInt32}});
    double v[1] = {7};
    t->Insert(v);
    db.Flush();
  }

  auto expected = Header();
  Append(&expected, {1, 0, 0, 0});                 // table_count
  Append(&expected, {1, 't'});                     // name_len, name
  Append(&expected, {1, 0, 0, 0});                 // field_count
  Append(&expected, {1, 'a', 2});                  // field: len, name, kInt32
  Append(&expected, {1, 0, 0, 0, 0, 0, 0, 0});     // record_count (u64)
  Append(&expected, {4, 0, 0, 0});                 // record byte_len
  Append(&expected, {7, 0, 0, 0});                 // record bytes
  EXPECT_EQ(ReadFile(), expected);
}

TEST_F(DatabaseTest, LoadsHandBuiltFile) {
  auto bytes = Header();
  Append(&bytes, {1, 0, 0, 0});
  Append(&bytes, {1, 't'});
  Append(&bytes, {1, 0, 0, 0});
  Append(&bytes, {1, 'a', 2});
  Append(&bytes, {2, 0, 0, 0, 0, 0, 0, 0});  // two records
  Append(&bytes, {4, 0, 0, 0, 0xFE, 0xFF, 0xFF, 0xFF});  // -2
  Append(&bytes, {4, 0, 0, 0, 0x2A, 0, 0, 0});           // 42
  WriteFile(bytes);

  Database db = Database::Open(path_);
  Table* t = db.GetTable("t");
  ASSERT_NE(t, nullptr);
  ASSERT_EQ(t->Count(), 2u);
  EXPECT_EQ(t->At(0).Get(0, t->Schema()), -2.0);
  EXPECT_EQ(t->At(1).Get(0, t->Schema()), 42.0);
}

TEST_F(DatabaseTest, RoundTripsAllFieldTypes) {
  const std::vector<Field> schema = {{"flag", FieldType::kBool},
                                     {"i32", FieldType::kInt32},
                                     {"i64", FieldType::kInt64},
                                     {"f32", FieldType::kFloat32},
                                     {"f64", FieldType::kFloat64}};
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("all", schema);
    double r0[5] = {1, -123456, 1234567890123.0, 0.5, 3.141592653589793};
    double r1[5] = {0, 2147483647, -9007199254740992.0, -2.25, -1e-300};
    t->Insert(r0);
    t->Insert(r1);
    db.Flush();
  }

  Database db = Database::Open(path_);
  Table* t = db.GetTable("all");
  ASSERT_NE(t, nullptr);

  ASSERT_EQ(t->Schema().size(), schema.size());
  for (size_t i = 0; i < schema.size(); ++i) {
    EXPECT_EQ(t->Schema()[i].name, schema[i].name) << "field " << i;
    EXPECT_EQ(t->Schema()[i].type, schema[i].type) << "field " << i;
  }

  ASSERT_EQ(t->Count(), 2u);
  const auto& s = t->Schema();
  EXPECT_EQ(t->At(0).Get(0, s), 1.0);
  EXPECT_EQ(t->At(0).Get(1, s), -123456.0);
  EXPECT_EQ(t->At(0).Get(2, s), 1234567890123.0);
  EXPECT_EQ(t->At(0).Get(3, s), 0.5);
  EXPECT_EQ(t->At(0).Get(4, s), 3.141592653589793);
  EXPECT_EQ(t->At(1).Get(0, s), 0.0);
  EXPECT_EQ(t->At(1).Get(1, s), 2147483647.0);
  EXPECT_EQ(t->At(1).Get(2, s), -9007199254740992.0);
  EXPECT_EQ(t->At(1).Get(3, s), -2.25);
  EXPECT_EQ(t->At(1).Get(4, s), -1e-300);
}

TEST_F(DatabaseTest, RoundTripsMultipleTablesInOrder) {
  {
    Database db = Database::Open(path_);
    Table* a = db.CreateTable("alpha", {{"x", FieldType::kInt32}});
    Table* b = db.CreateTable("beta", {{"y", FieldType::kFloat64},
                                       {"z", FieldType::kBool}});
    double av[1] = {11};
    double bv[2] = {2.5, 1};
    a->Insert(av);
    b->Insert(bv);
    b->Insert(bv);
    db.Flush();
  }

  Database db = Database::Open(path_);
  ASSERT_EQ(db.Tables().size(), 2u);
  EXPECT_EQ(db.Tables()[0]->Name(), "alpha");
  EXPECT_EQ(db.Tables()[1]->Name(), "beta");
  EXPECT_EQ(db.GetTable("alpha")->Count(), 1u);
  EXPECT_EQ(db.GetTable("beta")->Count(), 2u);
  EXPECT_EQ(db.GetTable("beta")->At(1).Get(0, db.GetTable("beta")->Schema()),
            2.5);
}

TEST_F(DatabaseTest, RoundTripsTableWithNoRecords) {
  {
    Database db = Database::Open(path_);
    db.CreateTable("empty", {{"n", FieldType::kInt64}});
    db.Flush();
  }
  Database db = Database::Open(path_);
  Table* t = db.GetTable("empty");
  ASSERT_NE(t, nullptr);
  EXPECT_EQ(t->Count(), 0u);
  ASSERT_EQ(t->Schema().size(), 1u);
  EXPECT_EQ(t->Schema()[0].name, "n");
}

TEST_F(DatabaseTest, RoundTripsZeroFieldTableWithRecords) {
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("nofields", {});
    t->Insert({});
    t->Insert({});
    t->Insert({});
    db.Flush();
  }
  Database db = Database::Open(path_);
  Table* t = db.GetTable("nofields");
  ASSERT_NE(t, nullptr);
  EXPECT_TRUE(t->Schema().empty());
  EXPECT_EQ(t->Count(), 3u);
}

TEST_F(DatabaseTest, RoundTripsManyRecords) {
  constexpr int kN = 5000;
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("big", {{"i", FieldType::kInt64},
                                      {"sq", FieldType::kFloat64}});
    for (int i = 0; i < kN; ++i) {
      double v[2] = {static_cast<double>(i), static_cast<double>(i) * i};
      t->Insert(v);
    }
    db.Flush();
  }
  Database db = Database::Open(path_);
  Table* t = db.GetTable("big");
  ASSERT_NE(t, nullptr);
  ASSERT_EQ(t->Count(), static_cast<uint64_t>(kN));
  for (int i = 0; i < kN; i += 499) {
    EXPECT_EQ(t->At(i).Get(0, t->Schema()), i);
    EXPECT_EQ(t->At(i).Get(1, t->Schema()), static_cast<double>(i) * i);
  }
  EXPECT_EQ(t->At(kN - 1).Get(0, t->Schema()), kN - 1);
}

TEST_F(DatabaseTest, RoundTripsMaxLengthAndUtf8Names) {
  const std::string long_name(255, 'x');  // name_len is a single byte
  const std::string utf8 = "사용자";       // length counted in bytes
  {
    Database db = Database::Open(path_);
    db.CreateTable(long_name, {{long_name, FieldType::kBool}});
    db.CreateTable(utf8, {{utf8, FieldType::kInt32}});
    db.Flush();
  }
  Database db = Database::Open(path_);
  Table* a = db.GetTable(long_name);
  ASSERT_NE(a, nullptr);
  EXPECT_EQ(a->Schema()[0].name, long_name);
  Table* b = db.GetTable(utf8);
  ASSERT_NE(b, nullptr);
  EXPECT_EQ(b->Schema()[0].name, utf8);
}

TEST_F(DatabaseTest, RecordsAccumulateAcrossSessions) {
  // Mirrors how bin/tdbd.cpp uses the database: reopen, append, flush.
  for (int session = 0; session < 3; ++session) {
    Database db = Database::Open(path_);
    Table* t = db.GetTable("log");
    if (!t) t = db.CreateTable("log", {{"session", FieldType::kInt32}});
    double v[1] = {static_cast<double>(session)};
    t->Insert(v);
    t->Insert(v);
    db.Flush();
  }
  Database db = Database::Open(path_);
  Table* t = db.GetTable("log");
  ASSERT_NE(t, nullptr);
  ASSERT_EQ(t->Count(), 6u);
  for (uint64_t i = 0; i < 6; ++i)
    EXPECT_EQ(t->At(i).Get(0, t->Schema()), static_cast<double>(i / 2));
}

TEST_F(DatabaseTest, FlushOverwritesPreviousContents) {
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("t", {{"n", FieldType::kInt64}});
    for (int i = 0; i < 100; ++i) {
      double v[1] = {static_cast<double>(i)};
      t->Insert(v);
    }
    db.Flush();
  }
  const auto big = fs::file_size(path_);

  {
    Database db = Database::Open(path_);
    ASSERT_EQ(db.GetTable("t")->Count(), 100u);
    db.GetTable("t")->SetRecords({});
    db.Flush();
  }
  // The file is truncated on flush, so no stale tail bytes survive.
  EXPECT_LT(fs::file_size(path_), big);
  Database db = Database::Open(path_);
  ASSERT_NE(db.GetTable("t"), nullptr);
  EXPECT_EQ(db.GetTable("t")->Count(), 0u);
}

TEST_F(DatabaseTest, FlushToUnwritablePathThrows) {
  const std::string bad =
      (fs::temp_directory_path() / "tdb_no_such_dir" / "x.db").string();
  Database db = Database::Open(bad);
  EXPECT_THROW(db.Flush(), std::runtime_error);
}

// --- corruption ---

TEST_F(DatabaseTest, TruncatedRecordDataThrows) {
  {
    Database db = Database::Open(path_);
    Table* t = db.CreateTable("t", {{"n", FieldType::kInt64}});
    double v[1] = {1};
    t->Insert(v);
    t->Insert(v);
    db.Flush();
  }
  TruncateBy(3);  // cut into the last record's bytes
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, TruncatedStringThrows) {
  auto bytes = Header();
  Append(&bytes, {1, 0, 0, 0});    // one table
  Append(&bytes, {5, 'a', 'b'});   // name_len=5 but only 2 bytes follow
  WriteFile(bytes);
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, TruncatedFieldNameThrows) {
  auto bytes = Header();
  Append(&bytes, {1, 0, 0, 0});
  Append(&bytes, {1, 't'});
  Append(&bytes, {1, 0, 0, 0});    // one field
  Append(&bytes, {9, 'a'});        // field name_len=9, 1 byte present
  WriteFile(bytes);
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}

TEST_F(DatabaseTest, RecordLengthPastEndOfFileThrows) {
  auto bytes = Header();
  Append(&bytes, {1, 0, 0, 0});
  Append(&bytes, {1, 't'});
  Append(&bytes, {1, 0, 0, 0});
  Append(&bytes, {1, 'a', 2});
  Append(&bytes, {1, 0, 0, 0, 0, 0, 0, 0});
  Append(&bytes, {0xFF, 0, 0, 0});  // claims 255 bytes, none follow
  WriteFile(bytes);
  EXPECT_THROW(Database::Open(path_), std::runtime_error);
}
