# chatllm
A conversational large language model for design soft

# build and run

```bash

# 1. 创建构建目录
mkdir build && cd build

# 2. 用 Ninja 生成构建文件
cmake -G Ninja ..

# 3. 构建所有目标
ninja

# 4. 运行主程序
ninja run

# 5. 运行测试
ninja test

# 6. 清理
ninja clean

```
