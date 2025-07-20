cmake_minimum_required(VERSION 3.16)
project(ggml_lib C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

# 设置源文件列表
file(GLOB GGML_SRC CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/*.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/*.cpp"
)

add_library(ggml STATIC ${GGML_SRC})

target_include_directories(ggml PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/
)

# 启用后端支持（根据需要开启）
target_compile_definitions(ggml PRIVATE GGML_USE_ACCELERATE=0)
target_compile_definitions(ggml PRIVATE GGML_USE_CUBLAS=0)
target_compile_definitions(ggml PRIVATE GGML_USE_CLBLAST=0)
target_compile_definitions(ggml PRIVATE GGML_USE_METAL=0)

# 多线程支持（部分后端依赖 pthread）
find_package(Threads REQUIRED)
target_link_libraries(ggml PRIVATE Threads::Threads)