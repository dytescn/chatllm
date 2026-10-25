# chatllm 插件能力向量与检索需求（plugin_embeddings）

| 项 | 内容 |
| --- | --- |
| 状态 | 已实现（chatllm 1.0.3：post_plugin_embed / post_plugin_search） |
| 归属 | **chatllm 自有**：向量维度、向量表、向量化与语义检索 |
| 数据库 | `dialogue.db`（chatllm 的库） |
| 模型 | `./models/qwen3-embed/qwen3-embedding-0.6.gguf` |
| 维度 | **1024**（GGUF 元数据 `qwen3.embedding_length`） |
| 关联 | `dytes_sys_operations/design_plugins/database.md`、`dytes_sys_operations/feature/007-plugin-ui.md` |

> 边界：`plugin_lib` 负责插件元数据、能力清单（`get_functions`）与向量化状态回写（`patch_function`）；
> **向量表、维度、模型、相似度检索全部由 chatllm 实现**。chatllm 不直接拼 `plugins.db` 的 SQL。

---

## 1. 目标与范围

### 1.1 目标

1. 把 `plugin_lib` 登记的能力（`plugin_functions`）转成向量，写入 `dialogue.plugin_embeddings`；
2. 按自然语言检索能力，返回可直接调用的 `lib_name` / `ffi_symbol` / 参数 schema；
3. 只召回本地可用且 `use_ai=1` 的函数（`is_local=1`），避免推荐未安装插件或已禁用 AI 的能力；
4. 向量化状态可增量维护：新增 / 描述变化 / 模型变化时才重新生成。

### 1.2 非目标

- 不管理插件安装、版本、配置（归 `plugin_lib`）；
- 不实现向量数据库服务（本地暴力余弦即可，函数数量级为百到千）；
- 不改动现有对话链路（`post_chat`），检索作为独立 FFI 供后续工具调用接入。

### 1.3 数据流

```text
plugin_lib.get_functions(local=1, embed_status=0/2)
        │  {uuid, lib_name, symbol, ffi_symbol, title, description,
        │   params_schema, return_schema, side_effects, ...}
        ▼
chatllm post_plugin_embed
        │  组装文本 → sha256 → qwen3-embed(1024) → 归一化
        ├──▶ dialogue.plugin_embeddings（UPSERT）
        └──▶ plugin_lib.patch_function({uuid, embed_status=1, ...})

用户查询
        ▼
chatllm post_plugin_search
        │  query → 1024 维向量 → 余弦 → top_k
        ├──▶ dialogue.plugin_embeddings（读向量）
        └──▶ plugin_lib.get_functions({uuids})（取详情 / 校验 is_local）
        ▼
{lib_name, ffi_symbol, params_schema, score}
```

---

## 2. 数据表

### 2.1 DDL（dialogue.db）

```sql
CREATE TABLE IF NOT EXISTS plugin_embeddings (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  function_uuid TEXT NOT NULL,              -- plugin_functions.uuid（跨库逻辑关联）
  lib_name      TEXT NOT NULL,              -- 冗余：检索结果可直接拼 POST /<lib_name>
  symbol        TEXT NOT NULL,              -- post_xxx
  use_ai        INTEGER DEFAULT 1,          -- 是否允许被 AI 检索/调用：1=允许，0=仅人工
  model         TEXT NOT NULL,              -- qwen3-embed
  dim           INTEGER NOT NULL,           -- 1024
  vector        BLOB NOT NULL,              -- float32 小端，长度 = dim * 4 = 4096
  text_hash     TEXT NOT NULL,              -- 向量文本 sha256，与 plugin_functions.embed_text_hash 对应
  created_at    INTEGER,
  updated_at    INTEGER,
  UNIQUE(function_uuid, model)
);

CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_model ON plugin_embeddings(model);
CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_fn ON plugin_embeddings(function_uuid);
CREATE INDEX IF NOT EXISTS idx_plugin_embeddings_ai ON plugin_embeddings(model, use_ai);
```

### 2.2 字段说明

