# chatllm 待办

> 本文件是 chatllm 仓库内的**唯一开发待办入口**。
> 架构、接口与数据库说明在 `dytes_sys_operations/sysinfo/sys/chatllm/`；开发完成后回填总库。
>
> 当前版本：1.0.6（2026-10-29）

## P0 数据一致性

- [ ] **CL-05 对话计数对账**
  - 位置：`src/utils/dialogue.rs` → `append_message`
  - 现状：先写 `designdb/<dialogue_id>.messages`，再 `UPDATE dialogue` 的 `message_count` / `last_seq`，无跨库事务。
  - 风险：第二步失败时计数偏小、`last_seq` 不前进；重试可能产生重复 `seq`。
  - 建议：给 `messages` 增加 `(dialogue_id, seq)` 唯一约束；或提供启动对账 / 重算脚本。
  - 验收：模拟 designdb 写成功、dialogue 更新失败后，重跑可修复且不重复。

- [ ] **旧数据迁移脚本演练**
  - 位置：`ts/migrate_dialogues.ts`
  - 现状：本机旧表已清理；脚本已改为迁移旧 SQLite 分库 / `designdb/chatllm` 并清理。
  - 验收：在备份旧库上运行，对话数、消息数、首尾 `seq` 一致，旧库删除。

## P1 功能

- [ ] **max_tokens 传入模型**
  - 位置：`src/ffi/chat.rs`、`src/ffi/run.rs`
  - 现状：`ChatReq.max_tokens` 只回显在响应里，未传给 siliconflow。
  - 验收：请求带 `max_tokens` 时，模型请求体包含该值。

- [ ] **对话级模型配置**
  - 位置：`src/ffi/run.rs` → `create_new_chat`
  - 现状：`model_name` 传空字符串；`system_prompt` / `model_config` 未从请求写入。
  - 验收：新建对话使用请求中的模型名、提示词与模型参数。

- [ ] **dialogue_search 检索消息内容**
  - 位置：`src/utils/dialogue.rs` → `search_dialogue`
  - 现状：只匹配 `title` / `keyword`。
  - 决策：确认是否需要消息全文检索；需要则接 designdb 或 FTS。
  - 验收：按消息内容能命中对话。

- [ ] **旧推理代码处置**
  - 位置：`src/utils/config.rs`、`generate.rs`、`loader.rs`、`prompt.rs`、`token.rs`、`ranking.rs`
  - 现状：未挂载 / 未启用。
  - 决策：删除，或迁移为独立模块并补齐依赖。

## P2 架构与健壮性

- [ ] **外部库路径配置化**
  - `db.rs` / `siliconflow.rs` 把 `./libary/*.so` 写死。
  - 建议：环境变量或配置注入；加载失败给出清晰错误。

- [ ] **64 KiB 输出缓冲**
  - 向量或大结果可能被截断；支持分块读取或扩容。

- [ ] **时间戳统一**
  - `dialogue` 用 `CURRENT_TIMESTAMP`（文本），`messages` 用 Unix 秒。

- [ ] **UUID 校验**
  - `valid_uuid` 只是宽松的 36 位十六进制 + 连字符校验。

## P3 测试、CI 与发布

- [ ] **集成测试标记 / 文档**
  - `tests/embed_prompts.rs`、`tests/turbovec_bench.rs` 依赖宿主服务与模型。
  - 建议：`#[ignore]` + 运行前提说明。

- [ ] **CI 增加检查**
  - 目前只执行 `cargo build --release`；建议补 `cargo test`、`cargo fmt --check`、`cargo clippy`。

- [ ] **发布索引**
  - `.github/workflows` 只上传 `system/chatllm/<version>/<platform>/chatllm.dll`。
  - `system/chatllm/packages.json` 仍需 `dytes publish` 或单独发布流程更新。

- [ ] **Cargo.lock 是否入库**
  - 当前被 `.gitignore` 忽略，构建不完全可复现。

## P4 代码质量

- [ ] 错误码集中定义（当前 `400100`–`400401` 散落在多个文件）。
- [ ] SQL 统一参数化 / 查询构造，减少 `format!` 拼接。
- [ ] `eprintln!` 调试日志改为结构化日志并控制级别。

## 已完成（2026-10）

- [x] `session` 表合并到 `dialogue`，删除 `session` 表与 `sessions_*`（1.0.5）。
- [x] 删除 `skills` 表与 `skills_*`，技能读取统一 `plugin_lib.get_skills`（1.0.5）。
- [x] 清理旧 `messages` / `messages_fts` / `sessions` 表（1.0.6）。
- [x] 移除 `designdb/chatllm` 过渡单库初始化。
- [x] README 补充能力、存储、构建与文档入口。
