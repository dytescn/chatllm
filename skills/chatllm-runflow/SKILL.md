---
name: chatllm-runflow
description: chatllm 对话流程：think → plugins → reply、taskflow 回调、轮次记录、配置与常见问题。
---

# chatllm 对话流程

## 1. 流程

```text
chat
  ├─ think：检索 chat_think，命中插件并回调取 Top1 value，直接交给 plugins
  ├─ plugins：用 think 的 Top1 value 检索 chat_plugins
  │    取 plugin_uuid → 读取 taskflow → 执行回调
  └─ reply：合并结果，检索 chat_reply；命中插件并回调；未命中原样输出合并结果
```

`check` 已移除，后续以插件形式提供。

| 阶段 | 集合 | 命中后 | 输出 |
| --- | --- | --- | --- |
| think | `chat_think` | 插件回调，取 Top1 `value` | Top1 `value` |
| plugins | `chat_plugins` | 用 Top1 `value` 检索 → `plugin_uuid` → taskflow → 回调 | 插件结果 |
| reply | `chat_reply` | 插件回调；未命中原样输出合并结果 | assistant 内容 |

## 2. 轮次记录

一次 `chat` 写入一条 `role = "round"` 记录，`content` 数组元素包含
`think` / `plugins` / `reply`。各阶段用 `calls` 记录每次插件调用（插件、
`prompt`、`params`、`response`），用 `precalls` 记录前置回调。字段见
[`docs/db/round.md`](../../docs/db/round.md)。

## 3. think

- 用固定关键词 `init` 检索 `chat_think`，`min_score = 1.0`；
- 命中插件并回调，回调结果写入 `outputs`；
- think 返回 `prompts[]` 与 `outputs[]`，`outputs` 直接交给 plugins；
- 未命中时跳过 plugins。

`think_base` 是可选资源包；技能从 designplugin_lib 的 `skills` 集合按
`plugin_uuid` / `_id` / `name` 读取，并写入 `dialogue.system_prompt`，不导出 FFI。
reply 未命中时直接输出 plugins 合并结果，不再使用 `reply_base`。

## 4. plugins

1. 用 think 的 Top1 `value` 检索 `chat_plugins`，取 Top1；
2. 解析 `plugin_uuid`（`meta.plugin_uuid` / `value.plugin_uuid` / `value.uuid`）
   与插件库名；
3. 读取 taskflow 中 `plugin_uuid` 匹配的元素；
4. 执行元素回调：`callback` → `ffi_symbol` → `method` → `uid`；
5. 调用 `post_<callback>`，请求 `{ "prompt": "<Top1 value>" }`，取 `data.content`。

失败保留错误文本，无任务流时跳过，最后合并结果。
插件库按名称缓存，同一线程禁止重入。

## 5. reply

1. 用 plugins 合并结果检索 `chat_reply`，取 Top1；
2. 解析 `plugin`、`plugins_uid`、`plugin_name`、`ffi_symbol`；
3. 业务插件按 `post_<ffi_symbol>` 调用，取 `data.content`；
4. 未命中、调用失败或空内容时原样输出 plugins 合并结果。

## 6. 配置

`info.db.setting`：

| key | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t` |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

检索阈值固定默认 `0.35`。集合为 `chat_think`、`chat_plugins`、`chat_reply`。

## 7. 常见问题

| 现象 | 处理 |
| --- | --- |
| 返回“请选择设置AI供应商” | 检查 check 插件与 `ai_supplier` / AppKey |
| assistant 未使用插件结果 | `chat_think` 未命中，或 `chat_plugins` 无匹配插件 |
| 未执行插件回调 | 命中记录缺 `plugin_uuid`，或 taskflow 缺少回调字段 |
| 插件重入返回 `400304` | 同一线程只允许一层插件调用 |
