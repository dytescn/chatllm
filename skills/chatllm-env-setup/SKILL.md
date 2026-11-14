---
name: chatllm-env-setup
description: chatllm 仓库在 Windows 下的工具链、项目形态、系统依赖、数据库与配置说明。
---

# chatllm 当前环境

## 1. 工作区与工具链

- 工作区：`<workspace>`（`D:\codes\github\dytescn\sys\chatllm`）
- 平台：Windows / amd64
- Rust：`rustc 1.96.0`、`cargo 1.96.0`
- Git：`2.52.0.windows.1`
- 不可用：`docker`、`go`、`make`、`rg`

## 2. 项目形态

`chatllm` 是 Rust `cdylib` + `rlib`：

- 产物：`target/release/chatllm.dll`（Linux `libchatllm.so`、macOS `libchatllm.dylib`）
- 对外接口：C ABI `post_*` 函数
- 运行时 `./libary`、`./database`、`./designdb` 均相对 cwd

构建：

```bash
cargo build --release
```

## 3. 系统依赖

`dytes.json` 的 `dependencies-sys`：

| 依赖 | 版本 | 用途 |
| --- | --- | --- |
| `database` | 1.0.6 | SQLite：`dialogue.db`、`info.db` |
| `designdb` | 1.0.4 | 每个对话 UUID 一个消息库 |
| `kv_lib` | 0.1.4 | `chat_think` / `chat_plugins` / `chat_reply` 检索 |
| `plugin_lib` | 1.0.21 | 插件能力清单 |

## 4. 数据库与配置

- 对话索引：`database/dialogue.db` → `dialogue`
- 对话消息：`designdb/<dialogue_id>` → `messages`
- AI 配置：`database/info.db` → `setting`

关键配置项：

| key | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t`；缺失或空值视为未选择 |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

`check` 已从主流程移除，后续以插件形式提供；配置缺失由 check 插件处理。

## 5. 注意事项

- 外部库路径固定为 `./libary/*`，未配置化；
- 输出缓冲区固定 64 KiB；
- 插件与系统库加载后常驻；
- 同一线程禁止插件重入；
- 临时文件放在会话 scratch 目录，不要留在仓库根目录。
