### sessions 数据库

#### 定位

记录 AI 对话中需要持久化的会话，用 UUID 作为主键，保存标题、模型名和系统提示词快照。

#### 表结构

```sql
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT,
    model TEXT,
    system_prompt TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);
```

#### FFI 接口

| Symbol | 方法 | 说明 |
| --- | --- | --- |
| sessions_list | GET | 分页列表 |
| sessions_get | GET | 按 id 获取单条 |
| sessions_post | POST | 创建，id 缺省时自动生成 UUID |
| sessions_update | PUT | 全量更新 title/model/system_prompt |
| sessions_patch | PATCH | 部分更新 title/model/system_prompt |

错误码：400100/400101 入参读取失败，400102 JSON 非法，400103 缺少必填字段，404 记录不存在，400310 数据库/业务失败。

#### 交付物

- [x] 1. `ts/init_sessions.ts` 初始化表结构
- [x] 2. `src/utils/sessions.rs` 通过 `db.rs` 读写
- [x] 3. `src/ffi/sessions.rs` 暴露 get/post/update/patch/list
- [x] 4. 编译并部署到 `~/app/piksel/libary`
- [x] 5. `ts/test_sessions.ts` 功能测试
