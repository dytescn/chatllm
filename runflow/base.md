# base 基础流程

> 执行脚本：`runflow/base.ts`
> 最后验证：2026-10-31（Windows / Deno 2.9.6，真实调用 SiliconFlow）

## 目标

验证“发送信息 → 存储 → 判断 AI 配置 → 检查 token → 调用供应商 → 记录回复”：

```text
用户发送数据
      │  POST /chatllm  FFI-Symbol: chat
      ▼
chatllm 保存 user 消息
      │
      ▼
kv_search(collection = chat_plugin)
      │  命中 do_test_chat
      ▼
do_test_chat 读取 info.db 的 ai_supplier
      │
      ├─ 未配置 ai_supplier ──► 默认 deepseek
      │
      ├─ 对应 token 未配置 ───► 返回错误：
      │                          AI 配置错误：未配置 deepseek token：deepseek_app_key
      │                         chatllm 把错误写入 assistant 消息
      │
      └─ token 已配置 ────────► 调用供应商插件
                                 deepseek → libdeepseek.dll
                                 siliconflow_t2t → libsiliconflow_t2t.dll
                                        │
                                        ▼
                                 chatllm 把供应商回复写入 assistant 消息
```

user 消息始终先落库；配置错误只影响 assistant 回复，`chat` 仍返回 200。

## 组件与依赖

| 组件 | 位置 | 作用 |
| --- | --- | --- |
| `cyctron` | `testenv/cyctron.exe` | HTTP → dll 调度 |
| `chatllm` | `testenv/libary/chatllm.dll` | 保存消息；检索并调用 chat 插件；写回回复/错误 |
| `kv_lib` | `testenv/libary/kv_lib.dll` | 插件检索（`chat_plugin`） |
| `do_test_chat` | `testenv/plugins/do_test_chat/do_test_chat.dll` | 唯一安装的对话插件；判断 AI 配置并转发 |
| `siliconflow_t2t` | `testenv/libary/libsiliconflow_t2t.dll` | 已配置 token 的供应商 |
| `deepseek` | `testenv/libary/libdeepseek.dll` | 默认供应商；本测试未配置 token |
| `info_lib` / `database` | `testenv/libary/*.dll` | 读写 `info.db` 的 `ai_supplier` 与 token |

## 环境准备

```bash
# 1. 部署被测库
cargo build --release
cp target/release/chatllm.dll testenv/libary/chatllm.dll

# 2. 安装 do_test_chat
cd ../design_plugins/chat_plugins/do_test_chat && cargo build --release
cp target/release/do_test_chat.dll ../../../sys/chatllm/testenv/plugins/do_test_chat/
cp target/release/do_test_chat.dll ../../../sys/chatllm/testenv/libary/

# 3. 安装两个供应商 sys lib
cp ../deepseek/target/release/deepseek.dll ../../../sys/chatllm/testenv/libary/libdeepseek.dll
cp ../siliconflow_t2t/target/release/siliconflow_t2t.dll \
   ../../../sys/chatllm/testenv/libary/libsiliconflow_t2t.dll

# 4. 启动 testenv
cd ../../../sys/chatllm/testenv && ./cyctron.exe start
```

`info.db` 配置：

| key | 本测试取值 |
| --- | --- |
| `ai_supplier` | 先删除（走默认 deepseek），再写 `siliconflow_t2t` / `deepseek` |
| `deepseek_app_key` | 不配置（用于验证报错） |
| `silicon_app_key` | 已配置（用于验证成功回复） |

## 执行

```bash
deno run --allow-net runflow/base.ts
```

## 流程明细

### 0. 准备

- 确保 `chat_plugin` 集合存在，并写入唯一插件 `do_test_chat`（rank 1）；
- 读取 `info.db`：确认 `deepseek_app_key` 未配置、`silicon_app_key` 已配置；
- 删除 `ai_supplier`，使下一次请求走默认值。

### 1. 未配置 ai 选项：默认 deepseek，缺 token 报错

```http
POST /chatllm  FFI-Symbol: chat
{ "prompt": "你好，默认用哪个模型？" }
```

执行结果：

1. user 消息落库；
2. `do_test_chat` 读到 `ai_supplier` 为空 → 默认 `deepseek`；
3. 检查 `deepseek_app_key`：未配置；
4. 返回错误；chatllm 写入 assistant：

```text
AI 配置错误：未配置 deepseek token：deepseek_app_key
```

`chat_detail` 返回 2 条消息，`chat` 本身返回 200。

### 2. 选择 siliconflow_t2t：token 已配置

写入 `ai_supplier = siliconflow_t2t` 后再次 `chat`：

1. `do_test_chat` 选择 `siliconflow_t2t`；
2. 检查 `silicon_app_key`：已配置；
3. 调用 `./libary/libsiliconflow_t2t.dll`；
4. 供应商读取自己的配置并请求 SiliconFlow；
5. chatllm 把模型回复写入 assistant。

实测回复示例：

```text
我是：一个由深度求索公司创造的AI助手，旨在用热情、细腻的方式为你解答问题……
```

### 3. 显式选择 deepseek：缺 token 同样报错

写入 `ai_supplier = deepseek` 后 `chat`，assistant 仍为：

```text
AI 配置错误：未配置 deepseek token：deepseek_app_key
```

说明默认分支与显式选择走同一套 token 检查。

### 4. 清理

恢复 `ai_supplier = siliconflow_t2t`，归档 3 个测试对话。

## 断言清单

| 步骤 | 断言 |
| --- | --- |
| 0 | `chat_plugin` 可用；do_test_chat 安装成功；deepseek token 未配置、silicon token 已配置 |
| 1 | 清除 `ai_supplier` 后，assistant 报 `AI 配置错误`，提示 `deepseek_app_key`；user 消息已存储 |
| 2 | `ai_supplier=siliconflow_t2t` 后，assistant 为真实模型回复，无配置错误 |
| 3 | 显式 `ai_supplier=deepseek` 且无 token 时，assistant 报 `deepseek_app_key` 未配置 |
| 4 | 恢复配置并归档全部测试对话 |

## 常见问题

| 现象 | 处理 |
| --- | --- |
| assistant 为 `AI 配置错误：未配置 ...` | 配置对应 `*_app_key`，或切换 `ai_supplier` |
| 报错中的供应商不是预期 | 检查 `info.db.ai_supplier`；为空时默认 `deepseek` |
| assistant 为 `请先安装插件` | `chat_plugin` 没有 do_test_chat，先写入插件记录 |
| assistant 为加载插件失败 | 检查 `plugins/do_test_chat/`、`libary/libdeepseek.dll` 等文件 |
| `/do_test_chat` 返回 404 | cyctron 只从 `libary/` 按端点名加载；需同时复制 `do_test_chat.dll` 到 `libary/` |
| `400101 invalid utf-8` | Windows 终端用 `curl` 传中文可能变 GBK；改用脚本 |

## 执行记录

- 2026-10-31：`deno run --allow-net runflow/base.ts` 全部通过；
  默认 deepseek 缺 token 报错、显式 deepseek 报错、siliconflow 真实回复均验证；
  测试对话 `7bc0f2e6-...`、`49601505-...`、`43211c18-...` 已归档。
