// ==================== 删除 prompts 表 ====================
const dropRes = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./design_plugins",
    sql: `DROP TABLE IF EXISTS prompts;`,
  }),
});
console.log("drop:", await dropRes.json());

// ==================== 创建 prompts 表 ====================
const createRes = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./design_plugins",
    sql: `CREATE TABLE prompts (
      id TEXT PRIMARY KEY NOT NULL,
      uuid TEXT NOT NULL,
      embedding BLOB NOT NULL
    );`,
  }),
});
console.log("create:", await createRes.json());

// ==================== 创建 uuid 索引 ====================
const indexRes = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./design_plugins",
    sql: `CREATE INDEX IF NOT EXISTS idx_prompts_uuid ON prompts(uuid);`,
  }),
});
console.log("index:", await indexRes.json());
