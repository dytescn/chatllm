# kv_lib FFI 调用方式

chatllm 通过 `dlopen ./libary/kv_lib.dll` 直接调用 kv_lib 的 FFI 符号，
不经过 cyctron 的 HTTP 层。当前依赖版本以 `dytes.json` 为准
（`kv_lib: 0.1.5`）。

## 1. 调用约定

kv_lib 0.1.5 是 Rust 编译的 `cdylib`，导出 4 个 C ABI 函数：

| 符号 | 用途 |
| --- | --- |
| `kv_collection` | 集合列表、统计、创建 / 更新 |
| `kv_embed` | 文本组写入与向量化、重建索引 / 分片统计 / 文本哈希 |
| `kv_record` | 记录 CRUD：`put` / `list` / `get` / `delete` |
| `kv_search` | 按 `query` 文本检索 |

```c
int kv_xxx(const char *json_in, char *json_out);
```

- `json_in`：NUL 结尾的 UTF-8 JSON 请求；空字符串按 `{}` 处理。
- `json_out`：调用方提供的缓冲区；`kv_search` 至少 1 MiB，其余接口至少 64 KiB。
- 返回值：`0` 已写入响应；`-1` `json_out` 为空指针；`-2` 响应达到缓冲上限、未写入。
- 业务错误放在响应的 `code` 字段中，不用负数返回值表达。

响应统一为 `{code, data, msg}`，成功 `code = 200`：

```json
{"code":200,"data":{...},"msg":"success"}
```

`action` 默认值与可用值：

| 符号 | 默认 `action` | 可用 `action` |
| --- | --- | --- |
| `kv_collection` | `list` | `list` / `put` / `stats` |
| `kv_embed` | `embed` | `embed` / `rebuild` |
| `kv_record` | `list` | `put` / `list` / `get` / `delete` |
| `kv_search` | 无 | 总是执行检索 |

未知 `action` 返回 `400102`。

## 2. 集合

kv_lib 使用内置 SQLite（`rusqlite`），数据库文件位于 `KV_DB_DIR`
（默认 `./database`）。初始化时写入 4 个默认集合：

| code | 名称 | 用途 | sort |
| --- | --- | --- | --- |
| `prompt` | 提示词 | system prompt 模板、规则与提示词条目 | 10 |
| `plugin` | 插件能力 | 插件能力描述向量 | 20 |
| `skill` | 技能 | 技能正文向量 | 30 |
| `material` | 素材 | 素材内容文本向量 | 40 |

默认参数：`dim = 1024`、`model = qwen3-embed`、`shard_max_mb = 16`、
`index_min_rows = 0`、`state = 0`（启用）。

chatllm 对话流程另用 3 个业务集合，由插件安装流程通过 `kv_collection`
（`action=put`）或 `kv_embed` 按需创建：

| code | 用途 |
| --- | --- |
| `chat_think` | think 阶段检索（`init` 关键词） |
| `chat_plugins` | chat_plugins 阶段逐条检索 |
| `chat_reply` | reply 阶段检索 |

集合的创建 / 更新由 `kv_collection` 的 `put` 提供；记录 CRUD 由 `kv_record`
提供。`kv_embed` 仍用于按 `id` 的文本组 upsert 与重建。

## 3. `kv_collection`

### action = `list`

