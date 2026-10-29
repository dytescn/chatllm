# chatllm 优化任务清单

| 项 | 内容 |
| --- | --- |
| 状态 | 历史清单（部分已完成）；当前开发待办见 [TODO.md](TODO.md) |
| 来源 | 2026-10-24 `plugin_embeddings` 设计评审中识别的结构问题 |
| 关联 | [plugin-embeddings.md](plugin-embeddings.md)（F-09 向量与检索需求） |
| 目标 | 先补齐 F-09 的前置能力，再处理既有结构问题，最后统一发布 |

---

## 0. 执行顺序

```text
第一批（可并行）
  CL-03  crate-type 加 rlib（测试可链接）
  CL-01  补 db_insert / db_update / db_delete 封装
  CL-06  错误码表与常量（先文档，后代码）

第二批（F-09 前置）
  CL-02  拆分 ChatConfig / EmbedConfig，启用 loader
  F-09   实现 plugin_embeddings + post_plugin_embed / post_plugin_search

第三批（结构收口）
  CL-04  sessions 去掉任意 SQL
  CL-05  统一 dialogue.db 与 uuid 分库

第四批（发布）
  CL-07  版本、dytes.json、setup.ts、发布验证
```

依赖关系：

```text
CL-03 ─┬─> CL-01 ─┬─> CL-04
       │          └─> F-09（写 plugin_embeddings）
       └─> CL-02 ────> F-09（生成向量）
CL-06 ─────────────> CL-07（错误码稳定后再发布）
CL-05 ─────────────> CL-07（避免发布后再迁移数据）
```

---

## 1. 任务总表

| ID | 任务 | 优先级 | 依赖 | 预计改动 | 验收 |
| --- | --- | --- | --- | --- | --- |
| CL-01 | 补数据库写封装 `db_insert` / `db_update` / `db_delete` | P0 | 无 | `src/utils/db.rs` | 临时库 CRUD 通过；错误码透传 |
| CL-02 | 拆分 `ChatConfig` / `EmbedConfig`，启用 `loader` | P0 | 无 | `src/utils/config.rs`、`mod.rs`、`loader.rs` | `cargo check`；embedding 模型加载（ignored 测试） |
| CL-03 | `crate-type` 加 `rlib` | P0 | 无 | `Cargo.toml` | `cargo test` 可编译；`cargo build --release` 仍产 DLL |
| CL-04 | `sessions.rs` 去掉任意 SQL，改结构化接口 | P1 | CL-01 | 已随 session 表与 `sessions_*` 移除（1.0.5） | 完成 |
| CL-05 | 统一 `dialogue.db` 与 `database/<uuid>.db` | P1 | 无（需产品确认） | `src/utils/dialogue.rs`、`setup.ts` | 新会话不再新增 uuid.db；历史数据可读 |
| CL-06 | 错误码表与常量统一 | P1 | 无 | `src/ffi/*.rs`、新增 `src/utils/error.rs` | 文档与代码一致；无散落字面量 |
| CL-07 | 版本、`dytes.json`、`setup.ts`、发布验证 | P2 | F-09、CL-05、CL-06 | `Cargo.toml`、`dytes.json`、`setup.ts` | 发布并安装成功；新符号可调用 |
| F-09 | `plugin_embeddings` + embed/search 接口 | P0 | CL-01、CL-02、CL-03 | 见 `plugin-embeddings.md` | 向量化、检索、状态回写通过 |

> 状态列统一使用：`待执行` / `进行中` / `完成` / `阻塞`。开工时在任务详情里补日期与提交号。

---

## 2. 任务详情

### CL-01 补数据库写封装（P0）

**背景**

`src/utils/db.rs` 目前只封装了 `post_db_query`，写操作没有统一入口。`setup.ts` 走 HTTP 的 `db_update`，
而 Rust 侧要写 `dialogue.plugin_embeddings`、回写 `plugin_functions` 时必须自己重复 libloading 逻辑。

**改动**

1. 在 `src/utils/db.rs` 增加可复用的符号加载：

   ```rust
   pub enum DbOp { Query, Insert, Update, Delete }

   pub fn db_exec(db_name: &str, sql: &str, op: DbOp) -> Result<Vec<Value>>;
   pub fn db_insert(db_name: &str, sql: &str) -> Result<Vec<Value>>;
   pub fn db_update(db_name: &str, sql: &str) -> Result<Vec<Value>>;
   pub fn db_delete(db_name: &str, sql: &str) -> Result<Vec<Value>>;
   ```

