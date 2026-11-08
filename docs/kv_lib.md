# kv_lib 调用方式

> 适用：`kv_lib` 0.1.0，chatllm 1.0.8
> 上游完整接口见 `dytes_sys_operations/sysinfo/sys/kv_lib/ffi.md`；本文只整理
> chatllm 侧会用到的调用方式。

## 1. 定位

`kv_lib` 是通用键值 + 向量检索库，数据落在 `database/kv.db` 与
`database/kv_<collection>_<n>.db`。chatllm 有两条调用路径：

| 场景 | 调用方 | 方式 |
| --- | --- | --- |
| 宿主 / UI 主动检索插件 | 宿主 | `POST /kv_lib` + `FFI-Symbol: kv_*` |
| chatllm 内部需要检索 | chatllm | `src/utils/kv.rs` 直接 `dlopen ./libary/kv_lib.dll` |

两条路径读写同一份数据，不会重复初始化。

## 2. 调用约定

### 2.1 HTTP

```http
POST http://127.0.0.1:44944/kv_lib
Content-Type: application/libary
FFI-Symbol: kv_search

{ "collection": "plugin", "query": "导出 PDF", "top_k": 5 }
```

cyctron 会把 `kv_*` 拼成导出函数 `post_kv_*`；`FFI-Symbol` 不带 `post_` 前缀。

### 2.2 C ABI

```c
int post_kv_search(const char *json_in, char *json_out);
```

| 项 | 约定 |
| --- | --- |
| 输入 | NUL 结尾 UTF-8 JSON |
| 输出 | 调用方提供 65,536 字节缓冲区（含结尾 NUL） |
| 返回值 | `0` 已写入；`-1` 输出指针为空；`-2` 输出超 64 KiB |
| 业务状态 | 看响应体 `code`，不看 HTTP 状态码 |
| 响应包络 | `{"code":200,"data":{...},"msg":"success"}` |

## 3. 集合

首次调用会自动创建 `database/kv.db` 并初始化四个集合：

| code | 名称 | 用途 |
| --- | --- | --- |
| `prompt` | 提示词 | system prompt 模板、规则与提示词条目 |
| `plugin` | 插件能力 | 插件能力描述向量（chatllm 检索插件用这个） |
| `skill` | 技能 | 技能正文向量 |
| `material` | 素材 | 素材内容文本向量 |
| `chat_plugin` | 聊天插件 | chat 分类插件的能力描述向量；`chat` 流程检索此集合，首次使用时自动创建 |

`prompt / plugin / skill / material` 由 kv_lib 首次初始化时自动建好；
`chat_plugin` 由 `ensure_chat_plugin_collection()`（或任意调用方）按需创建。

集合默认 `dim = 1024`、`model = qwen3-embed`，不要手工改 `dim`。

## 4. Rust 封装

实现：`src/utils/kv.rs`。首次调用时加载 `./libary/kv_lib.dll`（Linux
`./libary/kv_lib.so`），之后复用同一个 `Library`。

| 函数 | 对应 FFI | 说明 |
| --- | --- | --- |
| `call_ffi(symbol, &request)` | 任意 `post_kv_*` | 通用入口，返回响应 `data` |
| `collection_list(page, page_size)` | `kv_collection`（`action=list`） | 集合列表与统计 |
| `get(collection, keys, with_vector)` | `kv_record`（`action=get`） | 按键读取记录 |
| `put(collection, items)` | `kv_record`（`action=put`） | 批量写入 / 更新，按 `text` 生成向量 |
| `list(collection, prefix, page, page_size)` | `kv_record`（`action=list`） | 分页扫描 |
| `search(collection, query, top_k, min_score, candidate_keys, with_value)` | `kv_search` | 文本相似度检索 |
| `search_plugins(query, top_k)` | `kv_search` | 固定 `collection=plugin`，返回候选数组 |
| `collection_put(code, name, description, sort)` | `kv_collection`（`action=put`） | 创建 / 更新集合 |
| `ensure_chat_plugin_collection()` | `kv_collection`（`action=put`） | 确保 `chat_plugin` 存在 |
| `search_chat_plugins(query, top_k)` | `kv_search` | 固定 `collection=chat_plugin`，返回候选数组 |
| `embed(collection, keys, limit, offset, force)` | `kv_embed`（`action=embed`） | 补齐 / 刷新向量 |
| `stats(collection)` | `kv_collection`（`action=stats`） | 集合 / 分片 / 索引统计 |

### 4.1 检索聊天插件

