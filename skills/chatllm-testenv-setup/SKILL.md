---
name: chatllm-testenv-setup
description: 从零搭建 chatllm 测试环境：准备 testenv 动态库、编译并拷贝 info_lib、启动 cyctron、初始化 dialogue/info 数据库、写入 AI 供应商配置并运行 cargo test。
---

# chatllm 测试环境搭建

适用于 Windows 本地调试 `chatllm`。运行目录约定为 `<workspace>/testenv`，
所有 `./libary`、`./database`、`./designdb` 都相对该目录解析。

## 1. 前置条件

- Rust 工具链可用：`cargo`、`rustc`
- Deno 可用（执行 `testenv/*.ts`）；也可用 Node 24 运行不含类型注解的脚本
- `<workspace>/testenv/cyctron.exe` 已存在
- 端口 `44944` 未被占用

检查端口：

```bash
netstat -ano | grep 44944
```

## 2. 准备 testenv 目录

```text
testenv/
  cyctron.exe
  setup.ts          # 本地建 dialogue 表（等价仓库根 setup.json）
  seed.ts           # 插入基础对话
  clear_db.ts       # 清空 dialogue 库
  libary/           # 动态库
  database/         # SQLite：dialogue.db、info.db、kv*.db
  designdb/         # 每个对话 UUID 一个消息库
  plugins/          # do_test_chat、reply_sys_info
  models/
```

`testenv/` 被 `.gitignore` 忽略，可随时重建。

## 3. 放置动态库

必需库（放入 `testenv/libary/`）：

| 文件 | 来源 / 说明 |
| --- | --- |
| `database.dll` | SQLite 访问 |
| `libdesigndb.dll` | 对话消息库 |
| `kv_lib.dll` | `chat_plugin` 检索 |
| `parse.dll` | `content_html` 生成 |
| `cloud_lib.dll` | DeepSeek / SiliconFlow 云端调用 |
| `do_test_chat.dll` | 测试聊天插件 |
| `reply_sys_info.dll` | 回复组装插件 |
| `info_lib.dll` | `info.db` 的 `setting` / `router` 读写 |

`cloud_lib.dll` 从同级仓库构建：

```bash
cd ../cloud_lib
cargo build --release
cp target/release/cloud_lib.dll ../chatllm/testenv/libary/cloud_lib.dll
```

`info_lib.dll` 通常不在 testenv 自带库中，需要从同级仓库构建：

```bash
cd ../info_lib
cargo build --release
cp target/release/info_lib.dll ../chatllm/testenv/libary/info_lib.dll
```

插件以目录形式放在 `testenv/plugins/<name>/<name>.dll`：

```text
testenv/plugins/do_test_chat/do_test_chat.dll
testenv/plugins/reply_sys_info/reply_sys_info.dll
```

## 4. 编译 chatllm

```bash
cd <workspace>
cargo build --release
```

产物为 `target/release/chatllm.dll`。Rust 集成测试直接链接 crate，
不要求把 DLL 放进 testenv；只有通过 cyctron HTTP 调用时才需要安装 `chatllm` 包。

## 5. 启动宿主

```bash
cd <workspace>/testenv
./cyctron.exe start
```

服务地址：`http://127.0.0.1:44944`。用后台任务启动时，验证结束后要显式停止，
避免端口被占用。

## 6. 初始化 dialogue 数据库

```bash
cd testenv
deno run --allow-net clear_db.ts   # 可选：清空所有用户表
deno run --allow-net setup.ts      # 创建 dialogue 表
deno run --allow-net seed.ts       # 插入 4 条基础对话
```

也可以：

```bash
node clear_db.ts && node setup.ts && node seed.ts
```

`seed.ts` 使用固定 UUID 和 `INSERT OR REPLACE`，重复执行幂等。
`show_status = 2` 与 `is_archived = 1` 的数据用于验证列表过滤。

## 7. 初始化 info.db 与 AI 供应商

`database.dll` 在加载时可能创建空的 `testenv/database/info.db`；空库没有
`setting` 表。推荐通过 `info_lib` 写入，表和分类会惰性创建：

```bash
curl -s -X POST http://127.0.0.1:44944/info_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: info_post" \
  -d '{
    "key":"ai_supplier",
    "title":"AI 对话供应商",
    "value":"deepseek",
    "value_type":"enum",
    "category":"base",
    "description":"可选：deepseek / siliconflow_t2t",
    "note":{"key":"base","language":"default","title":"基础","description":"系统基础设置"}
  }'
```

再写入对应 token：

```bash
curl -s -X POST http://127.0.0.1:44944/info_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: info_post" \
  -d '{"key":"deepseek_app_key","title":"DeepSeek Token","value":"<token>","value_type":"secret","category":"base"}'
```

