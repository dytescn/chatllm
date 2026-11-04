// ts/test_live.ts
// 使用本地 token 文件测试真实供应商返回：
//   deepseek      → libdeepseek.dll
//   siliconflow_t2t → libsiliconflow_t2t.dll
//
// token 文件默认目录：./.tokens
// 可用 FUNXTOKEN_DIR 覆盖。
//
// 前置：
//   1. testenv 已启动：cd testenv && ./cyctron.exe start
//   2. testenv 已部署 chatllm.dll、libdeepseek、libsiliconflow_t2t
//   3. funxtoken/deepseek.md、funxtoken/silicon.md 存在
//
// 运行：
//   deno run --allow-net --allow-read --allow-env ts/test_live.ts
//
// 说明：
// - 脚本读取 token 后写入 testenv 的 info.db，不会打印 token；
// - 使用 chat_plugin 指向供应商，走 cloud::chat_with 统一接口；
// - 测试结束后恢复 ai_supplier、两个 AppKey、think-greeting、chat_plugin，
//   并归档测试对话。

const BASE = "http://127.0.0.1:44944";
const INFO_DB = "./info";
const TOKEN_DIR = Deno.env.get("FUNXTOKEN_DIR") ?? "./.tokens";

const SUPPLIER_KEY = "ai_supplier";
const DEEPSEEK_TOKEN = "deepseek_app_key";
const SILICON_TOKEN = "silicon_app_key";
const THINK_KEY = "think-greeting";
const THINK_COLLECTION = "chat_think";
const THINK_TEXT = "你好 问候 介绍 你是谁 在吗";

const PLUGIN_KEY = "chat-plugin-do-test-chat";
const PLUGIN_COLLECTION = "chat_plugin";

type Json = Record<string, any>;

let failed = 0;

function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

function assert(cond: boolean, message: string) {
  if (cond) {
    console.log(`✅ ${message}`);
  } else {
    console.error(`❌ ${message}`);
    failed += 1;
  }
}

async function post(api: string, symbol: string, body: Json = {}): Promise<Json> {
  const response = await fetch(`${BASE}/${api}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`${api}/${symbol} 返回非 JSON（HTTP ${response.status}）`);
  }
}

function rows(payload: Json): Json[] {
  const data = payload?.data;
  if (Array.isArray(data)) return data;
  if (Array.isArray(data?.list)) return data.list;
  if (Array.isArray(data?.rows)) return data.rows;
  return [];
}

function esc(value: string): string {
  return value.replace(/'/g, "''");
}

async function dbQuery(sql: string): Promise<Json> {
  return await post("database", "db_query", { path: INFO_DB, sql });
}

async function dbUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: INFO_DB, sql });
}

async function ensureSettingTable(): Promise<void> {
  const result = await dbUpdate(
    "CREATE TABLE IF NOT EXISTS setting (key TEXT NOT NULL UNIQUE, value TEXT)",
  );
  if (result.code !== 200) {
    throw new Error(`创建 setting 表失败：${JSON.stringify(result)}`);
  }
}

async function readSetting(key: string): Promise<string | null> {
  const result = await dbQuery(
    `SELECT value FROM setting WHERE key = '${esc(key)}' LIMIT 1`,
  );
  if (result.code !== 200) return null;
  const value = rows(result)[0]?.value;
  return typeof value === "string" ? value : null;
}

async function setSetting(key: string, value: string): Promise<void> {
  await ensureSettingTable();
  const result = await dbUpdate(
    `INSERT INTO setting (key, value) VALUES ('${esc(key)}', '${esc(value)}')
     ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
  );
  if (result.code !== 200) {
    throw new Error(`写入 ${key} 失败：${JSON.stringify(result)}`);
  }
}

async function deleteSetting(key: string): Promise<void> {
  await dbUpdate(`DELETE FROM setting WHERE key = '${esc(key)}'`);
}

