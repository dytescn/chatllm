# chatllm

`chatllm` 是 Rust 编写的对话存储与编排模块，编译为动态库，导出 C ABI
`post_*` 函数。`./libary`、`./database`、`./designdb`、`./plugins` 均相对
运行时 cwd 解析。

## 功能

- 新建 / 续聊对话，保存 user 与 assistant 消息；
- 对话列表、搜索、更新、详情与游标分页；
- `think → chat_plugins → reply` 对话编排；
- chat_plugins 按 `plugin_uuid` 读取 taskflow，执行插件回调。

`check` 已移除，后续以插件形式提供。

## 对话流程

```text
chat
  ├─ think：用默认分类关键词 base 检索 chat_think（100% 匹配），命中插件并回调，输出 prompts[] / outputs[]
  ├─ chat_plugins：按 think 的 prompts[] 逐条检索 chat_plugins（只取 Top1）、执行 taskflow 回调，
  │    逐条记录起止时间，只输出 outputs[]
  └─ reply：先按默认分类 base 检索 chat_reply，未命中再按合并结果检索；
       命中插件并回调；未命中原样输出合并结果
```

每个阶段的方法、输入、输出写入当前对话的轮次记录，格式见
[`docs/db.md`](docs/db.md)。

## FFI

统一返回 `{code, data, msg}`，成功 `code = 200`；结果写入调用方提供的
64 KiB 缓冲区（`kv_search` 为 1 MiB）。

| 导出函数 | 说明 |
| --- | --- |
| `post_chat` | 新建 / 续聊，执行对话流程 |
| `post_dialogue_list` | 对话分页列表 |
| `post_dialogue_update` | 更新对话字段 |
| `post_dialogue_search` | 按标题搜索 |
| `post_chat_detail` | 读取对话与消息 |

接口细节见 [`docs/ffi.md`](docs/ffi.md)。

## 存储

| 数据 | 位置 |
| --- | --- |
| 对话索引 | `database/dialogue.db` → `dialogue` |
| 对话消息 | `designdb/<dialogue_id>` → `messages` |

## 依赖

| 依赖 | 用途 |
| --- | --- |
| `database` | SQLite |
| `libdesigndb` | 消息库与 taskflow |
| `kv_lib` | `chat_think` / `chat_plugins` / `chat_reply` 检索（0.1.5） |
| `plugin_lib` | 插件能力清单（`utils/plugins_lib.rs` 封装） |

声明见 `dytes.json`；初始化文件为 `setup.json`。

## 构建

```bash
cargo build --release
```

产物为 `target/release/chatllm.dll`（Linux `libchatllm.so`、macOS
`libchatllm.dylib`）。

## 文档

- [`docs/ffi.md`](docs/ffi.md)：FFI 接口与错误码
- [`docs/db.md`](docs/db.md)：对话表、消息与轮次记录
- [`docs/kv_lib.md`](docs/kv_lib.md)：kv_lib 调用方式
- [`skills/chatllm-runflow/SKILL.md`](skills/chatllm-runflow/SKILL.md)：完整对话流程
- [`skills/chatllm-code-standards/SKILL.md`](skills/chatllm-code-standards/SKILL.md)：代码组织规范
- [`todo/TODO.md`](todo/TODO.md)：路线图与待办

## 已知限制

- 外部库路径固定为 `./libary/*`；
- 输出缓冲区 64 KiB（`kv_search` 为 1 MiB）；
- 插件与系统库加载后常驻；
- 同一线程禁止插件重入；
- `content_html` 暂为空字符串。
