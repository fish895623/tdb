#include <fstream>
#include <iostream>

using namespace std;

int main() {
  std::cout << "Hello, World!" << std::endl;

  ofstream myfile("testing_file.txt");
  myfile << "Writing this to a file.\n";
  myfile.close();

  return 0;
}