| 字段 | 说明 |
| --- | --- |
| `function_uuid` | 对应 `plugins.db.plugin_functions.uuid`；删除函数后由清理任务删除 |
| `lib_name` / `symbol` | 冗余，检索结果无需回查即可展示；调用时仍以 `get_functions` 的详情为准 |
| `model` | 模型标识；换模型时保留旧行，检索按当前模型过滤 |
| `dim` | 固定 1024；写入前校验向量长度 |
| `vector` | 归一化后的 `f32` 小端 BLOB；点积即余弦 |
| `text_hash` | 组装文本的 sha256；与 `embed_text_hash` 不一致即需重建 |
| `use_ai` | 是否允许被 AI 检索/调用；`1` 允许（默认），`0` 仅人工使用 |

### 2.3 初始化

- 建表放在 `chatllm/setup.ts`（与 `prompts` / `dialogue` 表一起，用 `db_update`）；
- 也可在 `post_plugin_embed` 首次调用时 `CREATE TABLE IF NOT EXISTS` 兜底；
- 两种方式都必须幂等。

### 2.4 清理

- `post_plugin_cleanup`（可选）：删除 `function_uuid` 在 `plugin_lib.get_functions` 中已不存在的行；
- 插件删除时由 `plugin_lib` 调用 chatllm 清理，或依赖定期清理；
- 不级联删除 `plugin_functions`（跨库无外键）。

---

## 3. 接口

### 3.1 post_plugin_embed

```text
POST http://127.0.0.1:44944/chatllm
Content-Type: application/libary
FFI-Symbol: plugin_embed

{ "limit": 100, "force": false }
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `limit` | int? | 单次处理上限，默认 100 |
| `force` | bool? | `true` 时忽略 `embed_status`，按 `text_hash` 重建 |

响应：

```json
{
  "code": 200,
  "msg": "success",
  "data": {
    "processed": 12,
    "skipped": 3,
    "failed": 0,
    "model": "qwen3-embed",
    "dim": 1024
  }
}
```

流程：

1. 调 `plugin_lib.get_functions({local: 1, embed_status: [0, 2], limit})`；
2. 组装文本：
   ```text
   {title}
   {description}
   目标: {target}
   参数: {name}: {description}; ...
   返回: {return description}
   ```
3. `sha256(text)`；与已有 `text_hash` 相同且非 `force` → `skipped`；
4. `qwen3-embed` 生成 1024 维向量 → L2 归一化；
5. `INSERT INTO plugin_embeddings (..., use_ai) VALUES (..., 1) ON CONFLICT(function_uuid, model) DO UPDATE ...`
   （已有行保留原 `use_ai`，不覆盖）；
6. 调 `plugin_lib.patch_function({uuid, embed_status: 1, embed_model, embed_text_hash, embedded_at})`；
7. 单条失败不影响后续，最终返回统计。

### 3.2 post_plugin_search

```text
POST http://127.0.0.1:44944/chatllm
Content-Type: application/libary
FFI-Symbol: plugin_search

{ "query": "把当前文档导出为 PDF", "top_k": 5, "model": "qwen3-embed", "min_score": 0.2 }
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `query` | string | 必填，自然语言 |
| `top_k` | int? | 默认 5，最大 50 |
| `model` | string? | 默认 `qwen3-embed` |
| `min_score` | float? | 默认 0，低于该分数丢弃 |

响应：

```json
{
  "code": 200,
  "msg": "success",
  "data": {
    "model": "qwen3-embed",
    "dim": 1024,
    "list": [
      {
        "uuid": "fn-cdrimex-export-file",
        "lib_name": "cdrimex",
        "symbol": "post_export_file",
        "ffi_symbol": "export_file",
        "title": "导出当前文档",
        "description": "导出当前活动文档到指定路径……",
        "params_schema": { "type": "object", "properties": { } },
        "return_schema": { "type": "object" },
        "side_effects": 1,
        "score": 0.83
      }
    ]
  }
}
```

流程：

