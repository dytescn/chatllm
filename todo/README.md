# chatllm 项目概览

## 这是什么

`chatllm` 是一个以 Rust 编写、编译为动态库（`cdylib`）的 AI 聊天系统模块。它不是独立服务，而是通过导出 C ABI 的 `post_*` 函数，被宿主程序（Piksel）通过 FFI 调用。

## 目录结构

- `src/lib.rs`：crate 根，只导出 `ffi` 和 `utils`。
- `src/ffi/`：对外导出的 C 函数入口。
  - `chat.rs`：`post_chat`、`post_dialogue_list`、`post_dialogue_update`、`post_dialogue_search`、`post_chat_detail`。
  - `plugin.rs`：`post_plugin_embed`、`post_plugin_search`。
  - `material.rs`：`post_material_embed`、`post_material_search`、`post_material_delete`。
  - `prompts.rs`：`post_embed_prompts`。
  - `search.rs`：`post_turbo_search_query`。
  - `run.rs`：一次完整 chat 的业务流程。
  - `types.rs`：请求体结构。
- `src/utils/`：内部工具模块。
  - `cjson.rs`：把结果写入 C 缓冲区。
  - `db.rs`：动态加载宿主的 `database.so` 并执行 SQL。
  - `designdb.rs`：动态加载 `libdesigndb.so`，读写每对话消息库。
  - `dialogue.rs`：对话列表、消息、搜索、更新。
  - `siliconflow.rs`：动态加载 `libsiliconflow_t2t.so` 并调用 `post_chat`。
  - `embedding.rs` / `embedding_store.rs` / `embed.rs`：向量生成与分片存储。
  - `plugin_lib.rs` / `skills.rs`：读取插件能力与技能，组装 system prompt。
  - `config.rs`、`generate.rs`、`loader.rs`、`prompt.rs`、`token.rs`、`ranking.rs`：旧的本地 llama 推理代码，目前未启用。
- `ts/`：FFI 手工测试脚本与 `migrate_dialogues.ts` 迁移脚本。
- `tests/`：嵌入向量与 turbovec 搜索集成测试（依赖本地模型与宿主服务）。
- `models/`：`qwen3-embedding-0.6.gguf` 嵌入模型。
- `.github/workflows/`：Windows / Linux / macOS 三平台 release 构建与上传。

## 核心调用链

`post_chat` 收到 JSON 后：

1. 解析并校验 `ChatReq`；
2. 调用 `run_chat`：
   - 无 `uuid` 新建对话，有 `uuid` 续聊；
   - 写用户消息到 `designdb/<dialogue_id>.messages`；
   - 读取历史消息与 `session_id`，组装 system prompt（可选 skills）；
   - 调用 siliconflow 生成回复；
   - 写助手消息并更新 `dialogue` 计数。
3. 通过 `cjson` 把结果写回 C 缓冲区。

## 存储

- 对话索引：`database/dialogue.db` → `dialogue`；
- 消息：`designdb/<dialogue_id>` → `messages`；
- 向量分片：`database/embedding_<category>_<n>.db`；
- 技能：读取走 `plugin_lib.get_skills`（`plugins.db.skills`）。

## 关键外部依赖

- `db.rs` 加载 `./libary/database.so`；
- `designdb.rs` 加载 `./libary/libdesigndb.so`；
- `siliconflow.rs` 加载 `./libary/libsiliconflow_t2t.so`；
- 路径目前写死为相对路径，运行时 cwd 是 Piksel 安装根目录。

## 待办

开发待办统一见 [TODO.md](TODO.md)。总库只保留架构与接口文档，开发完成后再回填。
