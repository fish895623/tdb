//
// Created by Bae Sung Hoon on 10/20/26.
//

#ifndef TDB_KV_LEXER_H
#define TDB_KV_LEXER_H
#include <spdlog/spdlog.h>

#include <ostream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

enum class TokenType {
  // Keywords
  kSet,
  kGet,
  kDel,

  // Literals & Identifiers
  kIdentifier,
  kNumber,
  kString,

  // Operators & Symbols
  kEqual,
  kEndOfFile
};

inline const char *ToString(TokenType type) {
  switch (type) {
  case TokenType::kSet:
    return "Set";
  case TokenType::kGet:
    return "Get";
  case TokenType::kDel:
    return "Del";
  case TokenType::kIdentifier:
    return "Identifier";
  case TokenType::kNumber:
    return "Number";
  case TokenType::kString:
    return "String";
  case TokenType::kEqual:
    return "Equal";
  case TokenType::kEndOfFile:
    return "EndOfFile";
  }
  return "Unknown";
}

inline std::ostream &operator<<(std::ostream &os, TokenType type) {
  return os << ToString(type);
}

struct Token {
  TokenType type;
  std::string value;
  int line;
  int column;
};

class KeyValueLexer {
public:
  explicit KeyValueLexer(std::string input) : input_(std::move(input)) {}

  // Splits the input into tokens, ending with an EndOfFile token. Throws
  // std::runtime_error on an unexpected character or unterminated string.
  std::vector<Token> Tokenize() {
    std::vector<Token> tokens;
    while (true) {
      SkipWhitespace();
      if (pos_ >= input_.size())
        break;
      tokens.push_back(Next());
    }
    tokens.push_back({TokenType::kEndOfFile, "", line_, column_});
    return tokens;
  }

private:
  char Peek() const { return input_[pos_]; }

  char Advance() {
    char c = input_[pos_++];
    if (c == '\n') {
      ++line_;
      column_ = 1;
    } else {
      ++column_;
    }
    return c;
  }

  void SkipWhitespace() {
    while (pos_ < input_.size() &&
           std::isspace(static_cast<unsigned char>(Peek()))) {
      Advance();
    }
  }

  [[noreturn]] void Fail(const std::string &what, int line, int column) const {
    throw std::runtime_error(what + " at " + std::to_string(line) + ":" +
                             std::to_string(column));
  }

  Token Next() {
    const int line = line_;
    const int column = column_;
    const unsigned char c = static_cast<unsigned char>(Peek());

    if (c == '=') {
      Advance();
      return {TokenType::kEqual, "=", line, column};
    }
    if (c == '"')
      return ReadString(line, column);
    if (std::isdigit(c) ||
        (c == '-' && pos_ + 1 < input_.size() &&
         std::isdigit(static_cast<unsigned char>(input_[pos_ + 1])))) {
      return ReadNumber(line, column);
    }
    if (std::isalpha(c) || c == '_')
      return ReadWord(line, column);

    Fail(std::string("unexpected character '") + static_cast<char>(c) + "'",
         line, column);
  }

  Token ReadString(int line, int column) {
    Advance(); // opening quote
    std::string value;
    while (pos_ < input_.size() && Peek() != '"')
      value += Advance();
    if (pos_ >= input_.size())
      Fail("unterminated string", line, column);
    Advance(); // closing quote
    return {TokenType::kString, std::move(value), line, column};
  }

  Token ReadNumber(int line, int column) {
    std::string value(1, Advance()); // first digit or '-'
    bool seen_dot = false;
    while (pos_ < input_.size()) {
      const unsigned char c = static_cast<unsigned char>(Peek());
      if (std::isdigit(c)) {
        value += Advance();
      } else if (c == '.' && !seen_dot) {
        seen_dot = true;
        value += Advance();
      } else {
        break;
      }
    }
    return {TokenType::kNumber, std::move(value), line, column};
  }

  Token ReadWord(int line, int column) {
    std::string value;
    while (pos_ < input_.size()) {
      const unsigned char c = static_cast<unsigned char>(Peek());
      if (!std::isalnum(c) && c != '_')
        break;
      value += Advance();
    }
    TokenType type = TokenType::kIdentifier;
    if (value == "set")
      type = TokenType::kSet;
    else if (value == "get")
      type = TokenType::kGet;
    else if (value == "del")
      type = TokenType::kDel;
    return {type, std::move(value), line, column};
  }

  std::string input_;
  size_t pos_ = 0;
  int line_ = 1;
  int column_ = 1;
};

inline void Hello() { SPDLOG_INFO("Hello"); };

#endif // TDB_KV_LEXER_H
