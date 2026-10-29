# 待完善事项

## 1. 模块整理与清理

- [ ] 处理 `src/utils` 下未启用的旧推理代码：`config.rs`、`generate.rs`、`loader.rs`、`prompt.rs`、`token.rs`、`ranking.rs`。其中 `generate.rs` / `loader.rs` / `token.rs` 依赖 `encoding_rs` 和 `llama_cpp_2`，而这两者目前只在 `dev-dependencies` 中，直接启用会编译失败。需要决定是删除还是迁移到独立模块。
- [ ] 清理 `ts/` 下重复或废弃脚本：`init_design_plugins.ts` 中 `prompts` 建表 SQL 混入了错误的 `CREATE INDEX` 和多余的 `);`，且与 `reinit_prompts.ts` 功能重复。

## 2. 功能缺陷

- [ ] `post_chat` 接收了 `max_tokens`，但没有传给 `run_chat` 或 `chat_single`，参数目前被忽略。
- [ ] 续聊没有携带历史上下文：`run_chat` 只把当前 `prompt` 发给 siliconflow，没有读取历史消息，多轮对话效果受限。
- [ ] `create_dialogue` 把 `model_name`、`system_prompt`、`model_config` 写死为空字符串；`DialogueUpdateReq` 虽支持这些字段，但业务侧没有真正使用。
- [ ] `dialogue_search` 只匹配 `keyword` 和 `title`，不检索消息内容；需确认这是有意设计还是遗漏。

## 3. 架构与健壮性

- [ ] 每个对话单独创建一个以 UUID 命名的数据库来存放 `messages` 表，对话数量大时会产生大量数据库文件和连接。建议评估合并为统一的 `messages` 表（带 `dialogue_id` 索引）的方案。
- [ ] `db.rs` 和 `siliconflow.rs` 把外部 `.so` 路径写死为相对路径，导致本 crate 无法独立运行和测试。建议改成从环境变量或配置注入，并在加载失败时给出清晰的错误提示。
- [ ] 输出缓冲区固定为 64KB，向量或大结果可能被截断（`tests` 中也提到了这一限制）。建议支持分块读取或扩大缓冲。
- [ ] 时间戳格式不统一：`dialogue` 表用 `CURRENT_TIMESTAMP`（文本），`messages` 表用 Unix 秒（整数）。
- [ ] `valid_uuid` 只是宽松的 36 位十六进制加连字符校验，并不是严格的 UUID v4 校验。

## 4. 测试与 CI

- [ ] `tests/embed_prompts.rs` 和 `tests/turbovec_bench.rs` 依赖本地 `127.0.0.1:44944` 的宿主服务和 `qwen3-embedding-0.6.gguf` 模型，普通 `cargo test` 环境无法直接运行。建议标记为 `#[ignore]` 或提供文档说明运行前提。
- [ ] CI 目前只在 push 到 `main` 时执行 `cargo build --release` 并上传产物，没有跑 `cargo test`、`cargo fmt`、`cargo clippy`。
- [ ] `README.md` 只有一句话，缺少项目说明、FFI 符号与 JSON 协议、错误码表和本地运行方法。

## 5. 代码质量

- [ ] 错误码（`400100` 到 `400401`）散落在多个文件里，建议集中定义，避免重复和冲突。
- [ ] SQL 大量使用 `format!` 拼接，虽然已有 `escape_sql`，但建议统一改为参数绑定或查询构造器。
- [ ] `eprintln!` 调试日志（如 `[db] ...`、`[siliconflow] ...`）建议替换为结构化日志，并控制输出级别。
