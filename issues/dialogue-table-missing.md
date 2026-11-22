# [P1] 初始化对话时 `dialogue` 表未创建

- 来源：GitHub issue [#12](https://github.com/dytescn/chatllm/issues/12)
- 报告人：`funxsayhi`
- 状态：open（2026-11-22 创建，暂无评论）
- 包：`chatllm`（`type: sys`）
- 发现日期：2026-11-22

## 现象

首次初始化对话时，`dialogue` 查询返回 `no such table: dialogue`；列表与详情接口分别以 `400302`、`400303` 失败。CLI 随后仍打印“对话已就绪”的提示。

报告日志（节选）：

```text
[dialogue] list: page=Some(1), page_size=Some(1)
[db] 加载 ./libary/database.so 成功
[db] call_ffi: db="dialogue", request="{\"path\":\"dialogue\",\"sql\":\"SELECT count(*) AS n FROM dialogue WHERE is_archived = 0 AND show_status != 2\"}"
[db] call_ffi 响应 code=500101
[cjson] write_error: code=400302, msg=list dialogue failed: database 错误 (code=500101): no such table: dialogue

[dialogue] get: id=00000000-0000-0000-0000-000000000000
[db] call_ffi: db="dialogue", request="{\"path\":\"dialogue\",\"sql\":\"SELECT * FROM dialogue WHERE id = '00000000-0000-0000-0000-000000000000' LIMIT 1\"}"
[db] call_ffi 响应 code=500101
[cjson] write_error: code=400303, msg=load dialogue failed: database 错误 (code=500101): no such table: dialogue
```

原文中 `[db]` / `[designdb]` / `[markparse]` 行属于底层库加载日志。

## 影响

- 对话列表与对话详情不可用，初始化流程实际中断；
- 依赖 `dialogue` 表的写消息、分页、归档等操作均会失败。

## 初步排查

- `dialogue` 表由 `setup.json` 的 `init` → `database.db_update` 创建（`CREATE TABLE IF NOT EXISTS dialogue ...`）；
- `src/utils/dialogue.rs` 注释明确：表结构以 `docs/db.md` 为准，由 `setup.json` 创建，代码不再建表或补列；
- 因此更可能是安装/初始化阶段 `setup.json` 未被执行或执行失败：
  - 安装后是否生成了 `database/dialogue.db`；
  - 宿主是否执行了 `setup.json` 的 `init`，失败是否被静默忽略；
  - `database` 依赖是否按 `dytes.json` 的版本正确安装。
- `setup.json` 中补列语句 `ALTER TABLE dialogue ADD COLUMN system_prompt TEXT` 带有 `allow_error`，建表失败时该步也不会报错。

## 待确认

- 复现环境与 `chatllm` / `database` 的实际版本；
- 安装目录下 `database/dialogue.db` 是否存在、schema 内容；
- 安装日志中 `setup.json` 的 `init` 执行结果。
