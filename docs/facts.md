# chatllm 事实清单（S2）

> **2026-10-30 更新**：siliconflow 模型调用与 skills 注入已移除；`chat` 仅写入 user 消息。
> 文中 skills / siliconflow / assistant 生成相关条目仅作历史记录。

> 快照：chatllm 1.0.6，2026-10-29。
> 每条记录“结论 + 证据”；证据写到文件与符号 / 表名，不依赖行号。
> 本清单是后续 S3 差异比对与各篇文档的唯一事实源。代码外约定列入 §J，需人工确认。
> 目标态设计与实施路线见 `analyze.md`；本清单只描述 1.0.6 现状。
> 接口契约见 `contract.md`（v1 候选冻结）。

## A. 版本与交付

| 主题 | 结论 | 证据 |
| --- | --- | --- |
| 版本 | `1.0.6`，`Cargo.toml` 与 `dytes.json` 一致 | `Cargo.toml` `[package].version`；`dytes.json` `version` |
| crate 类型 | `cdylib` + `rlib`，库名 `chatllm` | `Cargo.toml` `[lib]` |
| 发布产物 | Windows `chatllm.dll`、Linux `libchatllm.so`、macOS `libchatllm.dylib` | `Cargo.toml`、`.github/workflows/{win,linux,mac}.yml` |
| 发布方式 | `dytes.json` 导出 `./target/release/chatllm.dll` 与 `./setup.ts`，`init` 执行 `setup.ts` | `dytes.json` `exports` / `init` |
| CI 触发 | 推送到 `main`，三平台 `cargo build --release` 后上传 | `.github/workflows/*.yml` |
| 上传路径 | `system/chatllm/<version>/<platform>/` | `.github/workflows/*.yml` |
| 包索引 | CI 不更新 `packages.json` | 工作流仅上传产物 |
| 锁文件 | `Cargo.lock` 被 `.gitignore` 忽略 | `.gitignore` |

## B. FFI 接口

所有函数签名均为 `extern "C" fn(*const c_char, *mut c_char) -> c_int`，返回 `{code, data, msg}`，
成功 `code = 200`；输出缓冲 65,536 字节，超限返回 `-2`。

| FFI-Symbol | 导出函数 | 请求 | 成功 `data` | 证据 |
| --- | --- | --- | --- | --- |
| `chat` | `post_chat` | `ChatReq` | `uuid, created, prompt, content, content_html, max_tokens, keyword, session_id, system_prompt, skills, truncated` | `src/ffi/chat.rs` `post_chat`；`src/ffi/types.rs` `ChatReq` |
| `dialogue_list` | `post_dialogue_list` | `{page?, page_size?}`，可空 | `{list, page, page_size, total}` | `src/ffi/chat.rs` |
| `dialogue_update` | `post_dialogue_update` | `DialogueUpdateReq` | `{id}` | `src/ffi/chat.rs`；`types.rs` |
| `dialogue_search` | `post_dialogue_search` | `{keyword?, page?, page_size?}`，可空 | `{list, page, page_size, total}` | `src/ffi/chat.rs` |
| `chat_detail` | `post_chat_detail` | `{uuid, before_id?, after_id?, limit?}` | `{list, limit, first_id, last_id, has_more_before, has_more_after}` | `src/ffi/chat.rs` |

`ChatReq` 字段：`prompt`（必填）、`max_tokens?`、`uuid?`、`dialogue_id?`、`plugin_uuid?`、
`show_status?`、`name?`、`keyword?`、`use_skills?`、`skill_categories?`、`skill_ids?`、
`system_prompt?`、`refresh_system_prompt?`。

`DialogueUpdateReq` 字段：`id`（必填）、`title?`、`keyword?`、`model_name?`、`system_prompt?`、
`model_config?`、`is_pinned?`、`is_archived?`、`session_id?`、`show_status?`。

## C. 对话流程

