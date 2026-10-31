# chatllm 待办

> **2026-10-30**：siliconflow 模型调用与 skills 注入已移除，向量 / 提示词检索能力已移除。
> **2026-10-31**：`chat` 写入 user 消息后，用 `kv_lib` 检索 `chat_plugin`；无插件提示安装，
> 有插件时调用 Top1（`do_test_chat`）并把供应商回复写回对话。

> 本文件是 chatllm 仓库内的**唯一开发待办入口**。
> 架构、接口与数据库说明见 `README.md` / `todo/README.md`；
> 运行流程分析与实施路线见 `../docs/analyze.md`；本文件只记录可执行待办。
> 注意：本文件的 `P0–P4` 是优先级，不等同 `docs/analyze.md` 的优化批次编号。
> 开发完成后回填 `dytes_sys_operations/sysinfo/sys/chatllm/`。
>
> 当前版本：1.0.8（2026-10-31）
> **2026-10-30**：`embed_prompts` / `turbo_search_query` 与插件 / 素材向量能力已移除，
> 相关待办不再适用；通用向量能力由 `kv_lib` 提供。

## P0 数据一致性

- [ ] **CL-05 对话计数对账**
  - 位置：`src/utils/dialogue.rs` → `append_message`
  - 现状：先写 `designdb/<dialogue_id>.messages`，再 `UPDATE dialogue` 的 `message_count` / `last_seq`，无跨库事务。
  - 风险：第二步失败时计数偏小、`last_seq` 不前进；重试可能产生重复 `seq`。
  - 建议：给 `messages` 增加 `(dialogue_id, seq)` 唯一约束；或提供启动对账 / 重算脚本。
  - 验收：模拟 designdb 写成功、dialogue 更新失败后，重跑可修复且不重复。

- [ ] **旧数据迁移脚本演练**
  - 位置：`ts/migrate_dialogues.ts`
  - 现状：本机旧表已清理；脚本迁移旧 SQLite 分库 / `designdb/chatllm` 并清理，可重复执行。
  - 验收：在备份旧库上运行，对话数、消息数、首尾 `seq` 一致，旧库删除。

## P1 功能与契约

- [ ] **max_tokens 传入模型**
  - 位置：`src/ffi/chat.rs`、`src/ffi/run.rs`
  - 现状：`ChatReq.max_tokens` 只回显在响应里，未传给 siliconflow。
  - 验收：请求带 `max_tokens` 时，模型请求体包含该值。

- [ ] **对话级模型配置**
  - 位置：`src/ffi/run.rs` → `create_new_chat`
  - 现状：`create_dialogue_with` 的 `model_name` 传空字符串；请求中的 `system_prompt` 会写入，但 `model_config` 未从请求写入。
  - 验收：新建对话使用请求中的模型名与模型参数。

- [ ] **dialogue_search 检索消息内容**
  - 位置：`src/utils/dialogue.rs` → `search_dialogue`
  - 现状：只匹配 `title` / `keyword`。
  - 决策：确认是否需要消息全文检索；需要则接 designdb 或 FTS。
  - 验收：按消息内容能命中对话。

- [ ] **系统依赖声明**
  - 位置：`dytes.json` → `dependencies-sys`
  - 现状：只声明 `database` / `designdb` / `markparse`；代码还 dlopen `plugin_lib.dll` 与 `libsiliconflow_t2t.dll`，缺失时只在运行时调用报错。
  - 建议：补声明并确认发布顺序；加载失败给出明确错误。

## P2 架构与健壮性

- [ ] **外部库路径配置化**
  - `db.rs` / `designdb.rs` / `siliconflow.rs` / `plugin_lib.rs` / `markparse.rs` 把 `./libary/*` 与 `./models/*` 写死。
  - 建议：环境变量或配置注入；加载失败给出清晰错误。

- [ ] **64 KiB 输出缓冲**
  - `cjson::write_json_out` 与各 `call_ffi` 的缓冲均为 65,536 字节；大结果可能被截断（返回 `-2`）。
  - 建议：支持分块读取或扩容。

- [ ] **时间戳统一**
  - `dialogue` 用 `datetime('now')`（文本），`messages` 用 Unix 毫秒（`now_ms`）。

- [ ] **UUID 校验**
  - `valid_uuid` 只校验长度 36 且字符为十六进制或连字符，未校验分组格式与版本位。

## P3 测试、CI 与发布

- [ ] **CI 增加检查**
  - 目前只执行 `cargo build --release`；建议补 `cargo test`、`cargo fmt --check`、`cargo clippy`。

- [ ] **发布索引**
  - `.github/workflows` 只上传 `system/chatllm/<version>/<platform>/chatllm.dll`。
  - `system/chatllm/packages.json` 仍需 `dytes publish` 或单独发布流程更新。

- [ ] **Cargo.lock 是否入库**
  - 当前被 `.gitignore` 忽略，构建不完全可复现。

## P4 代码质量

- [ ] 错误码集中定义（当前 `400100`–`400500` 多个分段散落在各文件，`400503` 未使用）。
- [ ] SQL 统一参数化 / 查询构造，减少 `format!` 拼接。
- [ ] `eprintln!` 调试日志改为结构化日志并控制级别。

## 已完成（2026-10）

- [x] `session` 表合并到 `dialogue`，删除 `session` 表与 `sessions_*`（1.0.5）。
- [x] 删除 `skills` 表与 `skills_*`，技能读取统一 `plugin_lib.get_skills`（1.0.5）。
- [x] 清理旧 `messages` / `messages_fts` / `sessions` 表（1.0.6）。
- [x] 移除 `designdb/chatllm` 过渡单库初始化。
- [x] 消息改存 `designdb/<dialogue_id>`，`setup.ts` 负责建表、索引与旧表清理。
- [x] 插件对话复用：`plugin_uuid` / `dialogue_id` / `show_status`。
- [x] 移除 `embed_prompts` / `turbo_search_query` 与插件 / 素材向量、嵌入测试及模型依赖。
- [x] README 与 todo 文档按 1.0.6 代码校正。
