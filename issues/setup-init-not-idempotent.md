# [P1] init 脚本非幂等：重复执行报 `duplicate column name: system_prompt` 被 `allow_error` 掩盖

- 来源：GitHub issue [#13](https://github.com/dytescn/chatllm/issues/13)
- 报告人：`funxsayhi`
- 状态：open（2026-11-27 创建；截至 2026-11-27 同步 1 条评论，最后更新 2026-11-27 07:00 UTC）
- 包：`chatllm`（`type: sys`）
- 版本：`0.2.7`（dytes 0.2.26；复测使用 dytes 0.2.27）
- 修复版本：`0.2.8`（本地已修复，待发布后复测）
- 发现日期：2026-11-27

## 现象

chatllm 0.2.7 的 init 脚本在已有数据库上执行时出现重复列错误，被 `allow_error` 忽略后仍判定成功：

```text
  执行包: /media/funx/dytes/libary/chatllm
  读取到 2 个 FFI 请求
  [setup 1/2]  -> database:db_update
  [setup 2/2]  -> database:db_update
  忽略业务错误（allow_error）: database:db_update code 500101 - duplicate column name: system_prompt
  init 脚本执行成功（JSON）
```

## 复测（2026-11-27，dytes 0.2.27）

重新下载 0.2.7 后，`libary/chatllm/setup.json` 仍同时包含：

```sql
-- 建表语句里已经有该列
CREATE TABLE IF NOT EXISTS dialogue (..., system_prompt TEXT, ...);

-- 随后又尝试添加同一列
ALTER TABLE dialogue ADD COLUMN system_prompt TEXT;
```

在已初始化的数据库上重复执行 init 仍报 `duplicate column name: system_prompt`，依旧依赖 `allow_error` 掩盖，结论为未修复。

## 代码位置

- `setup.json` 的第二个请求：`ALTER TABLE dialogue ADD COLUMN system_prompt TEXT;`，带 `"allow_error": true`；
- 第一个请求的 `CREATE TABLE IF NOT EXISTS dialogue` 已包含 `system_prompt TEXT`；
- `src/utils/dialogue.rs::ensure_schema` 在运行时也执行同样的建表 + 补列，补列失败仅打印 `[dialogue] ensure_schema: 跳过补列`。

## 影响

- init 每次都报错但被吞掉，输出“执行成功”，无法区分“已是最新”和“迁移真的失败”；
- 真正的 SQL 错误（例如表结构损坏、磁盘只读）同样会被 `allow_error` 掩盖；
- 后续若移除 `allow_error` 或宿主调整错误处理，重复执行 init 将直接失败。

## 建议

1. 补列改为幂等写法：执行前用 `PRAGMA table_info(dialogue)` 判断列是否存在，或由 `database` 库提供可重复执行的迁移语句；
2. 建表语句与补列语句去重（新库建表已含列，旧库才需要补列），不要无条件执行两条；
3. 发布前在“已有数据的数据库”上连续执行 init 两次，确认无业务错误；
4. 不建议用 `allow_error` 掩盖初始化失败；至少应汇总输出被忽略的错误，便于排查。

## 待确认

- 需要兼容的最早库版本是否缺少 `system_prompt` 列（运行时已按列存在性补列，不再阻塞）。

## 修复（0.2.8）

- `setup.json` 删除冗余的 `ALTER TABLE dialogue ADD COLUMN system_prompt TEXT` 请求及
  `allow_error`，init 只保留 `CREATE TABLE IF NOT EXISTS`，可重复执行；
- `src/utils/dialogue.rs::ensure_schema` 建表后用
  `SELECT COUNT(*) FROM pragma_table_info('dialogue') WHERE name = 'system_prompt'`
  判断列是否存在，缺列才补；查询或补列失败会向上返回，不再吞错；
- `tests/common/mod.rs` 去掉同样的无条件补列；
- `docs/db/dialogue.md`、`skills/chatllm-setup/SKILL.md` 同步更新规范与示例。

验证结果（本地 `.dytes/env`）：

- 缺 `system_prompt` 的旧库首次调用自动补列成功；
- 同一库第二次调用只做存在性查询，不再执行 `ALTER`；
- 连续执行 `setup.json` 两次均成功，无 `duplicate column` 错误；
- `base_dialogue`、`base_get_dialogue`、`utils_dialogue` 测试通过。

待发布 0.2.8 后由报告人用 `dytes update` 复测确认。
