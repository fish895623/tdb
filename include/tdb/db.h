#pragma once

#include <sqlite3.h>

#include <stdexcept>
#include <string>

namespace tdb {

// Database is inline: it lives in memory (SQLite ":memory:"),
// no file is created. The name is used only for identification.
inline constexpr const char* kDatabaseName = "tdatabase";

// RAII wrapper around a SQLite in-memory connection.
// Implementation is inline (header-only).
class Database {
 public:
  inline Database() {
    // Inline (in-memory) database: no file on disk.
    if (sqlite3_open(":memory:", &db_) != SQLITE_OK) {
      std::string err = db_ ? sqlite3_errmsg(db_) : "unknown";
      sqlite3_close(db_);
      db_ = nullptr;
      throw std::runtime_error("failed to open in-memory database: " + err);
    }
  }

  inline ~Database() {
    if (db_) sqlite3_close(db_);
  }

  inline Database(const Database&) = delete;
  inline Database& operator=(const Database&) = delete;

  // Create the initial schema for the tdatabase database.
  inline void InitSchema() const {
    Execute("CREATE TABLE IF NOT EXISTS files ("
            "  id INTEGER PRIMARY KEY AUTOINCREMENT,"
            "  name TEXT NOT NULL UNIQUE,"
            "  data BLOB NOT NULL,"
            "  created_at TEXT NOT NULL DEFAULT (datetime('now'))"
            ");");
  }

  // Execute a single SQL statement.
  inline void Execute(const std::string& sql) const {
    char* err = nullptr;
    if (sqlite3_exec(db_, sql.c_str(), nullptr, nullptr, &err) != SQLITE_OK) {
      std::string msg = err ? err : "unknown";
      sqlite3_free(err);
      throw std::runtime_error("sqlite error: " + msg);
    }
  }

  inline const sqlite3* Handle() const { return db_; }

 private:
  sqlite3* db_ = nullptr;
};

}  // namespace tdb