async function restoreSetting(key: string, original: string | null): Promise<void> {
  if (original === null) {
    await deleteSetting(key);
  } else {
    await setSetting(key, original);
  }
}

async function kvGet(collection: string, keys: string[]): Promise<Json | null> {
  const result = await post("kv_lib", "kv_record", {
    action: "get",
    collection,
    keys,
    with_vector: false,
  });
  if (result.code !== 200) return null;
  return rows(result)[0] ?? null;
}

async function kvPut(collection: string, item: Json): Promise<void> {
  const result = await post("kv_lib", "kv_record", {
    action: "put",
    collection,
    items: [item],
  });
  if (result.code !== 200) {
    throw new Error(`写入 ${collection}/${item.key} 失败：${JSON.stringify(result)}`);
  }
}

async function ensurePluginCollection(): Promise<void> {
  const result = await post("kv_lib", "kv_collection", {
    action: "put",
    code: PLUGIN_COLLECTION,
    name: "聊天插件",
    description: "chat 分类插件的能力描述向量",
    dim: 1024,
    model: "qwen3-embed",
  });
  if (result.code !== 200) {
    throw new Error(`创建 ${PLUGIN_COLLECTION} 失败：${JSON.stringify(result)}`);
  }
}

function assistantContents(detail: Json): string[] {
  const list = detail?.data?.messages?.list ?? detail?.data?.list ?? [];
  return list
    .filter((message: Json) => message?.role === "assistant")
    .map((message: Json) => String(message?.content ?? ""));
}

/** 读取 token 文件：取第一行冒号后的内容，不打印 token。 */
async function readToken(file: string): Promise<string | null> {
  try {
    const text = await Deno.readTextFile(`${TOKEN_DIR}/${file}`);
    const line = text.split(/\r?\n/).find((item) => item.trim());
    if (!line) return null;
    const ascii = line.indexOf(":");
    const full = line.indexOf("：");
    const index = ascii >= 0 ? ascii : full;
    const value = index >= 0 ? line.slice(index + 1).trim() : line.trim();
    return value || null;
  } catch {
    return null;
  }
}

async function runSupplier(
  supplier: "deepseek" | "siliconflow_t2t",
  tokenKey: string,
  token: string,
  stamp: string,
  uuids: string[],
): Promise<void> {
  section(`供应商 ${supplier}`);

  await setSetting(SUPPLIER_KEY, supplier);
  await setSetting(tokenKey, token);
  assert((await readSetting(SUPPLIER_KEY)) === supplier, `ai_supplier = ${supplier}`);
  assert((await readSetting(tokenKey)) === token, `${tokenKey} 已写入`);

  await kvPut(PLUGIN_COLLECTION, {
    key: PLUGIN_KEY,
    value: JSON.stringify({
      name: supplier,
      lib_name: supplier,
      ffi_symbol: "chat",
      category: "chat",
    }),
    value_type: "json",
    text: "测试查询词",
    rank: 1,
    meta: { plugin: supplier, category: "chat" },
  });

  const created = await post("chatllm", "chat", {
    prompt: "你好",
    title: `test_live-${supplier}-${stamp}`,
  });
  console.log(created);
  assert(created.code === 200, "chat 返回 200");
  assert(created.data?.route === null, "route = null");

  const uuid = String(created.data?.uuid ?? "");
  if (!uuid) return;
  uuids.push(uuid);

  const detail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
  const replies = assistantContents(detail);
  assert(replies.length === 1, "写入 1 条 assistant 消息");
  const reply = replies[0] ?? "";
  console.log(`\n--- ${supplier} assistant ---\n${reply}\n`);

  assert(reply.length > 0, "assistant 内容非空");
  assert(!reply.includes("请选择设置AI供应商"), "没有返回配置缺失提示");
  assert(!reply.includes("插件处理失败"), "插件 / cloud 调用没有失败");
  assert(!reply.includes("AI 配置错误"), "AppKey 已被供应商接受");
}