1. 生成 query 向量（同一模型、同一维度）；
2. `SELECT function_uuid, lib_name, symbol, vector FROM plugin_embeddings WHERE model = ? AND use_ai = 1`；
3. 逐行反序列化 `f32`，计算点积（向量已归一化）；
4. 过滤 `min_score`，排序取 `top_k`；
5. 调 `plugin_lib.get_functions({uuids})` 取详情，过滤 `is_local != 1`；
6. 返回；`params_schema` / `return_schema` 解析为 JSON 对象。

### 3.3 错误码

| 码 | 含义 |
| --- | --- |
| `400500` | 参数 / JSON / UTF-8 错误 |
| `400501` | `plugin_lib` 不可用或调用失败 |
| `400502` | 向量生成失败（模型未加载 / 推理失败） |
| `400503` | 向量维度不匹配或数据损坏 |
| `400504` | `dialogue.db` 读写失败 |

沿用现有约定：`json_out` 返回 `{code, data, msg}`；成功 `code=200`。

---

## 4. 与 plugin_lib 的契约

### 4.1 调用方式

沿用 `src/utils/db.rs` / `markparse.rs` 的 `libloading` + `OnceLock` 方式：

```rust
#[cfg(windows)]
const PLUGIN_LIB_PATH: &str = "./libary/plugin_lib.dll";
#[cfg(not(windows))]
const PLUGIN_LIB_PATH: &str = "./libary/plugin_lib.so";

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;
// post_get_functions / post_patch_function
```

备选：通过 cyctron `POST /plugin_lib` 调用。优先 `dlopen`，与现有 `database.dll` / `parse.dll` 调用保持一致，避免引入 HTTP 依赖。

### 4.2 post_get_functions

请求：

```json
{ "local": 1, "embed_status": [0, 2], "limit": 100 }
```

或按 uuid 批量：

```json
{ "uuids": ["fn-cdrimex-export-file"], "local": 1 }
```

响应 `data.list[]` 至少包含：

```json
{
  "uuid": "fn-cdrimex-export-file",
  "lib_uuid": "lib-cdrimex-win",
  "lib_name": "cdrimex",
  "symbol": "post_export_file",
  "ffi_symbol": "export_file",
  "title": "导出当前文档",
  "description": "……",
  "category": "file",
  "params_schema": "{...}",
  "return_schema": "{...}",
  "side_effects": 1,
  "is_local": 1,
  "embed_status": 0,
  "embed_text_hash": null,
  "embed_model": null
}
```

> `target` 用于组装向量文本；若 `get_functions` 未返回，可从 `plugin_libs.target` 合并或省略该行。

### 4.3 post_patch_function

请求：

```json
{
  "uuid": "fn-cdrimex-export-file",
  "embed_status": 1,
  "embed_model": "qwen3-embed",
  "embed_text_hash": "<sha256>",
  "embedded_at": 1761000000
}
```

响应：`{ "code": 200, "data": { "updated": true } }`。

---

## 5. 代码骨架

> 以下为设计骨架，标注 `TODO` 的部分为实现时补齐；不改变现有 `post_chat` 行为。

### 5.1 `src/utils/embed.rs`

```rust
//! 文本向量化：qwen3-embedding-0.6b（1024 维）。
use anyhow::Result;
use std::sync::OnceLock;

pub const MODEL: &str = "qwen3-embed";
pub const DIM: usize = 1024;
pub const MODEL_PATH: &str = "./models/qwen3-embed/qwen3-embedding-0.6.gguf";

pub struct Embedder {
    // TODO: llama_cpp_2::llama_backend::LlamaBackend + LlamaModel + 复用 context
}

impl Embedder {
    pub fn load() -> Result<Self> {
        // TODO: 复用 utils/loader.rs 的 backend / model 加载；
        // 建议 ctx_size >= 512，n_threads 与 LlmConfig 一致。
        todo!()
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // TODO: tokenize → decode → embeddings → 取首 token 向量
        // 校验长度 == DIM，再做 L2 归一化。
        todo!()
    }
}

static EMBEDDER: OnceLock<Embedder> = OnceLock::new();

pub fn embedder() -> Result<&'static Embedder> {
    if let Some(e) = EMBEDDER.get() {
        return Ok(e);
    }
    let e = Embedder::load()?;
    Ok(EMBEDDER.get_or_init(|| e))
}

pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// 归一化向量的点积即余弦；长度不一致返回 0。
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn from_blob(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}
```