```json
{"action":"list","page":1,"page_size":20}
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `page` | 1 | 页码，最小 1 |
| `page_size` | 20 | 每页数量，1..=100 |

响应 `data`：

```json
{
  "list": [{
    "code":"prompt","name":"提示词","description":"system prompt 模板、规则与提示词条目",
    "dim":1024,"model":"qwen3-embed","shard_max_mb":16,"index_min_rows":0,
    "state":0,"sort":10,"generation":4,
    "row_count":4,"shard_count":1,"size_bytes":24576,
    "created_at":"2026-11-14 09:15:08","updated_at":"2026-11-14 09:15:08"
  }],
  "page":1,"page_size":20,"total":4
}
```

`row_count` / `size_bytes` 来自 `kv_shard` 缓存，可能不是实时值。

### action = `stats`

```json
{"action":"stats"}
{"action":"stats","collection":"prompt"}
```

响应 `data`：

```json
{
  "collections":[{"code":"prompt","rows":4,"vectors":3,"shards":1,
                  "size_bytes":24576,"generation":4,
                  "index":"fresh"}],
  "totals":{"collections":1,"shards":1,"rows":4,"vectors":3,"size_bytes":24576},
  "index":{"cached":1,"stale":0}
}
```

单集合 `index` 取值：`fresh`（索引缓存命中）/ `stale`（有数据但当前
generation 未缓存）/ `none`（没有可用索引数据）。

### action = `put`

```json
{
  "action":"put","code":"chat_think","name":"聊天思考",
  "description":"think 阶段检索词与技能",
  "dim":1024,"model":"qwen3-embed","shard_max_mb":16,
  "index_min_rows":0,"state":0,"sort":10
}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `code` | 是 | 集合编码 |
| `name` / `description` | 否 | 缺省保留原值；新建时 `name` 默认为 `code` |
| `dim` / `model` | 否 | 新建时默认 `1024` / `qwen3-embed`；已有集合改变 `dim` 且含记录时拒绝 |
| `shard_max_mb` / `index_min_rows` / `state` / `sort` | 否 | 缺省保留原值或默认值 |

响应 `data`：`{"collection":{...},"created":true}`。已存在时只更新传入字段，
`created` 为 `false`。

## 4. `kv_embed`

### action = `embed`：文本组写入与向量化

```json
{"action":"embed","collection":"chat_plugins","id":"550e8400-e29b-41d4-a716-446655440000","texts":["文字内容1","文字内容2"]}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `collection` | 是 | 集合编码；不存在时按嵌入器维度 / 模型自动创建 |
| `id` | 是 | 文本组唯一编号，推荐 UUID；不能含 `:`；作为记录 key 前缀 |
| `texts` | 是 | 文本数组，≤500 条，单条 ≤64 KiB；空数组表示清空该组 |

内部流程：

1. 校验 `collection` 与 `id`；
2. 集合不存在时自动创建（`dim=1024`、`model=qwen3-embed`，其余用默认值）；
3. 删除 key 以 `{id}:` 开头的旧记录；
4. 为每条文本写入 `{id}:{index}`，`value` 与 `text` 均为文本内容并生成向量。

删除与写入顺序执行、非事务；写入失败时旧组已被删除。空 `texts` 只执行删除。

响应 `data`：

```json
{
  "collection":"chat_plugins","id":"550e8400-...",
  "deleted":2,"written":2,"skipped":0,"failed":0,
  "errors":[],"model":"qwen3-embed","dim":1024,"elapsed_ms":1234
}
```

`errors` 条目格式为 `{"key":"...","code":400602,"message":"..."}`。嵌入失败时
记录可能以无向量状态写入，调用方需检查 `errors`。

### action = `rebuild`

```json
{"action":"rebuild","collection":"plugin","target":"index","force":false}
```

| `target` | 行为 |
| --- | --- |
| `index`（默认） | 清空集合索引缓存；`force=true` 时立即重建 |
| `shards` | 刷新所有非 `state=2` 分片的 `row_count` / `size_bytes` |
| `text_hash` | 按当前 `text` 重算非空文本的 `text_hash` |

响应 `data`：`{"collection":"plugin","action":"index","rebuilt":true,"processed":0,"elapsed_ms":12}`。

## 5. `kv_search`

```json
{"collection":"chat_plugins","query":"聊天","top_k":5,"min_score":0.0,"with_value":true}
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `collection` | — | 必填 |
| `query` | — | 查询文本，必填，会 trim |
| `top_k` | 5 | 结果数量，1..=100 |
| `min_score` | 0.0 | 最低相似度（点积） |
| `candidate_keys` | — | 只在这些 key 中检索 |
| `with_value` | false | 为 true 时补充 `value` / `value_type` / `meta` |

