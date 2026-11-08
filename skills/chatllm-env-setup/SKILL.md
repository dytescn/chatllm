---
name: chatllm-env-setup
description: chatllm 仓库在 Windows 下的本地开发、测试与 testenv 环境搭建说明，包括工具链、动态库依赖、数据库、cyctron 调试与已知限制。
---

# chatllm 当前环境搭建

本文件记录 `chatllm` 仓库当前可用的本地开发与测试环境。

## 1. 工作区与工具链

- 工作区：`<workspace>`（`D:\codes\github\dytescn\sys\chatllm`）
- 平台：Windows / amd64
- Rust：`rustc 1.96.0`、`cargo 1.96.0`
- Git：`2.52.0.windows.1`
- Node：`v24.16.0`，npm `12.2.1`
- Python：`3.14.5`（无 `python3` 命令）
- Deno：已安装（`~/.deno/bin/deno`），用于执行 TS 初始化与测试脚本
- 不可用：`docker`、`go`、`make`、`rg`

## 2. 项目形态

`chatllm` 是 Rust `cdylib` + `rlib`：

- 产物：`target/release/chatllm.dll`（Linux `libchatllm.so`、macOS `libchatllm.dylib`）
- 对外接口：C ABI `post_*`，由 cyctron 以 `POST /chatllm` + `FFI-Symbol` 调用
- 运行时 cwd 为 Piksel 安装根目录，`./libary`、`./database`、`./designdb` 均相对 cwd

构建与测试：

```bash
cargo build --release
cargo test
```

> `cargo clippy --all-targets -- -D warnings` 目前会被既有告警挡住：
> `src/utils/designdb.rs` 的 `empty_line_after_doc_comments`、
> `src/utils/cjson.rs` 的 `not_unsafe_ptr_arg_deref`。与业务改动无关。

## 3. 系统依赖

`dytes.json` 的 `dependencies-sys`：

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| `database` | 1.0.3 | SQLite：`dialogue.db`、`info.db` |
| `designdb` | 1.0.1 | 每个对话 UUID 一个消息库 |
| `cloud_lib` | 1.0.0 | DeepSeek / SiliconFlow 云端调用 |
| `info_lib` | 0.1.2 | `info.db.setting` / `router` 读写 |
| `kv_lib` | 0.1.2 | `chat_plugin` 等向量检索 |
| `markparse` | 1.0.1 | Markdown → `content_html` |
| `plugin_lib` | 1.0.8 | 插件能力清单（当前仅声明） |

## 4. testenv 运行环境

位置：`<workspace>/testenv`

```bash
cd testenv
./cyctron.exe start          # 监听 127.0.0.1:44944
```

运行前需要关注的目录：

| 目录 | 内容 |
| --- | --- |
| `testenv/libary/` | `database.dll`、`libdesigndb.dll`、`kv_lib.dll`、`parse.dll`、`cloud_lib.dll`、`do_test_chat.dll`、`reply_sys_info.dll` |
| `testenv/database/` | `dialogue.db`、`kv*.db`；`info.db` 可能由 `database.dll` 自动创建为空库 |
| `testenv/designdb/` | 按对话 UUID 分库的消息数据 |
| `testenv/plugins/` | `do_test_chat/`、`reply_sys_info/` |
| `testenv/models/` | 本地模型资源 |

`info_lib.dll` **不在 testenv 自带库中**。验证 `setting_ui` 对接时需要：

```bash
cd ../info_lib
cargo build --release
cp target/release/info_lib.dll ../chatllm/testenv/libary/info_lib.dll
```

## 5. 数据库与配置

- 对话索引：`database/dialogue.db` → `dialogue`
- 对话消息：`designdb/<dialogue_id>` → `messages`
- AI 配置：`database/info.db` → `setting`

关键配置项：

| key | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t`；缺失或空值视为未选择 |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

未选择 `ai_supplier` 或对应 AppKey 缺失时，`chat` 不进入后续流程，assistant 返回：

```text
请选择设置AI供应商并配置 AppKey：[前往设置](/setting)
```

响应额外带 `data.route = "/setting"`。

## 6. 测试现状

- `cargo test` 全绿：`check`、`base_message`、`base_dialogue`、`base_get_dialogue`、`base_call_chat`、`utils_*` 等。
- `base_call_chat` 额外依赖 `testenv/libary/kv_lib.dll` 与
  `testenv/plugins/do_test_chat/do_test_chat.dll`，缺失时自动跳过。
- `cargo test` 会切换进程 cwd 到 `testenv`；`database.dll` 会创建空 `info.db`，没有 `setting` 表时相关测试按环境缺失跳过。
- `setting_ui/src/setup.ts` 已实测可通过 `/info_lib` 写入：
  - 路由 500（父菜单）、501（`/setting/info`）、502（`/setting` 别名）；
  - `ai_supplier` 首次安装为未选择（`null`）；
  - `note` 分类对象写入 `designdb/setting`。
- 通过 cyctron HTTP 调 `/chatllm` 目前返回 `404 Plugin Not Found`：testenv 没有按 dytes 方式安装 `chatllm` 包。Rust 测试直接调用 FFI，不受影响。

## 7. 常用验证命令

```bash
# 编译
cargo build --release

# 全部测试
cargo test

# 只验证对话前置检查
cargo test --test base_message post_message_prompts_to_choose_ai_supplier

# call_chat 冒烟测试（需要 kv_lib 与 do_test_chat）
cargo test --test base_call_chat

# 启动 testenv
cd testenv && ./cyctron.exe start

# 调 info_lib 检查供应商配置
curl -s -X POST http://127.0.0.1:44944/info_lib \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: info_get" \
  -d '{"key":"ai_supplier"}'
```

## 8. 注意事项

- `testenv` 被 `.gitignore` 忽略；复制进去的 `info_lib.dll`、验证产生的 `info.db` 与 `designdb/setting` 属临时产物，验证后应清理。
- cyctron 服务用后台任务启动时，结束验证后要显式停止，避免端口 44944 被占用。
- 不要在仓库根目录留下探测脚本；临时文件放到会话 scratch 目录或 `testenv` 后清理。