### 5.2 `src/utils/plugin.rs`

```rust
//! 插件能力：调用 plugin_lib、维护 dialogue.plugin_embeddings。
use anyhow::Result;
use serde::{Deserialize, Serialize};

const DB: &str = "dialogue";
const TABLE: &str = "plugin_embeddings";

#[derive(Debug, Deserialize)]
pub struct EmbedReq {
    pub limit: Option<i64>,
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchReq {
    pub query: String,
    pub top_k: Option<i64>,
    pub model: Option<String>,
    pub min_score: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct EmbedStats {
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub model: String,
    pub dim: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub uuid: String,
    pub lib_name: String,
    pub symbol: String,
    pub ffi_symbol: String,
    pub title: String,
    pub description: String,
    pub params_schema: serde_json::Value,
    pub return_schema: serde_json::Value,
    pub side_effects: i64,
    pub score: f32,
}

pub struct FnRow {
    pub uuid: String,
    pub lib_name: String,
    pub symbol: String,
    pub ffi_symbol: String,
    pub title: String,
    pub description: String,
    pub params_schema: String,
    pub return_schema: String,
    pub side_effects: i64,
    pub embed_text_hash: Option<String>,
}

pub fn ensure_table() -> Result<()> {
    // TODO: CREATE TABLE IF NOT EXISTS + 索引（DDL 见 §2.1）
    todo!()
}

pub fn list_pending(limit: i64, force: bool) -> Result<Vec<FnRow>> {
    // TODO: dlopen plugin_lib，调用 post_get_functions
    todo!()
}

pub fn patch_function(uuid: &str, hash: &str) -> Result<()> {
    // TODO: 调用 post_patch_function 回写 embed_status/embed_model/embed_text_hash/embedded_at
    todo!()
}

pub fn upsert_embedding(row: &FnRow, vector: &[f32], hash: &str) -> Result<()> {
    // TODO: INSERT ... ON CONFLICT(function_uuid, model) DO UPDATE
    // 若 database 内置 SQLite 不支持 UPSERT，则用 INSERT OR REPLACE。
    todo!()
}

pub fn load_vectors(model: &str) -> Result<Vec<(String, String, String, Vec<f32>)>> {
    // TODO: SELECT function_uuid, lib_name, symbol, vector FROM plugin_embeddings WHERE model = ?
    todo!()
}

pub fn build_text(row: &FnRow) -> String {
    // TODO: title + description + target + params + return
    String::new()
}

pub fn run_embed(req: EmbedReq) -> Result<EmbedStats> {
    // TODO: ensure_table → list_pending → 逐条 embed/upsert/patch
    todo!()
}

pub fn run_search(req: SearchReq) -> Result<Vec<SearchHit>> {
    // TODO: embed(query) → load_vectors → cosine → top_k → get_functions(uuids) → 过滤 is_local
    todo!()
}
```

### 5.3 `src/ffi/plugin.rs`

```rust
use crate::utils::cjson::{write_error, write_success};
use crate::utils::plugin::{self, EmbedReq, SearchReq};
use serde_json::json;
use std::ffi::{c_char, c_int, CStr};

fn read_body<'a>(json_in: *const c_char) -> Result<&'a str, (c_int, &'static str)> {
    if json_in.is_null() {
        return Err((400500, "json_in is null"));
    }
    unsafe {
        CStr::from_ptr(json_in)
            .to_str()
            .map_err(|_| (400500, "invalid utf-8"))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_plugin_embed(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(v) => v,
        Err((code, msg)) => return write_error(json_out, code, msg),
    };
    let req: EmbedReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400500, &format!("invalid json: {}", e)),
    };
    match plugin::run_embed(req) {
        Ok(stats) => write_success(json_out, "success", json!(stats)),
        Err(e) => write_error(json_out, 400502, &format!("plugin embed failed: {:#}", e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn post_plugin_search(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(v) => v,
        Err((code, msg)) => return write_error(json_out, code, msg),
    };
    let req: SearchReq = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return write_error(json_out, 400500, &format!("invalid json: {}", e)),
    };
    if req.query.trim().is_empty() {
        return write_error(json_out, 400500, "query is empty");
    }
    match plugin::run_search(req) {
        Ok(list) => write_success(
            json_out,
            "success",
            json!({
                "model": crate::utils::embed::MODEL,
                "dim": crate::utils::embed::DIM,
                "list": list
            }),
        ),
        Err(e) => write_error(json_out, 400503, &format!("plugin search failed: {:#}", e)),
    }
}
```

