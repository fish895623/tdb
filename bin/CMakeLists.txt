# One executable per *.cpp here, named after the file (like Rust's src/bin/).
# Each file is a self-contained program with its own main(); shared code lives
# in include/tdb/.

file(GLOB sources CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/*.cpp)

set(targets)
foreach(src IN LISTS sources)
  get_filename_component(name ${src} NAME_WE)
  add_executable(${name} ${src})
  add_compile_definitions(SPDLOG_ACTIVE_LEVEL=SPDLOG_LEVEL_TRACE)
  target_include_directories(${name} PRIVATE ${PROJECT_SOURCE_DIR}/include)
  target_link_libraries(${name} PRIVATE spdlog::spdlog)
  list(APPEND targets ${name})
endforeach()

# Read by cmake/Coverage.cmake.
set(TDB_BIN_TARGETS ${targets} PARENT_SCOPE)
