# chatllm FFI 接口

> 快照：chatllm **1.0.6**。当前只保留对话与消息持久化，不调用模型、不注入 skills。
> 数据设计见 `db.md`。

## 1. 调用约定

chatllm 编译为 `cdylib`（Windows `chatllm.dll`、Linux `libchatllm.so`、macOS `libchatllm.dylib`），
由宿主以 **`POST /chatllm` + `FFI-Symbol`** 调用。

```c
int (*)(const char *json_in, char *json_out);
```

| 约定 | 说明 |
| --- | --- |
| 输入 | NUL 结尾的 UTF-8 JSON；`json_in = NULL` 返回 `400100`，非 UTF-8 返回 `400101`；`dialogue_list` / `dialogue_search` 允许空 body |
| 输出 | 调用方提供 65,536 字节缓冲区（含结尾 NUL）；超限不写入并返回 `-2` |
| 返回值 | `0` 已写入；`-1` 输出指针为空；`-2` 缓冲区不足；业务失败仍返回 `0`，看响应体 `code` |
| 响应包络 | `{"code":200,"data":{...},"msg":"success"}`；失败为 `{"code":<错误码>,"data":{},"msg":"<原因>"}` |
| 分页 | 对话列表 `page_size` 1–100；消息 `limit` 1–100 |

## 2. 接口总表

| FFI-Symbol | 导出函数 | 分类 | 说明 |
| --- | --- | --- | --- |
| `chat` | `post_chat` | 对话 | 新建 / 续聊并写入 user 消息；不生成回复 |
| `dialogue_list` | `post_dialogue_list` | 对话 | 对话分页列表（排除归档与 `show_status = 2`） |
| `dialogue_update` | `post_dialogue_update` | 对话 | 更新对话字段 |
| `dialogue_search` | `post_dialogue_search` | 对话 | 按 `title` / `keyword` 分页搜索 |
| `chat_detail` | `post_chat_detail` | 对话 | 读取某对话消息，`before_id` / `after_id` 游标分页 |

## 3. 对话接口

### 3.1 `chat` → `post_chat`

当前流程：校验 / 解析目标对话 → 可选写入 `system_prompt` → 写 user 消息 → 返回对话信息。
**不调用 siliconflow、不注入 skills、不写 assistant 消息。**

**请求 `ChatReq`**

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `prompt` | string | 是 | 用户消息；写入 `messages` |
| `uuid` | string | 否 | 续聊已有对话；必须是 36 字符合法 UUID，否则 `400103` |
| `dialogue_id` | string | 否 | 新建时指定 `dialogue.id`；默认 `show_status = 1` |
| `plugin_uuid` | string | 否 | 插件对话：等价指定 `dialogue.id`；默认 `show_status = 2` |
| `show_status` | number | 否 | 新建对话的显示状态：`1` 正常、`2` 不显示 |
| `name` | string | 否 | 新建对话标题；缺省为 `插件对话 <plugin_uuid>` 或 prompt 前 20 个字符 |
| `keyword` | string | 否 | 新建时写入；续聊时更新该对话的 `keyword` |
| `max_tokens` | number | 否 | 仅原样回显，不参与任何调用 |
| `system_prompt` | string | 否 | 显式 system prompt；新建时写入，续聊时覆盖保存 |

**目标对话解析优先级**：`uuid` 续聊 > `plugin_uuid` > `dialogue_id` > 自动生成。

**成功响应 `data`**

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `uuid` | string | 对话 id |
| `created` | boolean | 是否新建 |
| `prompt` | string | 请求 `prompt` 回显 |
| `content` | string | 固定为空字符串；模型生成已移除 |
| `content_html` | string | 固定为空字符串；模型生成已移除 |
| `max_tokens` | number \| null | 请求值回显 |
| `keyword` | string \| null | 请求值回显 |
| `session_id` | string \| null | `dialogue.session_id`，由宿主 / 其他服务维护 |
| `system_prompt` | string \| null | 当前对话的 system prompt |

> 响应中不再返回 `skills` / `truncated`。

**主要错误码**：`400100` / `400101` / `400102` / `400103` / `400300` / `400301` /
`400303` / `400305`。

### 3.2 `dialogue_list` → `post_dialogue_list`

请求（可省略 body）：`{page?, page_size?}`，默认 1 / 20，`page_size` clamp 到 1–100。
过滤 `is_archived = 0 AND show_status != 2`，排序 `is_pinned DESC, updated_at DESC`。
响应：`{list, page, page_size, total}`。

### 3.3 `dialogue_update` → `post_dialogue_update`

请求：`{id, title?, keyword?, model_name?, system_prompt?, model_config?, is_pinned?, is_archived?, session_id?, show_status?}`。
只更新出现的字段，并刷新 `updated_at`。响应：`{id}`。

### 3.4 `dialogue_search` → `post_dialogue_search`

请求（可省略 body）：`{keyword?, page?, page_size?}`；在 `title`、`keyword` 中不区分大小写匹配。
响应结构与 `dialogue_list` 一致。

### 3.5 `chat_detail` → `post_chat_detail`

请求：`{uuid, before_id?, after_id?, limit?}`；`before_id` / `after_id` 二选一，
都不传时返回最新 `limit` 条。响应：`{list, limit, first_id, last_id, has_more_before, has_more_after}`。
每条消息额外返回 `content_html`（由 parse.dll 即时生成）。

## 4. 错误码总表

| 码 | 含义 | 出现接口 |
| --- | --- | --- |
| `400100` | `json_in` 为空 | 全部 |
| `400101` | 请求非 UTF-8 | 全部 |
| `400102` | 请求 JSON 非法 | `chat` |
| `400103` | uuid 非法 | `chat`、`dialogue_update` |
| `400104` | `dialogue_update` JSON 非法或缺少 `id` | `dialogue_update` |
| `400105` | `dialogue_search` JSON 非法 | `dialogue_search` |
| `400106` | `dialogue_list` JSON 非法 | `dialogue_list` |
| `400107` | `chat_detail` JSON 非法 | `chat_detail` |
| `400300` | 创建对话失败 | `chat` |
| `400301` | 追加 user 消息失败 | `chat` |
| `400302` | 对话列表失败 | `dialogue_list` |
| `400303` | 读取 / 更新对话失败 | `chat`、`dialogue_update` |
| `400304` | 对话搜索失败 | `dialogue_search` |
| `400305` | 更新 keyword 失败 | `chat` |
| `400306` | 读取消息详情失败（含 uuid 非法） | `chat_detail` |

## 5. 相关文件

| 文件 | 内容 |
| --- | --- |
| `src/ffi/chat.rs` | 对话类 FFI 入口 |
| `src/ffi/run.rs` | `chat` 的对话选择与消息写入流程 |
| `src/ffi/types.rs` | 请求结构 |
| `src/utils/cjson.rs` | 响应包络与缓冲区写入 |
| `src/utils/dialogue.rs` | 对话与消息存储、分页与游标 |
| `src/utils/markparse.rs` | 消息 `content_html` 生成 |
| `ts/client.ts`、`ts/test_*.ts` | 宿主调用与手工测试脚本 |