| 主题 | 结论 | 证据 |
| --- | --- | --- |
| 目标选择 | 优先级 `uuid` 续聊 > `plugin_uuid` > `dialogue_id` > 自动生成 | `run.rs` `resolve_chat_target` |
| 插件对话 | `plugin_uuid` 作为 `dialogue.id`，默认 `show_status = 2`；已存在则复用，不重复创建 | `run.rs` `run_chat` |
| 指定 id | `dialogue_id` 默认 `show_status = 1`；`plugin_uuid` / `dialogue_id` 必须是 36 位十六进制 + 连字符，否则新建返回 `400103` | `run.rs`；`dialogue.rs` `valid_uuid` |
| 历史窗口 | 读取该对话**全部**消息，应用侧不按轮数或 token 截断 | `dialogue.rs` `message_history` |
| 标题 | `name` 非空优先；否则 `插件对话 <plugin_uuid>`；否则 prompt 前 20 字符；空则为 `新对话` | `run.rs` `new_title` |
| skills 开关 | `use_skills` 默认 false；`plugin_uuid` 且未显式给 `system_prompt` 时自动启用 | `run.rs` `should_use_skills` |
| system prompt | 新建时写入 `dialogue.system_prompt`；续聊时 `refresh_system_prompt` / 显式 `system_prompt` / 原值为空且启用 skills 才重建并更新 | `run.rs` |
| 消息顺序 | 先写 user 消息，再读全部历史（含本条 user），再调模型，最后写 assistant | `run.rs` |
| 计数 | 每条消息 `seq = last_seq + 1`，写后 `last_seq = seq`、`message_count + 1` | `dialogue.rs` `append_message` |
| 会话 id | 模型响应的 `id` 写入 `dialogue.session_id`，下次请求作为 `session_id` 传给 siliconflow | `run.rs`；`siliconflow.rs` |
| HTML | assistant 内容经 `parse.dll` 转 HTML；转译失败只记日志并返回空 `content_html`，不阻断对话 | `run.rs`；`markparse.rs` |
| max_tokens | 只在响应回显，未传给模型 | `chat.rs`；`run.rs` |
| 模型名 | 新建时 `model_name` 固定写空字符串；`model_config` 不从请求写入 | `run.rs` `create_new_chat` |

## D. 存储

### D1. `database/dialogue.db`

| 表 | 关键内容 | 证据 |
| --- | --- | --- |
| `dialogue` | 19 列（含主键 `id`）：`id, title, keyword, model_name, system_prompt, model_config, parent_id, skills_json, context_json, token_count, status, is_pinned, is_archived, session_id, show_status, message_count, last_seq, created_at, updated_at` | `setup.ts`；`dialogue.rs` `ensure_dialogue_schema` |

- `dialogue` 索引：`(is_archived, created_at DESC)`、`(is_pinned DESC, created_at DESC)`。
- `setup.ts` 对旧库执行 `ALTER TABLE dialogue ADD COLUMN` 补 `parent_id / skills_json / context_json / token_count / status`，已存在时忽略错误。
- `setup.ts` 合并旧 `session` 表到 `dialogue` 后删除；删除旧 `skills`、`messages_fts`、`messages`、`sessions`。
- PRAGMA：`foreign_keys=ON`、`journal_mode=WAL`、`cache_size=-64000`、`synchronous=NORMAL`。

### D2. 消息

| 主题 | 结论 | 证据 |
| --- | --- | --- |
| 位置 | `designdb/<dialogue_id>` 逻辑库，集合 `messages` | `dialogue.rs`；`designdb.rs` |
| 文档字段 | `_id`、`dialogue_id`、`seq`、`role`、`content`、`created_at`（Unix 毫秒） | `dialogue.rs` `append_message` |
| 读取 | 按 `seq` 升序分页扫描，页大小 50 | `dialogue.rs` `message_history`；`designdb.rs` `query_all` |
| 输出归一化 | `_id → uuid`、`seq → id`；附 `content_html` | `dialogue.rs` `normalize_message` / `list_messages` |
| 删除对话 | 先删消息库全部文档，再删 `dialogue` 行 | `dialogue.rs` `delete_dialogue` |

## F. 技能与提示词组装

| 主题 | 结论 | 证据 |
| --- | --- | --- |
| 技能来源 | `plugin_lib.get_skills`，请求 `{enabled:1, with_content:true, page:1, page_size:500}`，可带 `plugin_uuid/categories/ids` | `skills.rs` `build_skills_request` |
| 过滤 | 空 `content` 丢弃；有 `ids` 时按 id 过滤且忽略 `categories`；否则按 `categories` 过滤 | `parse_skills_response` |
| 排序 | 分类权重 `general 10 / design 20 / material 30 / plugin 40 / safety 90 / 其他 100`，再按 `sort`、`name` | `category_rank` |
| 上限 | 最多 20 个技能，system prompt 最多 6000 字符，超出逐个裁剪并置 `truncated` | `MAX_SKILLS` / `MAX_SYSTEM_CHARS` |
| 基础 prompt | 默认 `你是一个有用的助手。`；显式 `system_prompt` 优先 | `BASE_SYSTEM_PROMPT` / `build_system_prompt` |
| 模板 | 基础 prompt + 分类 + 技能 + 4 条使用规则 | `TEMPLATE` / `RULES` |

## G. 外部依赖与运行环境

