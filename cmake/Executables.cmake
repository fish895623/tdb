# One executable per bin/*.cpp, named after the file (like Rust's src/bin/).
# Each file is a self-contained program with its own main(); shared code lives
# in include/tdb/.

file(GLOB _bin_sources CONFIGURE_DEPENDS ${PROJECT_SOURCE_DIR}/bin/*.cpp)

set(TDB_BIN_TARGETS)
foreach(src IN LISTS _bin_sources)
  get_filename_component(name ${src} NAME_WE)
  add_executable(${name} ${src})
  target_include_directories(${name} PRIVATE ${PROJECT_SOURCE_DIR}/include)
  list(APPEND TDB_BIN_TARGETS ${name})
endforeach()
