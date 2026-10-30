# chatllm

本地对话存储能力库。Rust `cdylib`，导出 C ABI 的 `post_*` 函数，
由 cyctron 以 `POST /chatllm` + `FFI-Symbol` 加载。运行时 cwd 是 Piksel 安装根目录，
`./libary`、`./database`、`./designdb` 都相对该目录解析。

当前版本：**1.0.6**（`Cargo.toml` 与 `dytes.json` 一致）。

## 能力

| FFI-Symbol | 导出函数 | 说明 |
| --- | --- | --- |
| `chat` | `post_chat` | 新建 / 续聊并写入 user 消息；不调用模型、不生成回复、不注入 skills |
| `dialogue_list` | `post_dialogue_list` | 对话分页列表（排除归档与 `show_status = 2`） |
| `dialogue_update` | `post_dialogue_update` | 更新标题、关键字、模型、提示词、模型参数、置顶 / 归档等 |
| `dialogue_search` | `post_dialogue_search` | 按 `title` / `keyword` 分页搜索 |
| `chat_detail` | `post_chat_detail` | 读取某对话消息，支持 `before_id` / `after_id` 游标分页 |

所有接口统一返回 `{code, data, msg}`，成功 `code = 200`；结果写入调用方提供的 64 KiB 缓冲区。

> `embed_prompts` / `turbo_search_query`、siliconflow 模型调用、skills 注入与向量能力均已移除。

## 存储

| 数据 | 位置 |
| --- | --- |
| 对话索引 | `database/dialogue.db` → `dialogue` |
| 对话消息 | `designdb/<dialogue_id>` → `messages`（PoloDB 逻辑库，按对话 UUID 分库） |

## 外部依赖

| 依赖 | 用途 |
| --- | --- |
| `./libary/database.dll`（`.so`） | SQLite 访问：`dialogue.db` |
| `./libary/libdesigndb.dll`（`.so`） | 每对话消息库（PoloDB） |
| `./libary/parse.dll`（`.so`） | 读取消息时生成 `content_html` |

> `dytes.json` 的 `dependencies-sys` 只声明 `database` / `designdb` / `markparse`。

## 构建与发布

```bash
cargo build --release
```

产物：`target/release/chatllm.dll`（Linux `libchatllm.so`、macOS `libchatllm.dylib`）。
`.github/workflows/` 在 `main` 分支推送时三平台构建并上传到
`system/chatllm/<version>/<platform>/`。

## 文档

- `README.md`：能力、存储、依赖与构建入口；
- `todo/README.md`：目录结构、核心调用链与模块说明；
- `todo/TODO.md`：唯一的开发待办入口；
- `todo/optimization-tasks.md`：历史优化清单；
- `docs/facts.md`：1.0.6 代码事实清单；
- `docs/ffi.md`：FFI 接口说明；
- `docs/db.md`：`dialogue.db` 与 designdb 对话存储；
- `docs/analyze.md`：运行流程分析（历史分析，检索章节已过时）；
- `docs/contract.md`：接口契约 v1（候选冻结）。

外部总库 `dytes_sys_operations/sysinfo/sys/chatllm/` 保留架构与接口文档；
若与仓库内文档冲突，以仓库内文档为准并在开发完成后回填总库。

## 已知限制

- 错误码散落在各 FFI 文件，尚未集中定义；
- 输出缓冲区固定 64 KiB，超长结果会被截断（`write_json_out` 返回 `-2`）；
- 外部库路径写死为 `./libary/*`，未做配置化；
- 时间戳格式不统一：`dialogue` 用文本时间，`messages` 用 Unix 毫秒；
- `Cargo.lock` 未入库，构建不完全可复现。

处理清单见 `todo/TODO.md`。
