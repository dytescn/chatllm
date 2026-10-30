# chatllm 数据库

> 快照：chatllm **1.0.6**。只描述当前表结构。
> 接口层说明见 `ffi.md`。

## 1. 存储总览

| 存储 | 引擎 | 位置 | 表 / 集合 | 内容 |
| --- | --- | --- | --- | --- |
| SQLite | `./libary/database.dll` | `./database/dialogue.db` | `dialogue` | 对话索引与状态 |
| designdb | `./libary/libdesigndb.dll` | `./designdb/<dialogue_id>` | `messages` | 对话消息 |

- SQLite 请求 `{"path": "<库名>", "sql": "<SQL>"}`，库名 `dialogue` 映射到 `./database/dialogue.db`；
- designdb 以逻辑库 + 集合访问，`path` 为逻辑库名（对话 UUID）；
- 原 `design_plugins.prompts`、`embedding_category` / `embedding_shard` 与向量分片已移除。

## 2. `dialogue` 表（SQLite）

### 2.1 列结构

| 列 | 类型 / 默认 | 说明 |
| --- | --- | --- |
| `id` | `TEXT PRIMARY KEY NOT NULL` | 对话 UUID（36 字符）；插件对话用 `plugin_uuid` |
| `title` | `TEXT NOT NULL DEFAULT ''` | 标题 |
| `keyword` | `TEXT NOT NULL DEFAULT ''` | 关键字，用于搜索 |
| `model_name` | `TEXT NOT NULL DEFAULT ''` | 新建时写空字符串 |
| `system_prompt` | `TEXT` | 组装后的 system prompt，可为 `NULL` |
| `model_config` | `TEXT` | JSON 字符串，读取时解析为对象 |
| `parent_id` | `TEXT` | 预留 |
| `skills_json` | 已移除；旧库中的列由 `normalize_dialogue_row` 隐藏 |
| `context_json` | `TEXT` | 预留 |
| `token_count` | `INTEGER NOT NULL DEFAULT 0` | 预留，未使用 |
| `status` | `INTEGER NOT NULL DEFAULT 0` | 预留 |
| `is_pinned` | `INTEGER NOT NULL DEFAULT 0` | 置顶 |
| `is_archived` | `INTEGER NOT NULL DEFAULT 0` | 归档；列表默认排除 |
| `session_id` | `TEXT NOT NULL DEFAULT ''` | 模型响应 id |
| `show_status` | `INTEGER NOT NULL DEFAULT 1` | `1` 正常、`2` 不显示（插件对话） |
| `message_count` | `INTEGER NOT NULL DEFAULT 0` | 消息条数 |
| `last_seq` | `INTEGER NOT NULL DEFAULT 0` | 最后消息序号 |
| `created_at` | `DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP` | 文本时间（UTC） |
| `updated_at` | `DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP` | 文本时间（UTC），写入时刷新 |

### 2.2 索引

- `idx_dialogue_archived_created`：`(is_archived, created_at DESC)`；
- `idx_dialogue_pinned_created`：`(is_pinned DESC, created_at DESC)`。

### 2.3 写入规则

- 新建：写 `id / title / keyword / model_name / system_prompt / show_status`，
  `message_count` 与 `last_seq` 为 0；
- 更新：只更新请求中出现的字段，并刷新 `updated_at`；
- `ensure_dialogue_schema` 在业务调用前执行：表不存在时按 2.1 建表，
  表存在时缺列则 `ALTER TABLE dialogue ADD COLUMN` 补齐。

## 3. designdb 对话表

### 3.1 结构

- 每个对话一个逻辑库：库名 = `dialogue.id`（36 字符 UUID）；
- 集合固定为 `messages`；
- 文档结构：

```json
{
  "_id": "<uuid v4，消息文档 id>",
  "dialogue_id": "<对话 id>",
  "seq": 1,
  "role": "user",
  "content": "消息原文",
  "created_at": 1761000000000
}
```

| 字段 | 说明 |
| --- | --- |
| `_id` | 消息文档 id；读取时归一化为 `uuid` |
| `dialogue_id` | 所属对话 |
| `seq` | 从 1 递增，等于 `dialogue.last_seq + 1`；读取时复制为 `id` |
| `role` | `user` / `assistant` / `system` |
| `content` | 消息原文 |
| `created_at` | Unix 毫秒 |

### 3.2 写入

`append_message(uuid, role, content)`：

1. 读 `dialogue.last_seq` / `message_count`；
2. 生成 `seq = last_seq + 1` 与消息 `_id`；
3. `designdb::insert(uuid, "messages", doc)`；
4. `UPDATE dialogue SET last_seq = ?, message_count = message_count + 1,
   updated_at = datetime('now')`。

### 3.3 读取

- `message_history(uuid)`：全量读取后按 `seq` 升序排序；
- `list_messages(uuid, before_id, after_id, limit)`：游标分页，`limit` 默认 20、范围 1–100；
  返回 `list / limit / first_id / last_id / has_more_before / has_more_after`。

### 3.4 删除

先 `designdb::delete(id, "messages", {})` 清空消息库，再 `DELETE FROM dialogue WHERE id = ?`。

## 4. 跨表一致性

- 消息写 designdb、计数写 SQLite，两步跨库且无事务；第二步失败会导致
  `message_count` / `last_seq` 偏小，删除对话同理可能留下空对话索引；
- `dialogue.created_at / updated_at` 是 `datetime('now')` 文本（UTC），
  `messages.created_at` 是 Unix 毫秒；
- 两个存储单次响应不超过 65,536 字节，消息扫描页大小 50 为此留余量。

## 5. 相关文件

| 文件 | 内容 |
| --- | --- |
| `src/utils/db.rs` | SQLite 访问封装 |
| `src/utils/designdb.rs` | designdb 访问封装 |
| `src/utils/dialogue.rs` | `dialogue` 表 schema、对话与消息读写 |
| `setup.ts` | `dialogue` 表、索引与 PRAGMA 初始化 |