```rust
use crate::utils::kv;

let hits = kv::search_chat_plugins("帮我选一个聊天模型", 5)?;
for hit in hits {
    let key = hit["key"].as_str().unwrap_or("");
    let score = hit["score"].as_f64().unwrap_or(0.0);
    let meta = &hit["value"]; // 插件元数据 JSON 字符串
    eprintln!("{} {:.4} {}", key, score, meta);
}
```

`search_chat_plugins` 会先确保 `chat_plugin` 集合存在，再执行
`kv_search`；`chat` 流程内部使用该函数。

### 4.2 写入聊天插件能力

真实数据由 `plugin_lib` 同步；测试或联调时可手工写入：

```rust
use serde_json::json;
use crate::utils::kv;

kv::put("chat_plugin", &[json!({
    "key": "chat-plugin-deepseek",
    "value": "{\"name\":\"DeepSeek\",\"lib_name\":\"libdeepseek\",\"ffi_symbol\":\"chat\"}",
    "value_type": "json",
    "text": "DeepSeek 文本对话模型，适合通用聊天与推理",
    "rank": 1,
    "meta": { "plugin": "deepseek", "category": "chat" }
})])?;
```

`text` 是向量化源文本；相同文本会跳过（`skipped`），不重复生成向量。
`rank` 是业务排名，数值越小越靠前，`0` 表示未排名。

### 4.3 错误处理

- FFI 返回 `-2` → `kv_lib <symbol> 输出超过 64 KiB`
- 业务 `code != 200` → `kv_lib <symbol> 错误 (code=..., msg=...)`
- 响应非 UTF-8 / 非法 JSON → 带原始响应的上下文错误

调用方按普通 `anyhow::Result` 处理；检索失败时可降级为“无候选”。

## 5. 常用 HTTP 调用

以下命令均以 testenv 为例（`127.0.0.1:44944`）。

### 5.1 集合列表

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_collection" \
  -d '{"action":"list","page":1,"page_size":20}'
```

### 5.2 创建 chat_plugin 并写入聊天插件

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_collection" \
  -d '{"action":"put","code":"chat_plugin","name":"聊天插件","dim":1024,"model":"qwen3-embed"}'
```

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_record" \
  -d '{
    "action": "put",
    "collection": "chat_plugin",
    "items": [{
      "key": "chat-plugin-deepseek",
      "value": "{\"name\":\"DeepSeek\",\"lib_name\":\"libdeepseek\",\"ffi_symbol\":\"chat\"}",
      "value_type": "json",
      "text": "DeepSeek 文本对话模型，适合通用聊天与推理",
      "rank": 1,
      "meta": {"plugin": "deepseek", "category": "chat"}
    }]
  }'
```

响应：

```json
{ "code": 200, "data": { "written": 1, "skipped": 0, "failed": 0, "model": "qwen3-embed", "dim": 1024 } }
```

### 5.3 检索聊天插件

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_search" \
  -d '{"collection":"chat_plugin","query":"帮我选一个聊天模型","top_k":3,"with_value":true}'
```

响应（实测，按 rank 升序）：

```json
{
  "code": 200,
  "data": {
    "collection": "chat_plugin",
    "model": "qwen3-embed",
    "dim": 1024,
    "list": [
      { "key": "chat-plugin-deepseek", "rank": 1, "score": 0.6965,
        "value": "{\"name\":\"DeepSeek\",\"lib_name\":\"libdeepseek\",\"ffi_symbol\":\"chat\"}" },
      { "key": "chat-plugin-siliconflow", "rank": 2, "score": 0.6948 },
      { "key": "chat-plugin-do-test-chat", "rank": 3, "score": 0.5705 }
    ]
  }
}
```

**排序规则**：`rank` 升序优先（`0` / 负数视为未排名，排在最后），
同 rank 再按 `score` 降序、`key` 升序。`kv_record`（`action=list`）使用相同规则；
`kv_record`（`action=get`）每条记录也会返回 `rank`。

参数：

| 字段 | 说明 |
| --- | --- |
| `query` | 用户指令；由库内 qwen3-embed 编码 |
| `vector` | 与 `query` 二选一，十六进制 f32 向量 |
| `top_k` | 默认 5，上限 100 |
| `min_score` | 默认 0，按余弦分数过滤 |
| `candidate_keys` | 可选，先由 `plugin_lib` 元数据筛选再点查 |
| `with_value` | `true` 时返回 `value` / `meta` |

### 5.4 读取记录

```bash
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_record" \
  -d '{"action":"get","collection":"chat_plugin","keys":["chat-plugin-deepseek"]}'
```

