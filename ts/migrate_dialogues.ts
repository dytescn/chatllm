// ts/migrate_dialogues.ts
// 把旧 SQLite 中的对话与消息迁移到 designdb。
//
// 运行前提：
//   1. cyctron 已启动；
//   2. libary 中同时存在 database.dll（读旧库）与 libdesigndb.dll（写新库）；
//   3. 旧库位置：./database/dialogue.db 与 ./database/<uuid>.db。
//
// 用法（在 Piksel 运行目录执行）：
//   deno run -A ts/migrate_dialogues.ts
//
// 说明：
//   - 对话 _id 沿用旧 UUID；消息 _id 重新生成，seq 沿用旧 messages.id；
//   - 已存在的对话默认整体跳过（可重复执行，不会重复写入）；
//   - 仅迁移对话与消息；session / skills / prompts / plugin_embeddings 仍在 SQLite。

const DATABASE_URL = "http://127.0.0.1:44944/database";
const DESIGNDB_URL = "http://127.0.0.1:44944/libdesigndb";
const DB_PATH = "chatllm";

async function oldQuery(path: string, sql: string): Promise<any[]> {
  const res = await fetch(DATABASE_URL, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": "db_query",
    },
    body: JSON.stringify({ path, sql }),
  });
  const data = await res.json() as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    throw new Error(`SQLite 查询失败: ${JSON.stringify(data)}`);
  }
  const rows = data.data;
  if (Array.isArray(rows)) return rows;
  if (Array.isArray(rows?.list)) return rows.list;
  if (Array.isArray(rows?.rows)) return rows.rows;
  return [];
}

async function newCall(symbol: string, body: unknown): Promise<any> {
  const res = await fetch(DESIGNDB_URL, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  const data = await res.json() as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    throw new Error(`designdb ${symbol} 失败: ${JSON.stringify(data)}`);
  }
  return data.data;
}

async function insert(collection: string, doc: Record<string, unknown>) {
  await newCall("db_insert", { path: DB_PATH, collection, doc });
}

async function findOne(collection: string, filter: Record<string, unknown>) {
  const rows = await newCall("db_query", {
    path: DB_PATH,
    collection,
    filter,
    limit: 1,
  });
  return Array.isArray(rows) && rows.length > 0 ? rows[0] : null;
}

async function update(collection: string, filter: Record<string, unknown>, set: Record<string, unknown>) {
  await newCall("db_update", {
    path: DB_PATH,
    collection,
    filter,
    update: { $set: set },
  });
}

/** SQLite 的 DATETIME 文本（UTC）或 Unix 秒 → 毫秒 */
function toMs(value: unknown): number {
  if (typeof value === "number") {
    return value > 1_000_000_000_000 ? value : value * 1000;
  }
  if (typeof value === "string" && value.length > 0) {
    const text = value.includes("T") ? value : value.replace(" ", "T") + "Z";
    const ms = Date.parse(text);
    if (!Number.isNaN(ms)) return ms;
  }
  return Date.now();
}

function parseJson(value: unknown): unknown {
  if (typeof value !== "string" || value.length === 0) return null;
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

interface Stats {
  dialogues: number;
  skipped: number;
  messages: number;
  failed: number;
}

async function main() {
  const stats: Stats = { dialogues: 0, skipped: 0, messages: 0, failed: 0 };

  const dialogues = await oldQuery(
    "dialogue",
    `SELECT id, title, keyword, model_name, system_prompt, model_config,
            is_pinned, is_archived, created_at, updated_at
       FROM dialogue`,
  );
  console.log(`旧对话数：${dialogues.length}`);

  for (const row of dialogues) {
    const id = String(row.id ?? "");
    if (!id) {
      stats.failed += 1;
      continue;
    }

    try {
      const exists = await findOne("dialogues", { _id: id });
      if (exists) {
        stats.skipped += 1;
        console.log(`跳过已存在对话 ${id}`);
        continue;
      }

      let messages: any[] = [];
      try {
        messages = await oldQuery(
          id,
          `SELECT id, role, content, created_at FROM messages ORDER BY id ASC`,
        );
      } catch (e) {
        console.warn(`读取 ${id}.db 消息失败，按空对话迁移: ${e}`);
      }

      const lastSeq = messages.reduce(
        (max, m) => Math.max(max, Number(m.id ?? 0)),
        0,
      );
      const modelConfig = parseJson(row.model_config);

      await insert("dialogues", {
        _id: id,
        title: String(row.title ?? ""),
        keyword: String(row.keyword ?? ""),
        search_text: `${row.title ?? ""} ${row.keyword ?? ""}`
          .trim()
          .toLowerCase(),
        model_name: String(row.model_name ?? ""),
        system_prompt: row.system_prompt ?? null,
        model_config: modelConfig && typeof modelConfig === "object" ? modelConfig : {},
        ...(modelConfig === null && row.model_config
          ? { model_config_raw: String(row.model_config) }
          : {}),
        is_pinned: Number(row.is_pinned ?? 0),
        is_archived: Number(row.is_archived ?? 0),
        message_count: messages.length,
        last_seq: lastSeq,
        created_at: toMs(row.created_at),
        updated_at: toMs(row.updated_at),
      });
      stats.dialogues += 1;

      for (const m of messages) {
        try {
          await insert("messages", {
            _id: crypto.randomUUID(),
            dialogue_id: id,
            seq: Number(m.id ?? 0),
            role: String(m.role ?? "user"),
            content: String(m.content ?? ""),
            created_at: toMs(m.created_at),
          });
          stats.messages += 1;
        } catch (e) {
          stats.failed += 1;
          console.error(`消息迁移失败 ${id}#${m.id}: ${e}`);
        }
      }

      if (messages.length !== stats.messages) {
        // 保持计数与消息一致（部分失败时）
        await update(
          "dialogues",
          { _id: id },
          {
            message_count: messages.length,
            last_seq: lastSeq,
            updated_at: Date.now(),
          },
        );
      }
    } catch (e) {
      stats.failed += 1;
      console.error(`对话迁移失败 ${id}: ${e}`);
    }
  }

  await newCall("db_insert", {
    path: DB_PATH,
    collection: "meta",
    doc: {
      _id: "schema",
      version: 2,
      engine: "designdb",
      migrated_from: "sqlite",
      migrated_at: Date.now(),
      updated_at: Date.now(),
    },
  }).catch(() => {
    // 已存在时复用 update
    return update(
      "meta",
      { _id: "schema" },
      { migrated_from: "sqlite", migrated_at: Date.now(), updated_at: Date.now() },
    );
  });

  console.log(
    `迁移完成：对话 ${stats.dialogues}，跳过 ${stats.skipped}，消息 ${stats.messages}，失败 ${stats.failed}`,
  );
  if (stats.failed > 0) {
    Deno.exitCode = 1;
  }
}

await main();
