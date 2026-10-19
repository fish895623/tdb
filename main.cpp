#include <iostream>

#include "tdb/db.h"

int main() {
  std::cout << "Hello, World!" << std::endl;

  try {
    tdb::Database db;  // inline (in-memory) database
    db.InitSchema();
    std::cout << "Database '" << tdb::kDatabaseName << "' ready." << std::endl;
  } catch (const std::exception& e) {
    std::cerr << e.what() << std::endl;
    return 1;
  }

  return 0;
}
