cmake_minimum_required(VERSION 3.16)
project(ggml_test C CXX)

# 设置 C 和 C++ 标准
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 设置输出目录
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)

# 包含全局头文件路径
include_directories(${PROJECT_SOURCE_DIR}/include)

# 添加 ggml 静态库子目录（cpkg/ggml.c 应存在）
add_subdirectory(cpkg)

# 添加测试程序 test_basic
add_executable(test_basic tests/test_basic.c)
target_include_directories(test_basic PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_libraries(test_basic PRIVATE ggml)

# 添加示例程序
add_executable(simple examples/simple.c)
target_include_directories(simple PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_libraries(simple PRIVATE ggml)


# 添加主程序 chatllm
add_executable(chatllm src/chatllm.c)
target_include_directories(chatllm PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_libraries(chatllm PRIVATE ggml)

# 安装 chatllm 到 bin 目录
install(TARGETS chatllm
        RUNTIME DESTINATION bin)

# 可选：安装头文件
install(DIRECTORY ${PROJECT_SOURCE_DIR}/include/ DESTINATION include)

# 可选：安装 ggml 静态库（如果需要给其他项目使用）
install(TARGETS ggml
        ARCHIVE DESTINATION lib)
