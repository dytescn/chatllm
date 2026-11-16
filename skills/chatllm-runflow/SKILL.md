---
name: chatllm-runflow
description: chatllm 对话流程：think → chat_plugins → reply、taskflow 回调、轮次记录、依赖模块与常见问题。
---

# chatllm 对话流程

## 1. 流程

```text
chat
  ├─ think：用默认分类关键词 base 检索 chat_think，命中插件并回调，
  │    输出 prompts[] / outputs[]，记录起止时间
  ├─ chat_plugins：按 think 的 prompts[] 逐条检索 chat_plugins（只取 Top1），
  │    用 inputs[]（think 的 outputs）调用 taskflow 回调，逐条记录起止时间
  └─ reply：先按默认分类 base 检索 chat_reply，未命中再按合并结果检索；
       命中插件并回调；未命中原样输出合并结果
```

`check` 已移除，后续以插件形式提供。

| 阶段 | 集合 | 输入 | 输出 |
| --- | --- | --- | --- |
| think | `chat_think` | 用户文本 | `prompts[]` / `outputs[]` |
| chat_plugins | `chat_plugins` | think 的 `prompts[]` / `outputs[]` | `outputs[]` |
| reply | `chat_reply` | chat_plugins 合并结果 | assistant 内容 |

## 2. 轮次记录

一次 `chat` 写入一条 `role = "round"` 记录，`content` 数组元素包含
`think` / `plugins` / `reply` 三个阶段（字段名沿用 `plugins`）。

- think：`input.content`、`prompts`、`outputs`、`start_time` / `end_time`、`status`；
- plugins：`inputs`、`outputs`、`calls`、`start_time` / `end_time`、`status`；
- reply：`input.content`、`outputs`、`start_time` / `end_time`、`status`。

`plugins.calls` 每条对应一个 prompt：

| 字段 | 说明 |
| --- | --- |
| `step` | 序号 |
| `prompt` / `params.keyword` | 检索关键词 |
| `params.prompt` | 传给插件的输入文本 |
| `plugin` / `plugin_uuid` | 命中插件 |
| `callbacks` | 实际调用的回调方法 |
| `start_time` / `end_time` | 本条起止时间（下一条从本条 `end_time` 开始） |
| `response` / `status` / `error` | 结果与状态 |

完整字段见 [`docs/db/round.md`](../../docs/db/round.md)。

## 3. think

- 用默认分类关键词 `base` 检索 `chat_think`，`min_score = 1.0`；
- 命中插件后请求 `{ "prompt": "<用户文本>", "prompts": [], "outputs": [] }`；
- 插件响应优先读取 `data.prompts` / `data.outputs`，旧插件只返回 `data.content`
  时作为单条 output；
- `run` 返回 `prompts[]` / `outputs[]` 与起止时间，检索失败时 `status = "error"`，
  未命中为 `skipped`，不中断对话；
- 技能从 designplugin_lib 的 `skills` 集合按 `plugin_uuid` / `_id` / `name`
  读取，写入 `dialogue.system_prompt`，不导出 FFI。

## 4. chat_plugins

1. 检索关键词直接来自 think 的 `prompts[]`，输入文本来自 think 的 `outputs[]`；
2. 对每个 prompt 检索 `chat_plugins`，只取 Top1；
3. 解析 `plugin_uuid`（`meta.plugin_uuid` / `value.plugin_uuid` / `value.uuid`）
   与插件库名；
4. 读取 taskflow 中 `plugin_uuid` 匹配的元素；
5. 执行元素回调：`callback` → `ffi_symbol` → `method` → `uid`；
6. 请求：

```json
{
  "prompt": "<inputs[i]>",
  "keyword": "<prompts[i]>",
  "prompts": ["..."],
  "inputs": ["..."],
  "step": 1
}
```

调用 `post_<callback>`，取 `data.content` 作为该条输出；插件返回 `data.input`
可更新下一条的输入。每条写入 `plugins.calls`，最终只返回 `outputs[]`。

失败保留错误文本，无任务流时跳过；插件库按名称缓存，同一线程禁止重入。

## 5. reply

1. 先按默认分类 `base` 检索 `chat_reply`，未命中再按 chat_plugins 合并结果检索；
2. 解析 `plugin`、`plugins_uid`、`plugin_name`、`ffi_symbol`；
3. 业务插件按 `post_<ffi_symbol>` 调用，取 `data.content`；
4. 未命中、调用失败或空内容时原样输出合并结果。

## 6. 依赖模块

| 模块 | 用途 |
| --- | --- |
| `utils/kv.rs` | `chat_think` / `chat_plugins` / `chat_reply` 检索 |
| `utils/designdb.rs` | 消息库、技能与 taskflow 读取 |
| `utils/plugins_lib.rs` | designplugin_lib 的 FFI 客户端，接口与参数见 [`docs/plugins_lib.md`](../../docs/plugins_lib.md) |
| `utils/chat_plugins.rs` | 插件动态库加载、回调与重入保护 |

## 7. 配置

`info.db.setting` 由 check / setting 插件读写，chatllm 不直接访问：

| key | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t` |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

检索阈值固定默认 `0.35`；集合为 `chat_think`、`chat_plugins`、`chat_reply`。

## 8. 常见问题

| 现象 | 处理 |
| --- | --- |
| 返回“请选择设置AI供应商” | 检查 check 插件与 `ai_supplier` / AppKey |
| assistant 未使用插件结果 | `chat_think` 未命中 `base`，或 `chat_plugins` 无匹配插件 |
| chat_plugins 没有输出 | think 未返回 `prompts[]`，或 `chat_plugins` 集合不存在 |
| 未执行插件回调 | 命中记录缺 `plugin_uuid`，或 taskflow 缺少回调字段 |
| 插件重入返回 `error` | 同一线程只允许一层插件调用 |
