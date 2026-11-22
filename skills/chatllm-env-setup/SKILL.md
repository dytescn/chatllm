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
| `database` | 1.0.6 | SQLite：`dialogue.db` |
| `designdb` | 1.0.4 | 每个对话 UUID 一个消息库 |
| `kv_lib` | 0.1.5 | `chat_think` / `chat_plugins` / `chat_reply` 检索（内置 SQLite，无需 database.dll） |
| `plugin_lib` | 1.1.0 | 插件能力清单 |

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

## 5. 测试环境

- `dytes test`（0.2.3+）在包目录生成 `.dytes/env/`，其中只有依赖包；当前包产物使用
  `target/release/` 下的本地构建文件。
- `cargo test` 直接读取 `.dytes/env/`；测试首次运行会自动建表并写入种子对话。
- 缺少依赖库时测试默认失败并提示先运行 `dytes test`；设置 `CHATLLM_TEST_ALLOW_SKIP=1`
  可退回跳过模式，`CHATLLM_TESTENV` 可指向其它测试目录。
- 目录布局、依赖下载规则与排查见 skill `chatllm-testenv`。
- `setup.json` 的请求格式、`path` 规则与 dytes 处理方式见 skill `chatllm-setup`。

## 6. 注意事项

- 外部库路径固定为 `./libary/*`，未配置化；
- 输出缓冲区 64 KiB（`kv_search` 为 1 MiB）；
- 插件与系统库加载后常驻；
- 同一线程禁止插件重入；
- 临时文件放在会话 scratch 目录，不要留在仓库根目录。
