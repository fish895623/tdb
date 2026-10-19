#include <iostream>

#include "tdb/db.h"

using namespace tdb;

int main() {
  std::cout << "Hello, World!" << std::endl;

  Database db = Database::Open("tdatabase.db");

  Table* users = db.GetTable("users");
  if (!users) {
    users = db.CreateTable(
        "users",
        {{"id", FieldType::kInt64},
         {"active", FieldType::kBool},
         {"score", FieldType::kFloat64}});
  }

  // Append two new records on every run.
  for (int i = 0; i < 2; ++i) {
    double vals[3] = {(double)users->Count() + 1, 1.0, 3.5 + i};
    users->Insert(vals);
  }

  std::cout << "Table '" << users->Name() << "' has " << users->Count()
            << " record(s):" << std::endl;
  for (uint64_t i = 0; i < users->Count(); ++i) {
    const Record& r = users->At(i);
    const auto& schema = users->Schema();
    std::cout << "  [" << i << "] id=" << r.Get(0, schema)
              << " active=" << r.Get(1, schema)
              << " score=" << r.Get(2, schema) << std::endl;
  }

  db.Flush();
  std::cout << "Database '" << db.Path() << "' flushed." << std::endl;
  return 0;
}
