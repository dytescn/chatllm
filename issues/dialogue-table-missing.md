# [P1] 初始化对话时 `dialogue` 表未创建

- 来源：GitHub issue [#12](https://github.com/dytescn/chatllm/issues/12)
- 报告人：`funxsayhi`
- 状态：open（2026-11-22 创建；截至本次同步 21 条评论，最后更新 10:56 UTC）
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

## 线上讨论（2026-11-22 同步）

报告人在 10:55–10:56 UTC 连续贴出 `dytes update` 的运行日志（GitHub 上按行拆成了 21 条评论），按时间拼接如下：

```text
❯ dytes update
初始化更新程序...
进度: 0.0% - 读取配置文件...
应用信息:
  名称: train_chat
  版本: 0.1.0
  描述: train_chat package
  作者: funxdata
依赖统计:
  核心依赖 (应用): 1 个
  系统依赖 (库): 1 个
  Web 依赖: 0 个
  设计插件: 0 个
  设计软件: 0 个
  模型: 0 个
锁文件状态: 7 个包已安装
进度: 5.0% - 检查应用自身更新...
开始检查应用更新: train_chat
  锁文件中的版本: None
```

要点：

- 站点应用为 `train_chat 0.1.0`，锁文件中已有 7 个包；
- 日志止于应用自身更新检查，没有出现 chatllm 的下载记录，也没有出现 sys 包的 `[setup x/y]` / `init 脚本执行成功`；
- 因此这次 `dytes update` 不能证明 `setup.json` 被重新执行，`dialogue` 表仍不会创建；
- 需要确认锁文件中 chatllm 的版本，以及执行更新的 dytes 版本（`dytes --version`）。

## 待确认

- 复现环境与 `chatllm` / `database` 的实际版本；
- 锁文件中 chatllm 的版本（若已记录为 0.2.2，`update_sys` 会判定“已是最新版本”并跳过下载与 init）；
- 安装目录下 `database/dialogue.db` 是否存在、schema 内容；
- 安装日志中 `setup.json` 的 `init` 执行结果。
