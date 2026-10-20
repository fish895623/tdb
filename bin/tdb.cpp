#include "tdb/kv_lexer.h"

#include <iostream>
#include <string>

int main() {
  const std::string commands = "set name = hello\n";

  KeyValueLexer lexer(commands);
  std::vector<Token> tokens = lexer.Tokenize();
  for (const auto& token : tokens) {
    std::cout << token.type << ": " << token.value << std::endl;
  }
  return 0;
}
