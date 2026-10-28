// 初始化 skills 数据库表
const DB_URL = "http://127.0.0.1:44944/database";

await fetch(DB_URL, {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./skills",
    sql: `CREATE TABLE IF NOT EXISTS skills (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uid TEXT NOT NULL UNIQUE,
        content TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
}).then((r) => r.json()).then((d) => console.log("create skills:", d));

await fetch(DB_URL, {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./skills",
    sql: "CREATE INDEX IF NOT EXISTS idx_skills_uid ON skills(uid);",
  }),
}).then((r) => r.json()).then((d) => console.log("index skills:", d));
