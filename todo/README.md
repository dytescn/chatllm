# chatllm 项目概览

## 这是什么

`chatllm` 是一个以 Rust 编写、编译为动态库（`cdylib`）的**对话与消息存储**模块。
它导出 C ABI 的 `post_*` 函数，由宿主（Piksel / cyctron）通过
`POST /chatllm` + `FFI-Symbol` 调用。运行时 cwd 为 Piksel 安装根目录。

当前版本：**1.0.6**。

> siliconflow 模型调用与 skills 注入已移除；`chat` 只负责新建 / 续聊并写入 user 消息。
> 向量与提示词检索也已移除，通用能力见 `kv_lib`。

## 目录结构

- `src/lib.rs`：crate 根，只导出 `ffi` 与 `utils`。
- `src/ffi/`：对外导出的 C 函数入口。
  - `chat.rs`：`post_chat`、`post_dialogue_list`、`post_dialogue_update`、`post_dialogue_search`、`post_chat_detail`。
  - `run.rs`：对话选择、system prompt 写入与 user 消息落库。
  - `types.rs`：请求体结构。
- `src/utils/`：内部工具模块。
  - `cjson.rs`：把结果写入 C 缓冲区（固定 64 KiB）。
  - `db.rs`：动态加载 `database.dll`，封装 `db_query` / `db_execute` / `db_update`。
  - `designdb.rs`：动态加载 `libdesigndb.dll`，按对话 UUID 读写消息库。
  - `dialogue.rs`：对话索引与消息、搜索、更新、schema 自愈。
  - `markparse.rs`：动态加载 `parse.dll`，读取消息时生成 `content_html`。
  - `threading.rs`：大栈线程工具。
- `setup.ts`：初始化 `dialogue.db`（表、索引、PRAGMA），并清理旧表。
- `ts/`：FFI 手工测试脚本与 `migrate_dialogues.ts` 迁移脚本。
- `tests/`：请求结构与编译基准。
- `.github/workflows/`：Windows / Linux / macOS 三平台 release 构建与上传。

## 核心调用链

### post_chat

请求字段见 `src/ffi/types.rs` 的 `ChatReq`：`prompt` 必填；`uuid` 续聊；
`plugin_uuid` / `dialogue_id` 指定新对话 id；`show_status`、`name`、`keyword`、
`system_prompt` 控制对话属性。

1. 解析并校验 `ChatReq`，校验 `uuid` 格式；
2. 选择目标对话（优先级：`uuid` 续聊 > `plugin_uuid` > `dialogue_id` > 自动生成）：
   - `plugin_uuid` 作为 `dialogue.id`，默认 `show_status = 2`；
   - 已存在的 `plugin_uuid` 对话直接复用；
3. 新建时写入标题 / keyword / system_prompt；
   续聊时按需更新 keyword 与显式 `system_prompt`；
4. 写用户消息到 `designdb/<dialogue_id>.messages`，并更新 `last_seq` / `message_count`；
5. 返回 `{uuid, created, prompt, content:"", content_html:"", session_id, system_prompt}`。

不调用模型、不写 assistant 消息；`session_id` 由宿主 / 其他服务维护。

## 存储

- 对话索引：`database/dialogue.db` → `dialogue`；
- 消息：`designdb/<dialogue_id>` → `messages`；
- 无 skills / 向量 / prompts 表。

## 关键外部依赖

- `db.rs` 加载 `./libary/database.dll`（Linux `.so`）；
- `designdb.rs` 加载 `./libary/libdesigndb.dll`；
- `markparse.rs` 加载 `./libary/parse.dll`。

## 构建与测试

```bash
cargo build --release
cargo test
```

## 待办

开发待办统一见 [TODO.md](TODO.md)；历史优化记录见 [optimization-tasks.md](optimization-tasks.md)。
