// 初始化 sessions 数据库表
const DB_URL = "http://127.0.0.1:44944/database";

await fetch(DB_URL, {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./sessions",
    sql: `CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY NOT NULL,
        title TEXT,
        model TEXT,
        system_prompt TEXT,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
}).then((r) => r.json()).then((d) => console.log("create sessions:", d));

await fetch(DB_URL, {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./sessions",
    sql: "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);",
  }),
}).then((r) => r.json()).then((d) => console.log("index sessions:", d));
