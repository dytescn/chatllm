// setup.ts —— 初始化 chatllm 数据库，表结构严格对应 docs/db.md

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS dialogue (
        id TEXT PRIMARY KEY NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        token_count INTEGER NOT NULL DEFAULT 0,
        status INTEGER NOT NULL DEFAULT 0,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        is_archived INTEGER NOT NULL DEFAULT 0,
        show_status INTEGER NOT NULL DEFAULT 1,
        system_prompt TEXT,
        message_count INTEGER NOT NULL DEFAULT 0,
        last_seq INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

console.log("dialogue 表初始化完成");

// 旧库迁移：补充 system_prompt 列（已存在时忽略）。
const migrated = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: "ALTER TABLE dialogue ADD COLUMN system_prompt TEXT;",
  }),
});
const result = await migrated.json();
if (result?.code !== 200 && !String(result?.msg ?? "").includes("duplicate column")) {
  console.warn("system_prompt 迁移跳过：", JSON.stringify(result));
}