| 依赖 | 逻辑路径 | 符号 | 证据 |
| --- | --- | --- | --- |
| database | `./libary/database.dll` / `.so` | `post_db_query`、`post_db_insert`、`post_db_update` | `db.rs` |
| designdb | `./libary/libdesigndb.dll` / `.so` | `post_db_query`、`post_db_insert`、`post_db_update`、`post_db_delete` | `designdb.rs` |
| siliconflow | `./libary/libsiliconflow_t2t.dll` / `.so` | `post_chat` | `siliconflow.rs` |
| plugin_lib | `./libary/plugin_lib.dll` / `.so` | `get_functions`、`get_skills`、`patch_function` | `plugin_lib.rs` |
| markparse | `./libary/parse.dll` / `.so` | `post_markdown_to_html` | `markparse.rs` |

- 所有路径相对 cwd（Piksel 安装根目录），在代码中写死。
- 库句柄与符号用 `OnceLock` 缓存；加载失败在首次调用时报错。
- `dytes.json` 的 `dependencies-sys` 只声明 `database 1.0.2`、`designdb 1.0.0`、`markparse 1.0.0`；
  `plugin_lib`、`siliconflow` 未声明。
- `dependencies-web`：`setting 1.0.0`、`chat_ui 1.0.6`。
- 无额外环境变量。

## H. 错误码

| 码 | 含义 | 出现位置 |
| --- | --- | --- |
| `400100` | `json_in` 为空 | 各 FFI `read_body` |
| `400101` | 请求非 UTF-8 | 各 FFI |
| `400102` | chat 请求 JSON 非法 | `ffi/chat.rs` |
| `400103` | uuid 非法 | `chat.rs`；`run.rs` |
| `400104` | `dialogue_update` JSON 非法 | `chat.rs` |
| `400105` | `dialogue_search` JSON 非法 | `chat.rs` |
| `400106` | `dialogue_list` JSON 非法 | `chat.rs` |
| `400107` | `chat_detail` JSON 非法 | `chat.rs` |
| `400109` | 组装 system prompt / 读取技能失败 | `run.rs` `ChatFlowError::Skills` |
| `400300` | 创建对话失败 | `run.rs` `CreateDialogue` |
| `400301` | 读取历史或写 user 消息失败 | `run.rs` `LoadHistory` / `AppendUser` |
| `400302` | 对话列表失败 | `chat.rs` |
| `400303` | 读取 / 更新对话、写 session_id 失败 | `chat.rs`；`run.rs` |
| `400304` | 对话搜索失败 | `chat.rs` |
| `400305` | 更新 keyword 失败 | `run.rs` |
| `400306` | 读取消息详情失败 | `chat.rs` |
| `400400` | siliconflow 调用失败 | `run.rs` |
| `400401` | 写 assistant 消息失败 | `run.rs` |
| `400500` | 参数 / JSON / 通用失败 | `ffi/chat.rs` |
| `400503` | 未使用 | — |

外部服务错误码（如 markparse `400201`）只出现在测试，不映射到 chatllm 输出。

## I. 测试与脚本

| 文件 | 用途 | 前提 |
| --- | --- | --- |
| `tests/compile_bench.rs` | 编译与产物统计 | 调用 `cargo`，依赖编译环境 |
| `ts/test_chat.ts` | `post_chat` 手工测试 | 宿主服务 |
| `ts/test_dialogue.ts` | 对话列表 / 更新 / 搜索 / 详情 | 宿主服务 |
| `ts/test_search.ts` | 对话搜索测试 | 宿主服务 |
| `ts/test_error.ts` | 错误码测试 | 宿主服务 |
| `ts/migrate_dialogues.ts` | 旧对话迁移与清理，`--keep` 保留旧库 | 宿主服务、两个库 |

- `compile_bench` 会调用 `cargo`；其余脚本需要 `127.0.0.1:44944`。
- 原嵌入 / turbovec 测试与 `embed_prompts` / `turbo_search_query` 脚本已移除。

- 测试均无 `#[ignore]`；CI 不执行 `cargo test`。

## J. 代码外约定（待确认）

| 主题 | 待确认内容 |
| --- | --- |
| 运行时目录 | 注释写 cwd 为 `/home/funx/app/piksel`；实际以 Piksel 安装根目录为准 |
| `packages.json` | 由 `dytes publish` 还是单独流程更新 |
| 迁移窗口 | `migrate_dialogues.ts` 对旧 `<uuid>.db` 与 `designdb/chatllm` 的保留策略 |
| siliconflow 响应 | 除 `data.content` / `data.id` 外是否还有需要透传的字段 |
| plugin_lib 版本 | `dytes.json` 未声明，安装顺序与兼容版本待确认 |
