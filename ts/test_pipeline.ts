// ts/test_pipeline.ts
// 测试 AI 供应商配置与完整流程：
//   1. 供应商已选但 AppKey 未配置 → assistant 返回设置提示 + /setting
//   2. 配置 AppKey 后执行 check → think → plugins → reply
//
// 前置：
//   1. testenv 已启动：cd testenv && ./cyctron.exe start
//   2. /chatllm、/kv_lib、/database 可调用
//   3. testenv 已部署 chatllm.dll、reply_sys_info
//
// 运行：
//   deno run --allow-net ts/test_pipeline.ts
//
// 说明：
// - 临时设置 ai_supplier=deepseek，并先删除 deepseek_app_key；
// - 第二阶段写入测试用 deepseek_app_key，并使用 reply_sys_info 作为
//   chat_plugin，避免依赖真实供应商网络；
// - 临时改 think-greeting 的 value，测试后恢复；
// - 结束后恢复 ai_supplier、deepseek_app_key、think-greeting，并归档测试对话。

const INFO_DB = "./info";
const SUPPLIER_KEY = "ai_supplier";
const TOKEN_KEY = "deepseek_app_key";
const THINK_KEY = "think-greeting";
const PLUGIN_KEY = "chat-plugin-do-test-chat";
const PLUGIN_COLLECTION = "chat_plugin";
const THINK_COLLECTION = "chat_think";
const THINK_TEXT = "你好 问候 介绍 你是谁 在吗";

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

import { post } from "./ffi.ts";

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

async function dbUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: INFO_DB, sql });
}

async function dbQuery(sql: string): Promise<Json> {
  return await post("database", "db_query", { path: INFO_DB, sql });
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

async function main() {
  const stamp = Date.now().toString(36);
  const prompt = "你好";
  let uuid = "";

  const originalSupplier = await readSetting(SUPPLIER_KEY);
  const originalToken = await readSetting(TOKEN_KEY);
  const originalThink = await kvGet(THINK_COLLECTION, [THINK_KEY]);
  console.log(
    `原 ${SUPPLIER_KEY} = ${originalSupplier === null ? "<未设置>" : JSON.stringify(originalSupplier)}`,
  );

  try {
    section("1. 供应商已选但 AppKey 未配置");
    await setSetting(SUPPLIER_KEY, "deepseek");
    await deleteSetting(TOKEN_KEY);
    assert((await readSetting(TOKEN_KEY)) === null, "deepseek_app_key 已删除");

    const blocked = await post("chatllm", "chat", {
      prompt,
      title: `test_pipeline-${stamp}`,
    });
    console.log(blocked);
    assert(blocked.code === 200, "chat 返回 200");
    assert(blocked.data?.route === "/setting", "route = /setting");
    uuid = String(blocked.data?.uuid ?? "");
    assert(uuid.length === 36, "返回合法 uuid");

    const blockedDetail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    const blockedReplies = assistantContents(blockedDetail);
    assert(blockedReplies.length === 1, "写入 1 条 assistant 消息");
    assert(
      (blockedReplies[0] ?? "").includes("请选择设置AI供应商"),
      "assistant 提示配置供应商 / AppKey",
    );
    assert(
      (blockedReplies[0] ?? "").includes("/setting"),
      "assistant 包含 /setting",
    );

    section("2. 配置 AppKey 后执行 check → think → plugins → reply");
    await setSetting(TOKEN_KEY, "test-token");
    assert((await readSetting(TOKEN_KEY)) === "test-token", "deepseek_app_key 已配置");

    await kvPut(THINK_COLLECTION, {
      key: THINK_KEY,
      value: "不应使用的片段|测试查询词",
      value_type: "text",
      text: originalThink?.text ?? THINK_TEXT,
      rank: originalThink?.rank ?? 1,
      meta: originalThink?.meta ?? { category: "chat_think" },
    });
    assert(true, "think-greeting 已改为“前缀|测试查询词”");

    await ensurePluginCollection();
    await kvPut(PLUGIN_COLLECTION, {
      key: PLUGIN_KEY,
      value: JSON.stringify({
        name: "reply_sys_info",
        lib_name: "reply_sys_info",
        ffi_symbol: "chat",
        category: "chat",
      }),
      value_type: "json",
      text: "测试查询词",
      rank: 1,
      meta: { plugin: "reply_sys_info", category: "chat" },
    });
    assert(true, "chat_plugin 已写入 reply_sys_info");

    const continued = await post("chatllm", "chat", { prompt, uuid });
    console.log(continued);
    assert(continued.code === 200, "续聊返回 200");
    assert(continued.data?.created === false, "续聊 created = false");
    assert(continued.data?.route === null, "已配置，route = null");

    const detail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    const replies = assistantContents(detail);
    assert(replies.length === 2, "累计写入 2 条 assistant 消息");
    const reply = replies[replies.length - 1] ?? "";
    console.log(`\n--- assistant ---\n${reply}\n`);
    assert(reply.length > 0, "assistant 内容非空");
    assert(!reply.includes("请选择设置AI供应商"), "没有走配置缺失分支");
    assert(reply.includes("我收到你的信息了"), "plugins 结果已交给 reply 并返回");
  } finally {
    section("3. 清理：恢复配置并归档测试对话");

    try {
      await restoreSetting(TOKEN_KEY, originalToken);
      await restoreSetting(SUPPLIER_KEY, originalSupplier);
      assert((await readSetting(SUPPLIER_KEY)) === originalSupplier, "ai_supplier 已恢复");
      assert((await readSetting(TOKEN_KEY)) === originalToken, "deepseek_app_key 已恢复");
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
          // kv_record get 不返回 text，使用该记录的原始向量文本恢复。
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

    if (uuid) {
      const archived = await post("chatllm", "dialogue_update", {
        id: uuid,
        is_archived: 1,
      });
      assert(archived.code === 200, "测试对话已归档");
    }
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
