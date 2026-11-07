# chatllm FFI 接口

`chatllm` 编译为 `cdylib`，由宿主通过 `POST /chatllm` + `FFI-Symbol` 调用。
对话流程为 `check → think → plugins → reply`；数据存储见
[`db.md`](db.md)，向量检索见 [`kv_lib.md`](kv_lib.md)。

> 当前版本 **1.0.11**。供应商调用统一走 `cloud_lib`；对话表由 `setup.json` 初始化。

## 1. 调用约定

```c
int (*)(const char *json_in, char *json_out);
```

| 约定 | 说明 |
| --- | --- |
| 输入 | NUL 结尾的 UTF-8 JSON；`json_in = NULL` 返回 `400100`，非 UTF-8 返回 `400101` |
| 输出 | 调用方提供 65,536 字节缓冲区（含结尾 NUL）；超限返回 `-2` |
| 返回值 | `0` 已写入；`-1` 输出指针为空；`-2` 缓冲区不足；业务失败仍返回 `0`，查看响应 `code` |
| 响应 | `{"code":200,"data":{...},"msg":"success"}`；失败为 `{"code":<错误码>,"data":{},"msg":"<原因>"}` |

## 2. 接口

| FFI-Symbol | 导出函数 | 说明 |
| --- | --- | --- |
| `chat` | `post_chat` | 新建 / 续聊并执行对话流程，写 user 与 assistant |
| `dialogue_list` | `post_dialogue_list` | 对话分页列表 |
| `dialogue_update` | `post_dialogue_update` | 更新对话字段 |
| `dialogue_search` | `post_dialogue_search` | 按标题搜索 |
| `chat_detail` | `post_chat_detail` | 读取对话与消息 |

### 2.1 `chat` → `post_chat`

请求：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `prompt` | string | 是 | 用户消息 |
| `uuid` | string | 否 | 续聊已有对话 |
| `plugin_uuid` | string | 否 | 新建时作为对话 `id`，默认 `show_status = 2` |
| `show_status` | number | 否 | `1` 正常显示，`2` 不显示 |
| `title` | string | 否 | 对话标题；缺省取 `prompt` 前 20 字符 |
| `max_tokens` | number | 否 | 原样回显，不参与对话逻辑 |

目标对话优先级：`uuid` > `plugin_uuid` > 自动生成。

流程：

1. `check`：读取 `info.db.setting.ai_supplier` 与对应 AppKey；
2. `think`：检索 `chat_think`，取 Top1 的 `value`，按 `|` 拆分；
3. `plugins`：逐段检索 `chat_plugin` 并执行 Top1 插件；
4. `reply`：用合并结果检索 `plugins_reply`，调用命中插件生成回复；未命中回退 `reply_base`。

供应商或 AppKey 缺失时，`check` 直接返回，不创建对话、不写消息：

```json
{
  "code": 200,
  "data": {
    "uuid": null,
    "created": false,
    "content": "请选择设置AI供应商并配置 AppKey：[前往设置](/setting)",
    "content_html": "",
    "route": "/setting"
  },
  "msg": "success"
}
```

正常响应：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `uuid` | string | 对话 id |
| `created` | boolean | 是否新建 |
| `prompt` | string | 请求回显 |
| `max_tokens` | number \| null | 请求回显 |
| `route` | null | 正常流程为 `null` |
| `plugin_uuid` | string \| null | think 命中的资源包 uuid |

`check` 成功后会缓存 `info.db` 的修改时间与大小；文件变化后缓存自动失效。
供应商插件（`deepseek` / `siliconflow_t2t`）由 `cloud_lib` 调用，chatllm 不直接加载供应商动态库。

### 2.2 `dialogue_list` → `post_dialogue_list`

请求可省略 body：

```json
{ "keyword": "标题", "page": 1, "page_size": 20 }
```

`keyword` 非空时按标题搜索（等价 `dialogue_search`）；`page_size` 范围 1–100。过滤 `is_archived = 0 AND show_status != 2`，按
`is_pinned DESC, updated_at DESC` 排序。响应：

```json
{ "list": [], "page": 1, "page_size": 20, "total": 0 }
```

### 2.3 `dialogue_update` → `post_dialogue_update`

请求：

```json
{
  "uuid": "<dialogue-id>",
  "title": "新标题",
  "show_status": 1,
  "is_pinned": 0,
  "is_archived": 0
}
```

`uuid` 也兼容 `id`。只更新出现的字段，并刷新 `updated_at`。响应：`{ "id": "<uuid>" }`。

### 2.4 `dialogue_search` → `post_dialogue_search`

请求可省略 body：

```json
{ "keyword": "标题", "page": 1, "page_size": 20 }
```

按 `title` 不区分大小写匹配；`keyword` 为空时退化为普通列表。响应结构与 `dialogue_list` 一致。

### 2.5 `chat_detail` → `post_chat_detail`

请求：

```json
{ "uuid": "<dialogue-id>", "before_id": null, "after_id": null, "limit": 20 }
```

`before_id` / `after_id` 二选一；都不传时返回最新 `limit` 条。响应：

```json
{
  "dialogue": { "...": "对话字段" },
  "messages": {
    "list": [],
    "limit": 20,
    "first_id": null,
    "last_id": null,
    "has_more_before": false,
    "has_more_after": false
  }
}
```

每条消息附带 `content_html`（由 `parse` 即时渲染）。

## 3. 错误码

| 码 | 含义 |
| --- | --- |
| `400100` | `json_in` 为空 |
| `400101` | 请求非 UTF-8 |
| `400102` | 请求 JSON 非法 |
| `400103` | uuid 非法 |
| `400300` | 创建对话失败 |
| `400301` | 追加 user 消息失败 |
| `400302` | 对话列表失败 |
| `400303` | 读取 / 更新对话失败 |

## 4. 相关文件

| 文件 | 内容 |
| --- | --- |
| `src/ffi/chat.rs` | FFI 入口 |
| `src/ffi/types.rs` | 请求结构 |
| `src/utils/cjson.rs` | 响应包络与缓冲区 |
| `src/utils/check.rs` | 供应商配置检查 |
| `src/utils/think.rs` | `chat_think` 检索与分段 |
| `src/utils/plugins.rs` | `chat_plugin` 检索与插件调用 |
| `src/utils/reply.rs` | 结果回复 |
| `src/utils/reply_base.rs` | reply_base 资源包读取与清洗 |
| `src/utils/markparse.rs` | `content_html` 渲染 |