0.1.5 不再支持 `vector` 参数；请求中出现 `vector` 返回 `400603`。

响应 `data`：

```json
{
  "collection":"chat_plugins","model":"qwen3-embed","dim":1024,
  "query":"聊天","top_k":5,
  "list":[{"key":"chat-plugin-demo","score":0.93,"rank":1,
           "value":"...","value_type":"json","meta":{}}]
}
```

排序：`score` 降序 → `rank`（正数升序，`0` 未排名置后）→ `key`。
集合不存在时返回 `400600`，业务侧通常按“无命中”处理。

## 6. `kv_record`

宿主通过该接口维护 `key` / `value` / `text` / `meta` / `rank` 记录，与
`kv_embed` 的 `{id}:{index}` 文本组共用分片；`UNIQUE(key)` 仍是分片内约束。

### action = `put`

```json
{
  "action":"put","collection":"chat_think",
  "items":[{
    "key":"uuid:prompt:greeting","value":"问候|你好","text":"问候|你好",
    "meta":{"plugin_uuid":"uuid","kind":"prompt"},"rank":1,"force":false
  }]
}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `key` | 是 | 记录键，1..=128 字节 |
| `value` / `value_type` | 否 | `value_type` 为 `text` 或 `json`，默认 `text` |
| `text` / `text_hash` | 否 | `text` 非空时计算哈希并按集合模型向量化 |
| `meta` | 否 | JSON 对象，序列化后 ≤8 KiB |
| `rank` | 否 | 业务排序，正数升序，`0` 视为未排名 |
| `force` | 否 | `true` 时即使文本未变也重新嵌入 |
| `if_version` | 否 | 乐观锁；与现有版本不一致时该条失败 |

响应 `data`：`{"written":1,"skipped":0,"failed":0,"errors":[],"model":"qwen3-embed","dim":1024}`。
嵌入失败时记录可能以无向量状态写入，`errors` 会列出对应 key。

### action = `list`

```json
{"action":"list","collection":"chat_think","prefix":"uuid:prompt:","page":1,"page_size":100,"with_value":true}
```

| 字段 | 默认值 | 说明 |
| --- | --- | --- |
| `collection` | — | 必填 |
| `prefix` | 空 | 按 `key` 前缀过滤；空表示全部 |
| `page` / `page_size` | 1 / 20 | `page_size` 限制 1..=100 |
| `with_value` | true | 是否返回 `value` / `meta` |

响应 `data` 含 `list` / `total` / `page` / `page_size`；`list` 条目包含
`key`、`value`、`value_type`、`version`、`rank`、`dim`、`has_vector`、
`text_hash`、`created_at`、`updated_at`、`meta`。

### action = `get`

```json
{"action":"get","collection":"chat_think","keys":["uuid:prompt:greeting"],"with_vector":false}
```

响应 `data`：`{"list":[{...}],"missing":[]}`；`with_vector=true` 时返回向量。

### action = `delete`

```json
{"action":"delete","collection":"chat_think","keys":["uuid:prompt:greeting"]}
```

响应 `data`：`{"deleted":1,"requested":1,"missing":[]}`。

### 限制

- 单批 ≤500 条；`page_size` 1..=100；
- `meta` ≤8 KiB；`value` ≤256 KiB；`text` ≤64 KiB；
- 输出缓冲区 64 KiB（`kv_search` 为 1 MiB）。

## 7. 在对话流程中的位置

```text
chat
  ├─ think：search(chat_think, "base") → 命中插件并回调，结果交给 chat_plugins
  ├─ plugins：search(chat_plugins, think 的 Top1 value) → plugin_uuid → taskflow → 回调
  └─ reply：search(chat_reply, 合并结果) → 插件 / 资源命中，否则原样输出
