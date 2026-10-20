#include <iostream>
#include <uuid/uuid.h>

int main() {
  std::cout << "asdf" << std::endl;
  uuid_t uui;
  uuid_generate(uui);
  char uuid_str[37];
  uuid_unparse_upper(uui, uuid_str);

  std::cout << "Generated UUID v7: " << uuid_str << std::endl;

  return 0;
}
