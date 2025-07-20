cmake_minimum_required(VERSION 3.16)
project(ggml_test C CXX)

# 包含头文件路径
include_directories(${PROJECT_SOURCE_DIR}/include)

# 使用 Ninja 或默认编译器
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

# 添加子目录 cpkg，里面会构建 ggml 库
add_subdirectory(cpkg)


# 添加测试程序
add_executable(test_basic tests/test_basic.c)

# 链接 ggml 静态库
target_link_libraries(test_basic PRIVATE ggml)