2. 符号映射：`Query → post_db_query`、`Insert → post_db_insert`、`Update → post_db_update`、
   `Delete → post_db_delete`；沿用 `OnceLock<Library>` 缓存，避免重复 dlopen。
3. 统一请求 `{path, sql}`、输出缓冲 65,536 字节、响应 `{code,data,msg}` 解析。
4. 保留现有 `db_query` 作为 `db_exec(.., DbOp::Query)` 的薄封装，避免改调用方。

**验收**

```bash
cd /d/codes/github/dytescn/chatllm
cargo test db_exec
```

- 临时库：建表 → 插入 → 更新 → 删除 → 查询，每步 `code=200`；
- 错误 SQL 返回明确 `code` / `msg`，不 panic；
- 产物仍可通过 cyctron 调用。

**风险**

- `database.dll` 的 insert/update 返回结构可能与 query 不同（`data` 为受影响行数或空），需实测后统一解析。

---

### CL-02 拆分配置并启用 loader（P0）

**背景**

`src/utils/config.rs` 的 `LlmConfig` 同时承担生成参数与模型路径，但 `model_path` 指向
`qwen3-embedding-0.6.gguf`；`utils/mod.rs` 又把 `config` / `generate` / `loader` / `token` 全部注释，
embedding 推理没有接入点。

**改动**

1. 拆分：

   ```rust
   pub struct ChatConfig { /* 现有采样参数 */ }
   pub struct EmbedConfig {
       pub model_path: &'static str, // ./models/qwen3-embed/qwen3-embedding-0.6.gguf
       pub dim: usize,               // 1024
       pub ctx_size: u32,            // >= 512
       pub cpu_threads: i32,
   }
   ```

2. 在 `utils/mod.rs` 启用 `config`、`loader`（`generate` / `token` 是否启用按需）。
3. `EmbedConfig::default()` 给出与 GGUF 元数据一致的 `dim = 1024`。
4. 为 `Embedder::load()`（见 F-09）提供 `LlamaBackend` / `LlamaModel` 加载路径，沿用 `loader.rs`。

**验收**

```bash
cargo check
cargo test -- --ignored embed_model_load
```

- 模型文件存在时加载成功；缺失时返回明确错误；
- 不影响现有 `post_chat`（siliconflow 路径）。

---

### CL-03 crate-type 加 rlib（P0）

**背景**

`Cargo.toml` 只有 `crate-type = ["cdylib"]`，`normalize` / `cosine` / blob 转换等纯函数无法被
`cargo test` 链接，F-09 的单测没有落点。

**改动**

```toml
[lib]
crate-type = ["cdylib", "rlib"]
name = "chatllm"
```

**验收**

```bash
cargo test
cargo build --release
```

- 测试目标可编译并运行；
- `target/release/chatllm.dll` 照常生成，`dytes.json` 的 `exports` 不变。

---

### CL-04 sessions 去掉任意 SQL（P1）

**背景**

`src/ffi/sessions.rs` 的 `post_session_list/create/update/delete` 都接收 `{sql}` 并直接执行，
既有注入面，也与「UI/chatllm 不拼 SQL」的目标冲突。

**改动**

1. 先确认调用方：`grep -rn "post_session\|session_list\|session_create"` 于 `chat_ui` / `chatllm/ts`；
2. 改为结构化请求，例如：

   ```json
   { "op": "list", "page": 1, "page_size": 20 }
   { "op": "create", "title": "..." }
   { "op": "update", "id": "...", "title": "..." }
   { "op": "delete", "id": "..." }
   ```

3. 内部构造固定 SQL；若 `database.dll` 不支持参数绑定，至少做字段白名单 + 值转义；
4. 删除 `parse_sql`，不再接受客户端 SQL。

**验收**

- 传 `{"sql":"DROP TABLE session"}` 返回 `400500`；
- 列表 / 创建 / 更新 / 删除与现有调用方兼容；
- `cargo test` 覆盖正常与恶意输入。

**风险**

- 若现有调用方依赖任意 SQL，需同步改调用方；先 grep 再动手。

---

### CL-05 统一数据库（P1，需产品确认）

**背景**

两套存储并存：

- `chatllm/setup.ts` 在 `dialogue.db` 建 `prompts` / `dialogue` / `messages` / `sessions`；
- `src/utils/dialogue.rs` 用 `db_query(uuid, sql)` 为每个会话建独立库，`database/` 下有大量 `<uuid>.db`。

**改动（二选一，确认后执行）**

- **方案 A（推荐）**：统一到 `dialogue.db`，`messages` 按 `session_uuid` 关联；提供一次性迁移脚本，
  把 `<uuid>.db` 的 `messages` 合并进主库；保留旧文件只读一个版本周期。