### 5.4 模块注册与依赖

```rust
// src/ffi/mod.rs
pub mod chat;
pub mod plugin;
pub mod run;
pub mod types;

// src/utils/mod.rs
pub mod cjson;
pub mod db;
pub mod dialogue;
pub mod embed;
pub mod markparse;
pub mod plugin;
pub mod siliconflow;
```

```toml
# Cargo.toml
sha2 = "0.10"   # embed_text_hash
```

> `utils/db.rs` 目前只有 `db_query`；写 `dialogue.db` 需要补 `db_exec`（调用 `post_db_update` / `post_db_insert`），
> 或在 `plugin.rs` 内复用同一套 libloading 封装。

---

## 6. 文本组装与哈希

输入字段（来自 `plugin_functions`）：

```text
{title}
{description}
目标: {target}
参数: {name}: {description}; ...
返回: {return_schema 的 description 或字段名}
```

规则：

- 只纳入 `is_local=1` 的函数；
- `description` 为空时用 `title` + 符号名；
- 参数按 `required` 优先、再按 schema 属性顺序；
- `sha256(text)` 写入 `text_hash`，与 `plugin_functions.embed_text_hash` 比较；
- 文本或模型变化才重建；`force=true` 忽略 hash。

---

## 7. 检索与排序

- 向量均已 L2 归一化，分数 = 点积，范围 `[-1, 1]`；
- 默认 `top_k=5`，最大 50；
- 默认 `min_score=0`，可配置；
- 同分按 `symbol` 稳定排序，保证结果可复现；
- 召回后必须回查 `plugin_functions`，丢弃 `is_local != 1` 或已删除的函数；
- 检索结果不直接执行，交由上层确认（`side_effects=1` 的能力需用户确认）。

---

## 8. 测试计划

| 层级 | 用例 |
| --- | --- |
| 单元 | `normalize` / `cosine` / `to_blob`↔`from_blob` 往返；`build_text` 字段顺序；`sha256` 稳定性 |
| 单元 | 向量长度不等于 1024 时返回 `400503` |
| 集成（mock plugin_lib） | `post_plugin_embed` 写入 `plugin_embeddings` 并回写状态；重复调用 `skipped` 增加 |
| 集成（临时 dialogue.db） | UPSERT 覆盖旧向量；模型切换保留两行 |
| 集成 | `post_plugin_search` 返回排序正确、过滤 `is_local=0`、`min_score` 生效 |
| 端到端 | cyctron + chatllm + plugin_lib：扫描 → 向量化 → 查询 → 按结果调用 `POST /<lib>` |
| 性能 | 1,000 条 1024 维向量暴力检索 < 50 ms（不含 query 向量生成） |

---

## 9. 待确认

1. **llama_cpp_2 embedding API**：当前 `utils/loader.rs` 只接生成路径；需确认 embedding 接口与首 token 取值方式；
2. **UPSERT 支持**：`database.dll` 内置 SQLite 版本是否支持 `ON CONFLICT ... DO UPDATE`；否则用 `INSERT OR REPLACE`；
3. **向量生成性能**：0.6B 模型在 CPU 上单条耗时；是否需要批量 / 缓存；
4. **plugin_lib 不可用**：降级为“跳过向量化、检索返回空列表 + 明确错误码”，还是阻断；
5. **多模型**：是否长期保留多套向量，或模型升级后清理旧行；
6. **清理时机**：插件删除时由 `plugin_lib` 主动调用 chatllm，还是 chatllm 启动时清理孤儿。