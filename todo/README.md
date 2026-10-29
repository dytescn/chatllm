# chatllm 项目概览

## 这是什么

`chatllm` 是一个以 Rust 编写、编译为动态库（`cdylib`）的 AI 聊天系统模块。它不是独立服务，而是通过导出 C ABI 的 `post_*` 函数，被宿主程序（piksel）通过 FFI 调用。

## 目录结构

- `src/lib.rs`：crate 根，只导出 `ffi` 和 `utils` 两个模块。
- `src/ffi/`：对外导出的 C 函数入口。
  - `chat.rs`：`post_chat`、`post_dialogue_list`、`post_dialogue_update`、`post_dialogue_search`、`post_chat_detail`。
  - `run.rs`：一次完整 chat 的业务流程。
  - `types.rs`：请求体的结构定义。
- `src/utils/`：内部工具模块。
  - `cjson.rs`：把结果写入 C 缓冲区。
  - `db.rs`：动态加载宿主的 `database.so` 并执行 SQL。
  - `dialogue.rs`：对话列表、消息、搜索、更新的业务实现。
  - `siliconflow.rs`：动态加载宿主的 `libsiliconflow_t2t.so` 并调用 `post_chat`。
  - `config.rs`、`generate.rs`、`loader.rs`、`prompt.rs`、`token.rs`、`ranking.rs`：旧的本地 llama 推理相关代码，目前未启用。
- `ts/`：针对 FFI 接口的手工测试脚本（需要宿主 HTTP 服务）。
- `tests/`：嵌入向量和 turbovec 搜索的集成测试（依赖本地模型与宿主服务）。
- `models/`：`qwen3-embedding-0.6.gguf` 嵌入模型。
- `data/`：空目录。
- `.github/workflows/`：Windows / Linux / macOS 三平台的 release 构建与上传。

## 核心调用链

`post_chat` 收到 JSON 后：

1. 解析并校验 `ChatReq`。
2. 调用 `run_chat`：
   - 无 `uuid` 则新建对话，有 `uuid` 则续聊并更新 `keyword`。
   - 把用户消息写入该对话的 `messages` 表。
   - 调用 `chat_single` 让 siliconflow 生成回复。
   - 把助手回复写入 `messages` 表。
3. 通过 `cjson` 把结果写回 C 缓冲区。

## 关键外部依赖

- `db.rs` 会加载 `./libary/database.so` 中的 `post_db_query`。
- `siliconflow.rs` 会加载 `./libary/libsiliconflow_t2t.so` 中的 `post_chat`。
- 两个路径都写死为相对路径，代码注释标明运行时 cwd 是 `/home/funx/app/piksel`。
- `lib.rs` 只依赖 `libc`、`serde`、`serde_json`、`anyhow`、`uuid`、`turbovec`；llama 相关依赖只在 `dev-dependencies` 里。

## 当前仓库状态

`cargo check` 可以通过。工作区存在未提交改动：`Cargo.toml` 有修改，`src/utils/ranking.rs`、`tests/`、`ts/init_design_plugins.ts`、`ts/query_design_plugins.ts`、`ts/reinit_prompts.ts` 是未跟踪文件。