- **方案 B**：明确保留分库，补文档，`setup.ts` 不再建重复的 `messages` 表。

**验收**

- 新建会话后 `database/` 不再新增 `<uuid>.db`；
- 历史会话与消息可正常读取；
- `chat_detail` / `dialogue_list` 行为不变。

**风险**

- 迁移涉及用户数据，需要备份与回滚脚本；发布前必须确认。

---

### CL-06 错误码统一（P1）

**背景**

当前错误码分散：`400100/400101/400102`（chat）、`400300–400305`（dialogue）、`400400/400401`
（siliconflow），F-09 又需要 `4005xx`。部分文件内联字面量，缺少统一表。

**建议分段**

| 段 | 用途 |
| --- | --- |
| `4000xx` | 通用参数 / JSON / UTF-8 |
| `4001xx` | chat / 生成 |
| `4003xx` | dialogue / session / 历史消息 |
| `4004xx` | 外部服务（siliconflow、markparse） |
| `4005xx` | plugin / embed / search |
| `5000xx` | 数据库 / 文件 / 加载失败 |

**改动**

1. 新增 `src/utils/error.rs`：常量 + `msg(code)`；
2. 各 FFI 改用常量，移除散落字面量；
3. 在 `plugin-embeddings.md` 的错误码表与实现保持一致。

**验收**

- `grep -rn "write_error(.*, 4[0-9][0-9][0-9]" src` 不再出现裸数字（或仅保留常量定义）；
- 现有调用方收到的 code 不变（除新增段）。

---

### CL-07 版本与发布（P2）

**背景**

`Cargo.toml` 与 `dytes.json` 均为 `1.0.1`；新增 `post_plugin_embed` / `post_plugin_search`
后需要升版并验证安装链路。

**改动**

1. `Cargo.toml` / `dytes.json` 升到 `1.0.2`（两处一致）；
2. `setup.ts` 增加 `plugin_embeddings` 建表与索引（DDL 见 `plugin-embeddings.md` §2.1）；
3. 确认 `dytes.json` 的 `exports` 指向 `./target/release/chatllm.dll`；
4. 发布并在 `app/piksel` 安装验证。

**验收**

```bash
cargo build --release
./publish.exe
cd /d/codes/app/piksel && ./update.exe --auto
curl -X POST http://127.0.0.1:44944/chatllm \
  -H "Content-Type: application/libary" -H "FFI-Symbol: plugin_search" \
  -d '{"query":"导出文档"}'
```

- `dytes.lock` 中 chatllm 为 1.0.2；
- `dialogue.db` 存在 `plugin_embeddings`；
- `post_plugin_embed` / `post_plugin_search` 可调用。

---

## 3. 与 F-09 的关系

F-09（`plugin-embeddings.md`）是功能实现，本清单是它的前置与配套治理：

| F-09 依赖 | 对应任务 |
| --- | --- |
| 写 `dialogue.plugin_embeddings` | CL-01 |
| 生成 1024 维向量 | CL-02 |
| 纯函数单测（normalize/cosine/blob） | CL-03 |
| 错误码 `4005xx` | CL-06 |
| 建表与发布 | CL-07 |

建议顺序：**CL-03 → CL-01 → CL-02 → F-09 → CL-04 → CL-05 → CL-06 → CL-07**；
其中 CL-05（数据迁移）风险最高，必须在 CL-07 之前完成或明确延期。

---

## 4. 完成标准

- [ ] 所有 P0 任务完成，`cargo test` / `cargo check` 通过；
- [ ] F-09 的 `post_plugin_embed` / `post_plugin_search` 可被 cyctron 调用；
- [ ] `plugin_embeddings` 有向量行，`plugin_functions.embed_status` 正确回写；
- [ ] `sessions` 不再接受任意 SQL；
- [ ] 不再新增 `<uuid>.db`（或已明确保留并文档化）；
- [ ] 错误码文档与实现一致；
- [ ] `chatllm 1.0.2` 发布并安装验证通过。

---

## 5. 待确认

1. `database.dll` 的 `post_db_insert` / `post_db_update` / `post_db_delete` 返回结构；
2. `post_session_*` 的现有调用方是否依赖任意 SQL；
3. `<uuid>.db` 分库是否仍有业务依赖（决定 CL-05 方案）；
4. F-09 的向量检索是否需要在 `post_chat` 内自动触发，还是仅提供独立 FFI；
5. 发布窗口与 `plugin_lib 1.0.0` 的联调顺序。