关键约定：

| 配置 | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t`；空值或未知值视为未选择 |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

未选择 `ai_supplier` 或对应 AppKey 缺失时，`chat` 不进入后续流程，assistant 返回
`请选择设置AI供应商并配置 AppKey：[前往设置](/setting)`，响应带 `data.route = "/setting"`。
因此做真实供应商联调前，必须先写入 `ai_supplier` 与对应的 `*_app_key`。

也可直接使用 SQL（`database` 的 `db_update`）写入 `setting` 表，
但表结构需与 `info_lib` 一致。

## 8. 运行 Rust 测试

在仓库根目录执行即可：

```bash
cd <workspace>
cargo test
```

`tests/common/mod.rs::db_ready()` 会在首次调用时把进程 cwd 切到 `testenv`，
然后直接 `dlopen` `./libary/database.dll`。缺少
`testenv/libary/database.dll` 或 `testenv/database/dialogue.db` 时，
依赖数据库的测试自动跳过并打印原因。

`call_chat` 冒烟测试使用 `tests/common/mod.rs::kv_ready()`，额外要求
`testenv/libary/kv_lib.dll` 与 `testenv/plugins/do_test_chat/do_test_chat.dll`。

只跑某个用例：

```bash
cargo test --test base_message
cargo test --test base_message post_message_prompts_to_choose_ai_supplier
cargo test --test base_call_chat
```

## 9. 通过 HTTP 验证

`info_lib`：

```bash
curl -s -X POST http://127.0.0.1:44944/info_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: info_get" \
  -d '{"key":"ai_supplier"}'
```

`chatllm`：

```bash
curl -s -X POST http://127.0.0.1:44944/chatllm \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: chat" \
  -d '{"prompt":"你好","title":"smoke"}'
```

直接调用插件（跳过 think / reply）：

```bash
curl -s -X POST http://127.0.0.1:44944/chatllm \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: call_chat" \
  -d '{"category":"chat_plugin","prompts":"do_test_chat","context":{}}'
```

注意：仅把 `chatllm.dll` 拷进 `testenv/libary` 或 `plugins/` 不会被 cyctron 识别，
`/chatllm` 会返回 `{"error":404,"message":"Plugin Not Found"}`。
需要按 dytes 的包安装方式把 `chatllm` 安装到 testenv。
Rust 测试不受此限制。

## 10. 可选：安装 do_test_chat 插件向量

真实联调供应商时可先写入 `chat_plugin` 集合，让 `chat` 在 think 命中后
检索到 Top1 插件；若 think 未命中，按当前流程会跳过 plugins。
集合不存在时先创建：

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: kv_collection_put" \
  -d '{
    "code":"chat_plugin",
    "name":"聊天插件",
    "description":"chat 分类插件的能力描述向量",
    "dim":1024,
    "model":"qwen3-embed"
  }'
```

再写入插件记录：

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: kv_put" \
  -d '{
    "collection":"chat_plugin",
    "items":[{
      "key":"chat-plugin-do-test-chat",
      "value":"{\"name\":\"do_test_chat\",\"lib_name\":\"do_test_chat\",\"ffi_symbol\":\"chat\",\"category\":\"chat\"}",
      "value_type":"json",
      "meta":{"plugin":"do_test_chat","category":"chat"}
    }]
  }'
```

完整流程参考 `runflow/base.md`。

## 11. 重置与清理

```bash
# 清空对话库
cd testenv && deno run --allow-net clear_db.ts

# 删除测试产生的临时配置库（谨慎，确认后执行）
rm -f database/info.db
rm -rf designdb/setting

# 删除手动拷入的 info_lib.dll
rm -f libary/info_lib.dll
```

## 12. 常见问题

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| `Plugin Not Found`（`/chatllm`） | testenv 未安装 chatllm 包 | 用 dytes 安装包，或改用 `cargo test` 的 FFI 测试 |
| `no such table: setting` | `info.db` 为空库 | 先通过 `info_lib.info_post` 建表并写入配置 |
| 对话一直返回“请选择设置AI供应商” | `ai_supplier` 缺失或空值 | 写入 `deepseek` / `siliconflow_t2t` |
| `AI 配置错误：未配置 ... token` | 供应商 token 缺失 | 写入 `deepseek_app_key` / `silicon_app_key` |
| 数据库测试全部跳过 | `database.dll` 或 `dialogue.db` 缺失 | 检查 `testenv/libary`、先跑 `setup.ts` |
| 端口 44944 被占用 | 已有一个 cyctron 在运行 | 停止旧进程后再启动 |
| `cargo clippy -D warnings` 失败 | 既有告警：`designdb.rs` doc comment、`cjson.rs` raw pointer | 与测试环境无关，按需单独处理 |