async function main() {
  const stamp = Date.now().toString(36);
  const uuids: string[] = [];

  const deepseekToken = await readToken("deepseek.md");
  const siliconToken = await readToken("silicon.md");
  if (!deepseekToken || !siliconToken) {
    console.error(
      `未读取到 token：请检查 ${TOKEN_DIR}/deepseek.md 与 silicon.md`,
    );
    Deno.exit(1);
  }
  console.log(`token 已读取：deepseek=${deepseekToken.length} chars, silicon=${siliconToken.length} chars`);

  const originalSupplier = await readSetting(SUPPLIER_KEY);
  const originalDeepseek = await readSetting(DEEPSEEK_TOKEN);
  const originalSilicon = await readSetting(SILICON_TOKEN);
  const originalThink = await kvGet(THINK_COLLECTION, [THINK_KEY]);
  const originalPlugin = await kvGet(PLUGIN_COLLECTION, [PLUGIN_KEY]);

  try {
    section("准备 think 与 chat_plugin");
    await kvPut(THINK_COLLECTION, {
      key: THINK_KEY,
      value: "不应使用的片段|测试查询词",
      value_type: "text",
      text: originalThink?.text ?? THINK_TEXT,
      rank: originalThink?.rank ?? 1,
      meta: originalThink?.meta ?? { category: "chat_think" },
    });
    assert(true, "think-greeting 已指向“测试查询词”");
    await ensurePluginCollection();

    await runSupplier("deepseek", DEEPSEEK_TOKEN, deepseekToken, stamp, uuids);
    await runSupplier(
      "siliconflow_t2t",
      SILICON_TOKEN,
      siliconToken,
      stamp,
      uuids,
    );
  } finally {
    section("清理：恢复配置并归档测试对话");

    try {
      await restoreSetting(DEEPSEEK_TOKEN, originalDeepseek);
      await restoreSetting(SILICON_TOKEN, originalSilicon);
      await restoreSetting(SUPPLIER_KEY, originalSupplier);
      assert((await readSetting(SUPPLIER_KEY)) === originalSupplier, "ai_supplier 已恢复");
      assert((await readSetting(DEEPSEEK_TOKEN)) === originalDeepseek, "deepseek_app_key 已恢复");
      assert((await readSetting(SILICON_TOKEN)) === originalSilicon, "silicon_app_key 已恢复");
    } catch (error) {
      console.error("恢复配置失败：", error);
      failed += 1;
    }

    if (originalThink) {
      try {
        await kvPut(THINK_COLLECTION, {
          key: THINK_KEY,
          value: originalThink.value ?? "",
          value_type: originalThink.value_type ?? "text",
          text: originalThink.text ?? THINK_TEXT,
          rank: originalThink.rank ?? 1,
          meta: originalThink.meta ?? { category: "chat_think" },
        });
        assert(true, "think-greeting 已恢复");
      } catch (error) {
        console.error("恢复 think-greeting 失败：", error);
        failed += 1;
      }
    }

    if (originalPlugin) {
      try {
        await kvPut(PLUGIN_COLLECTION, {
          key: PLUGIN_KEY,
          value: originalPlugin.value ?? "",
          value_type: originalPlugin.value_type ?? "json",
          text: originalPlugin.text ?? "测试查询词",
          rank: originalPlugin.rank ?? 1,
          meta: originalPlugin.meta ?? { category: "chat" },
        });
        assert(true, "chat_plugin 已恢复");
      } catch (error) {
        console.error("恢复 chat_plugin 失败：", error);
        failed += 1;
      }
    }

    for (const uuid of uuids) {
      const archived = await post("chatllm", "dialogue_update", {
        id: uuid,
        is_archived: 1,
      });
      assert(archived.code === 200, `归档测试对话 ${uuid.slice(0, 8)}`);
    }
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
