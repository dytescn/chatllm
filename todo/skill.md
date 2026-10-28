### skills 数据库

#### 定位

存储 skill 的文本内容，每条记录有全局唯一的 `uid` 和递增的 `version`，用于按 id/uid 读取、创建、更新和分页列表。

#### 表结构（已修正原稿中自引用外键的错误）

```sql
CREATE TABLE IF NOT EXISTS skills (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_skills_uid ON skills(uid);
```

#### FFI 接口

| Symbol | 方法 | 说明 |
| --- | --- | --- |
| skills_list | GET | 分页列表 |
| skills_get | GET | 按 id 或 uid 获取单条 |
| skills_post | POST | 创建，uid 缺省时自动生成 |
| skills_update | PUT | 全量更新 content（version 可选） |
| skills_patch | PATCH | 部分更新 content / version |

错误码：400100/400101 入参读取失败，400102 JSON 非法，400103 缺少必填字段，400104 缺少定位字段，404 记录不存在，400300 数据库/业务失败。

#### 交付物

- [x] 1. `ts/init_skills.ts` 初始化表结构
- [x] 2. `src/utils/skills.rs` 通过 `db.rs` 读写
- [x] 3. `src/ffi/skills.rs` 暴露 get/post/update/patch/list
- [x] 4. 编译并部署到 `~/app/piksel/libary`
- [x] 5. `ts/test_skills.ts` 功能测试