```

think 用默认分类关键词 `base` 检索 `chat_think`；reply 先按 `base` 检索 `chat_reply`，未命中时原样输出合并结果。

## 8. Rust 封装

实现：`src/utils/kv.rs`，统一经 `call_ffi(symbol, request)` 调用。

| 函数 | FFI | 说明 |
| --- | --- | --- |
| `search(collection, query, top_k, min_score, candidate_keys, with_value)` | `kv_search` | 文本检索；`top_k` 夹到 1..=100 |
| `search_chat(query, top_k)` | `kv_search` | `chat` 集合检索 |
| `search_plugins(query, top_k)` | `kv_search` | `plugin` 集合检索 |
| `search_chat_plugins(query, top_k)` | `kv_search` | `chat_plugins`；集合不存在返回空 |
| `collection_list(page, page_size)` | `kv_collection` | `action = list` |
| `collection_put(code, spec)` | `kv_collection` | `action = put`；`spec` 为 `CollectionSpec` |
| `stats(collection)` | `kv_collection` | `action = stats` |
| `record_put(collection, items)` | `kv_record` | 批量写入 / 更新记录 |
| `record_list(collection, prefix, page, page_size, with_value)` | `kv_record` | 按 `key` 前缀分页读取 |
| `record_get(collection, keys, with_vector)` | `kv_record` | 按 key 读取 |
| `record_delete(collection, keys)` | `kv_record` | 按 key 删除 |
| `call_ffi(symbol, request)` | `kv_<symbol>` | 通用调用，64 KiB 缓冲 |
| `ffi_symbol(symbol)` | — | 拼出实际导出名，如 `search` → `kv_search` |
| `is_error_code(error, code)` | — | 判断 `KvError` 的业务错误码 |
| `extract_list(data)` | — | 取 `data.list`，缺失时返回空列表 |
| `parse_response(symbol, out)` | — | 解析 `{code,data,msg}`，错误转为 `KvError` |

对应的 `build_*_request`（`build_collection_put_request`、`build_record_put_request`、
`build_record_list_request`、`build_record_get_request`、
`build_record_delete_request`）是纯请求构造函数，便于测试与内省。

> **符号名说明**：0.1.5 的 PE 导出表是 `kv_collection` / `kv_embed` /
> `kv_record` / `kv_search`，不带 `post_` 前缀（0.1.4 曾导出 `post_kv_*`）。直接
> `dlopen` + `GetProcAddress` 按名精确匹配，升级时只需改 `kv::FFI_PREFIX`。
> 可用 `dumpbin /exports libary/kv_lib.dll`（或 `objdump -p`）核对导出表。
>
> `kv_search` 的输出缓冲为 1 MiB，其余接口为 64 KiB；`call_ffi` 默认 64 KiB，
> 检索路径内部使用 1 MiB。`kv_embed` 暂无专用封装，需要时用
> `call_ffi("embed", ...)` 调用。

## 9. 错误码

| code | 含义 |
| --- | --- |
| `400100` | `json_in` 为空指针 |
| `400101` | 请求不是 UTF-8 |
| `400102` | JSON / 反序列化错误、未知 action、缺必填字段、分页 / 数量参数非法 |
| `400600` | 未知集合 |
| `400601` | 非法 key |
| `400602` | 向量维度不匹配 |
| `400603` | 非法检索：缺少 `query`、提供 `vector`、候选超过限制 |
| `400604` | 非法集合编码（自动创建或内部集合操作） |
| `400605` | 集合只读 / 禁用 |
| `400606` | 文本超过 64 KiB |
| `500600` | 嵌入失败（模型缺失、加载失败等） |
| `500601` | 数据库调用失败 |
| `500602` | 分片创建失败 |
| `500603` | 索引构建 / 缓存失败 |

## 10. 运行环境

- 数据库：内置 SQLite，`KV_DB_DIR` 指定目录（默认 `./database`）；
  0.1.5 不再依赖 `./libary/database.dll`。
- 嵌入模型：默认 `./models/qwen3-embed/qwen3-embedding-0.6.gguf`，
  可用 `KV_EMBED_MODEL` 覆盖；`KV_EMBED_THREADS` 控制嵌入线程数
  （默认 `min(CPU 核数, 4)`）。
- `turbovec` 要求维度为 8 的正整数倍；`dim=1024` 走索引检索，其他维度回退精确扫描。
