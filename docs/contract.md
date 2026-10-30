# chatllm 接口契约 v1（候选冻结）

> **2026-10-30 更新**：siliconflow 模型调用与 skills 注入已移除；`chat` 只写入 user 消息。
> 契约中与模型生成 / skills 相关的字段与工具仅作历史记录。

> 状态：**候选冻结**，等待产品 / 宿主签署。签署后作为 M1a 起的实现依据。
> 范围：`post_chat` 及其新增字段；其他 FFI 维持现状。
> 依据：`docs/facts.md`（1.0.6 现状）与 `docs/analyze.md`（分析与路线图）。
> 原则：只增加可选字段，不改现有字段语义；错误码只增不改。

## 1. 契约范围

- 本文定稿 M0 所需的输入 / 输出结构与默认值；
- 只覆盖与运行流程相关的 `post_chat`；`dialogue_*` 维持现有接口；`plugin_*` / `material_*` / `embed_prompts` / `turbo_search_query` 已移除，不再属于契约范围；
- 所有字段名为候选，签署后不再随意改名；
- 未实现前，任何调用方都不得依赖新字段。

## 2. 责任边界

| 节点 | 负责 | 不负责 |
| --- | --- | --- |
| 输入采集、文件、文件夹 | 宿主 | chatllm 不读文件、不遍历目录 |
| 通用文件解析 | 宿主（后续可抽独立 sys 能力） | chatllm 不引入解析依赖 |
| 设计格式导出 | 对应插件 | chatllm 不连接设计软件 |
| 上下文组装、embed、检索 | chatllm | — |
| 能力选择 | chatllm 输出建议 | 不执行、不校验权限 |
| 插件执行 | 宿主编排，插件实现 | chatllm 不 dlopen 业务插件 |
| 资产存储、展示 | 宿主 / 资产库 | chatllm 不存二进制、不生成 URL |
| 消息持久化 | chatllm | — |

## 3. 输入：ChatReq

保留现有字段：

```text
prompt: string                    必填
uuid?: string                     续聊
name?: string
keyword?: string
max_tokens?: number               继续只做回显；预留为输出预算
use_skills?: boolean
skill_categories?: string[]
skill_ids?: string[]
system_prompt?: string
refresh_system_prompt?: boolean
plugin_uuid?: string
dialogue_id?: string
show_status?: number
```

新增可选字段：

```text
attachments?: Attachment[]
app_context?: AppContext
tool_results?: ToolResult[]
budget?: Budget
```

## 4. 输出：ChatResp

保留现有 `data` 字段：

```text
uuid, created, prompt, content, content_html, max_tokens, keyword,
session_id, system_prompt, skills, truncated
```

新增可选字段：

```text
attachments?: Attachment[]
tool_calls?: ToolCall[]
usage?: { prompt_tokens?, completion_tokens?, estimated? }
context_truncated?: { system?, user?, history?, file?, folder? }
dropped_messages?: number
summary_used?: boolean
round?: number
round_limit_reached?: boolean
protocol_error?: boolean
retrieval_failed?: boolean
unsupported?: string[]
request_id?: string
elapsed_ms?: number
stages?: { parse?, context?, retrieval?, model?, persist? }
```

约束：

- 未触发新能力时，只返回原有字段，或新字段为空；
- `content` 仍是主文本字段；附件与工具调用不替代它；
- `context_truncated` 是新增对象；现有布尔 `truncated` 保持“skills 是否被裁剪”的语义，二者不复用。

## 5. 数据结构

```text
Attachment
  kind: "image"|"video"|"file"|"audio"|"folder"
  ref: string                     宿主可解析，chatllm 不读
  mime?: string
  name?: string
  size?: number
  width?: number
  height?: number
  duration?: number               毫秒
  thumb_ref?: string
  caption?: string
  source: "user"|"plugin"|"material"|"model"

AppContext
  app: "coreldraw"|"illustrator"|"photoshop"
  document_ref: string
  document_name?: string
  document_version?: string
  page?: number
  units?: string
  selection?: [{ id, type, name?, text?, bbox? }]
  export_refs?: string[]

ToolResult
  call_id: string
  ok: boolean
  data?: object|string
  error?: { code?: string, message?: string }

ToolCall
  call_id: string
  tool: "execute_plugin"|"read_file"|"list_folder"
  args: object

Budget
  max_context_tokens?: number     默认 8192
  max_output_tokens?: number      默认 max_tokens，否则 1024
  max_tool_rounds?: number        默认 2，最大 3
  keep_recent?: number            可选，最近保留条数
```

