# testenv 测试环境

> 只记录本地测试环境的初始化与搭建。数据库结构见 `docs/db.md`，
> 接口测试用例见 `docs/test_base_dialogue.md`、`docs/test_base_message.md`。

## 1. 目录与文件

| 路径 | 用途 |
| --- | --- |
| `testenv/cyctron.exe` | 本地宿主，提供 `http://127.0.0.1:44944` 的 HTTP/FFI 路由 |
| `testenv/setup.ts` | 本地建库脚本（创建 `dialogue` 表）；正式安装使用仓库根 `setup.json` |
| `testenv/seed.ts` | 基础数据脚本，插入 4 条固定 UUID 的对话（不在建表脚本中生成） |
| `testenv/clear_db.ts` | 清空 `dialogue` 库：删除所有用户表（含表结构） |
| `testenv/database/dialogue.db` | SQLite 数据库文件（库名 `./dialogue`） |
| `testenv/libary/*.dll` | `database.dll`、`libdesigndb.dll` 等宿主依赖库 |

## 2. 前置条件

- Windows 环境，`testenv/libary/database.dll`、`libdesigndb.dll`、`cloud_lib.dll` 已就位；
- 数据库库名固定为 `./dialogue`，由 `database.dll` 解析为 `./database/dialogue.db`；
- 脚本以 Deno 约定编写（`deno run --allow-net xxx.ts`），也可用 Node 24 直接运行（脚本不含类型注解）。

Rust 侧通过 `dlopen` 直接加载 `./libary/database.dll` 与 `./libary/libdesigndb.dll`，
路径相对于进程工作目录，因此测试进程的 cwd 必须是 `testenv`。
`tests/common/mod.rs::db_ready()` 会在首次调用时把 cwd 切到 `testenv`，所以**在仓库根目录执行 `cargo test` 即可**。

缺少 `testenv/libary/database.dll` 或 `testenv/database/dialogue.db` 时，依赖数据库的测试会自动跳过并打印原因。

## 3. 初始化流程

```bash
# 1. 启动宿主（HTTP 44944）
cd testenv && ./cyctron.exe start

# 2. 清空数据库（可选，会删除所有用户表）
deno run --allow-net clear_db.ts

# 3. 按 docs/db.md 建表
deno run --allow-net setup.ts

# 4. 插入基础数据
deno run --allow-net seed.ts
```

> 实测时也可用 Node 直接执行：`node clear_db.ts && node setup.ts && node seed.ts`。

`cyctron.exe` 只用于通过 HTTP 执行上述脚本的 SQL；Rust 测试直接加载 DLL，不依赖宿主进程。

## 4. 基础数据（seed.ts）

| UUID | 标题 | `is_pinned` | `is_archived` | `show_status` | `message_count` / `last_seq` |
| --- | --- | --- | --- | --- | --- |
| `11111111-1111-4111-8111-111111111111` | 产品需求讨论 | 1 | 0 | 1 | 2 / 2 |
| `22222222-2222-4222-8222-222222222222` | 接口联调记录 | 0 | 0 | 1 | 1 / 1 |
| `33333333-3333-4333-8333-333333333333` | 插件对话：不显示 | 0 | 0 | 2 | 0 / 0 |
| `44444444-4444-4444-8444-444444444444` | 已归档对话 | 0 | 1 | 1 | 0 / 0 |

脚本使用 `INSERT OR REPLACE`，重复执行结果幂等。
`show_status = 2` 与 `is_archived = 1` 两条用于验证列表过滤。

## 5. 注意事项

- `clear_db.ts` 会删除库中**所有用户表**（包括与 chatllm 无关的 `prompts`、`embedding_*`），执行前确认该库可重置。
- 当前 `dialogue` 表有 12 个列；Rust 侧不再自动建表或补列，正式初始化用 `setup.json`，本地 testenv 可用 `setup.ts`。
- 每次重置环境后需重新执行 `setup.ts` 与 `seed.ts`，否则依赖基础数据的测试会失败。