### 5.5 扫描 / 统计 / 补向量

```bash
# 分页扫描
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_record" \
  -d '{"action":"list","collection":"chat_plugin","page":1,"page_size":20}'

# 集合统计
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_collection" \
  -d '{"action":"stats","collection":"chat_plugin"}'

# 为文本非空的记录补向量
curl -s -X POST http://127.0.0.1:44944/kv_lib \
  -H "Content-Type: application/libary" -H "FFI-Symbol: kv_embed" \
  -d '{"action":"embed","collection":"chat_plugin","limit":100,"force":false}'
```

## 6. chat_think / chat_plugin 在对话流程中的位置

```text
用户输入
  │
  ├─ POST /chatllm  chat
  │     1. 写 user 消息
  │     2. check：读取 ai_supplier 与对应 AppKey
  │          供应商缺失 / AppKey 缺失 → assistant：请选择设置AI供应商并配置 AppKey + route=/setting
  │     3. think：kv_search(collection=chat_think, query=用户输入)
  │          按 "|" 拆分为数组
  │     4. plugins：逐段 kv_search(collection=chat_plugin, query=片段)
  │          命中 → 调用 Top1 插件
  │          供应商类插件（deepseek / siliconflow_t2t）→ cloud::chat_with → cloud_lib
  │          未命中 → 跳过该段
  │     5. reply：合并插件结果 → 检索 chat_reply → 插件调用 cloud → assistant 最终内容
  │
  ├─ POST /chatllm  call_chat
  │     kv_search(collection = category, query = prompts) → 调用 Top1 插件
  │     插件请求：{ plugins_uid, category, prompts, context }
  │     返回插件 data；不经过 check / think / reply
  │
  └─ chat_detail 读取 user + assistant
```

- think 未命中或分割后没有有效段时，跳过插件，直接把用户 prompt 交给 reply；
- plugins 失败时，把错误文本作为结果交给 reply；
- 供应商类插件由 `src/utils/cloud.rs` 统一调用独立系统库 `cloud_lib`；示例中的
  `lib_name` 仅作为插件标识，实际不会加载 `libdeepseek` / `libsiliconflow_t2t`；
- `ai_supplier` 或 AppKey 缺失时，check 阶段直接返回设置提示，不进入后续流程。

完整可执行流程见 `runflow/base.md`；think 命中后把 `|` 拆分出的数组逐段查询插件，
合并结果交给 reply。

## 7. 模型与存储

| 项 | 位置 |
| --- | --- |
| 模型 | `./models/qwen3-embed/qwen3-embedding-0.6.gguf` |
| 模型覆盖 | 环境变量 `KV_EMBED_MODEL=<gguf 绝对路径>` |
| 元数据库 | `database/kv.db`（`kv_collection` / `kv_shard`） |
| 集合分片 | `database/kv_<collection>_<n>.db`（`kv_record`） |

- `KV_EMBED_MODEL` 必须在启动 cyctron 前设置；dll 与 cyctron 同进程。
- `kv_record` 含 `rank INTEGER NOT NULL DEFAULT 0`；旧分片在首次
  `ensure_schema` 时自动执行 `ALTER TABLE ... ADD COLUMN rank`。
- 模型缺失时 `kv_record`（`action=put`）/ `kv_search`（文本查询）返回 `500600`。
- 纯文本检索失败可降级为无候选；写入失败应记录日志并继续对话。

## 8. 错误码

| code | 含义 |
| --- | --- |
| `400100` / `400101` / `400102` | 空请求 / 非 UTF-8 / JSON 解析失败 |
| `400600` | 集合不存在 |
| `400601` | `key` 非法 |
| `400602` | 向量格式 / 维度不合法 |
| `400603` | `top_k` / `min_score` 不合法 |
| `400605` | 集合只读 / 停用，或非空集合未 `hard` 删除 |
| `400606` | `value` / `text` / `meta` 超限 |
| `409101` | `if_version` 版本冲突 |
| `500600` | 模型加载 / 向量生成失败 |
| `500601` | `database.dll` 读写失败 |
| `500602` / `500603` | 分片 / 索引打开或检索失败 |
| `500900` | 输出超过 64 KiB |

## 9. 验证

```bash
# 单元测试：响应解析
cargo test kv

# call_chat 冒烟测试：分类检索 + 插件调用
cargo test --test base_call_chat

# 端到端：启动 testenv 后执行基础流程
(cd testenv && ./cyctron.exe start)
deno run --allow-net ts/test_pipeline.ts
(cd testenv && ./cyctron.exe stop)
```