## 6. 工具协议

模型只输出两种动作：

```json
{ "action": "tool", "tool": "search_plugins", "args": { "query": "...", "top_k": 5 } }
{ "action": "tool", "tool": "execute_plugin", "args": { "lib_name": "...", "ffi_symbol": "...", "params": {} } }
{ "action": "final", "content": "..." }
```

| 工具 | 执行方 | 是否返回 `tool_calls` |
| --- | --- | --- |
| `execute_plugin` / `read_file` / `list_folder` | 宿主 | 是，结束本次调用 |

> 原 `search_plugins` / `search_materials` / `search_prompts` 检索工具已随向量能力移除。

规则：

- 轮次：内部检索与外部执行共用，默认 2、最大 3；
- `tool_results` 按 `call_id` 匹配，未匹配的忽略并标记；
- 同一 `call_id` 重复回灌只取第一条；
- 工具结果作为 `role=tool` 消息注入，视为不可信数据；
- 协议解析失败按普通文本返回，置 `protocol_error`。

## 7. 错误码

保留现有码值；新增：

| 码 | 含义 |
| --- | --- |
| `400110` | 工具结果非法 |
| `400111` | 轮次超限 |
| `400112` | 工具协议解析失败 |
| `400505` | 输出超过 64 KiB |
| `400506` | 一致性对账失败 |

## 8. 兼容规则

1. 新字段全部可选，旧调用方不传时行为与 1.0.6 一致；
2. 旧消息缺 `attachments` 时按 `[]` 读取；缺 `tool_calls` 时视为无；
3. `messages` 新增 `role="tool"`，旧数据不出现该角色；
4. 错误码只增不改；
5. 不承诺版本号映射，字段以本契约 `v1` 为准；
6. 契约变更需在本文顶部升版并记录原因。

## 9. M0 决策（必须确认）

| 编号 | 结论 | 状态 |
| --- | --- | --- |
| Q1 文件解析归属 | 宿主优先；通用格式宿主或独立 sys 能力；设计格式插件 | 候选 |
| Q2 插件执行通道 | 宿主编排，插件执行；chatllm 只返回 `tool_calls` | 候选 |
| Q3 图片 / 视频产出 | chatllm 不生成；只返回素材或插件产物引用 | 候选 |
| Q7 token 计数 | 预算用字符估算；`usage` 优先上游回填 | 候选 |
| Q8 长文本策略 | 先分块池化（512 / 64），不改表 | 候选 |
| Q9 摘要方式 | 先规则摘要，模型摘要后置 | 候选 |
| Q10 模型协议 | 结构化 JSON（方案 A），不依赖原生 `tools` | 候选 |
| Q11 轮次预算 | 默认 2、最大 3，内外共用 | 候选 |
| Q12 `tool_calls` 落库 | 落库，`chat_detail` 返回 | 候选 |

## 10. 未决项（不阻塞 M1a / M1b）

| 编号 | 问题 | 最晚确认点 |
| --- | --- | --- |
| Q13 视觉输入 | 图片能否直接进推理 | M4 |
| Q14 资产库归属 | 删除 / 去重 / GC | M4 |
| Q15 生成来源 | 图片 / 视频由谁生成 | M4 |
| Q16 附件上限 | 20 个、`ref` ≤ 512 | M4 |
| Q17 通用解析位置 | 宿主 vs 独立 sys | M5a |
| Q18 OCR | 是否支持扫描件 | M5a |
| Q19 插件范围 | cdr 先行，ai / ps 后置 | M5b |
| Q20 文件夹上限 | 200 文件 / 50 MB / 1 层 | M5a |
| Q21 `app_context` 字段 | 最小集是否够用 | M5a |
| Q22 日志落点 | stderr JSON，宿主收集 | M1a |
| Q23 `health` | 是否新增 FFI | M6 |
| Q24 删除语义 | 软删除 + 后台清理 | M6 |
| Q25 时间戳 | 新数据毫秒，旧数据兼容 | M2a |
| Q26 缓冲错误码 | `400505` 与 `-2` 并存 | M1a |

## 11. M0 完成判据

- [ ] §9 的 9 项决策由产品 / 宿主确认；
- [ ] 字段名与默认值不再变更；
- [ ] 宿主确认 `tool_calls` / `tool_results` 的对接方式；
- [ ] `docs/analyze.md` §6 指向本文，不再维护重复字段定义；
- [ ] `README.md` 文档入口包含本文。
