# think_base / reply_base 流程

`think_base` 与 `reply_base` 是只包含 `skills/`、`prompts/` 的资源包，不导出
FFI。chatllm 通过资源包 uuid 定位系统对话，并把 skills 写入
`dialogue.system_prompt`。

## 资源包

```text
think_base/                 reply_base/
  dytes.json                  dytes.json
  skills/think_base.md        skills/reply_base.md
  prompts/think_base.json     prompts/reply_base.json
```

| 包 | uuid | 用途 |
| --- | --- | --- |
| `think_base` | `dytes.json.uuid` | think 阶段的系统提示词与 prompts |
| `reply_base` | `dytes.json.uuid` | reply 阶段的系统提示词与 prompts |

安装后文件位于：

```text
plugins/<package>/<package>.md
plugins/<package>/<package>.json
```

## 数据

`chat_think` 命中记录需要携带资源包信息：

```json
{
  "key": "think-base",
  "value": "问候|系统信息",
  "text": "你好 问候 系统信息 环境 版本 配置 插件",
  "rank": 1,
  "meta": {
    "category": "chat_think",
    "plugin_uuid": "<think_base uuid>",
    "plugin_name": "think_base"
  }
}
```

| 字段 | 用途 |
| --- | --- |
| `meta.plugin_uuid` | 对话 id / `plugin_uuid` |
| `meta.plugin_name` | 定位 `plugins/<name>/<name>.md` |
| `value` | 按 `|` 拆分为 plugins 查询数组 |

`reply_base` 只在 reply 回退时直接读取安装目录；正常 reply 会先检索
`chat_reply`，命中插件后走 cloud 或业务插件。

## 流程

```text
chat
  │
  ├─ think：检索 chat_think，命中 think_base
  │    ├─ 读取 meta.plugin_uuid / plugin_name
  │    ├─ 按 plugin_uuid 查对话
  │    │    ├─ 不存在：创建对话，show_status=2，写入 system_prompt
  │    │    └─ 已存在：复用
  │    └─ 将 value 按 "|" 拆分
  │
  ├─ plugins：每段检索 chat_plugin 并执行
  │
  └─ reply：合并结果，检索 chat_reply
       ├─ 命中插件：按 plugins_uid 查 / 建对话，调用 cloud 或业务插件
       ├─ 未命中 / 失败：回退 reply_base 清洗
       └─ assistant = 最终内容
```

## 对话字段

`dialogue` 表包含：

| 列 | 类型 | 说明 |
| --- | --- | --- |
| `system_prompt` | `TEXT` | 资源包 skills 内容 |

`show_status = 2` 的系统对话不会出现在普通对话列表中。

## 验证

```bash
cargo build --release
cp target/release/chatllm.dll testenv/libary/chatllm.dll
```

在仓库根目录运行脚本：

```bash
deno run --allow-net --allow-read ts/test_think_base.ts
deno run --allow-net --allow-read ts/test_full_flow.ts
```

cyctron 在另一个终端启动：

```bash
cd testenv
./cyctron.exe start
```

测试脚本会临时写入所需的 `chat_think` / `chat_plugin` 数据，结束后恢复。

预期：

- 首次请求创建 think_base 系统对话，`system_prompt` 等于 `think_base.md`；
- `问候|系统信息` 拆成两段，分别调用 `do_test_chat`；
- 插件结果以空行合并，交给 reply；
- reply 检索 `chat_reply`，命中插件调用 cloud；未命中回退 reply_base；
- assistant 为最终内容。

## 边界

- 当前 `system_prompt` 只写入 `dialogue`，尚未传给供应商请求；
- think_base 的 `chat_think` 记录由安装 / 测试脚本写入；reply_base 只在
  回退时直接读取安装目录，正常 reply 查询 `chat_reply`；
- think / reply 未命中资源包时跳过对应阶段。
