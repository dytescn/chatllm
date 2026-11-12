// ts/test_full_flow.ts
// 完整对话流程测试：
//   check → think(chat_think) → think_base uuid → 查询/创建对话
//   → skills 写入 system_prompt → user 消息
//   → think value 按 | 拆分 → 每段查询 chat_plugin → 执行 do_test_chat
//   → 合并插件结果 → reply_sys_info → assistant
//
// 前置：
//   1. testenv 已启动
//   2. testenv/plugins/think_base、do_test_chat 已安装
//   3. chatllm.dll 已更新
//
// 运行：
//   deno run --allow-net --allow-read ts/test_full_flow.ts

const INFO_DB = "./info";
const DIALOGUE_DB = "./dialogue";
const THINK_COLLECTION = "chat_think";
const PLUGIN_COLLECTION = "chat_plugin";
const THINK_KEY = "think-base";
const SUPPLIER_KEY = "ai_supplier";
const TOKEN_KEY = "deepseek_app_key";

type Json = Record<string, any>;
let failed = 0;

function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

function assert(cond: boolean, message: string) {
  if (cond) console.log(`✅ ${message}`);
  else {
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

async function dbQuery(sql: string): Promise<Json> {
  return await post("database", "db_query", { path: INFO_DB, sql });
}

async function dbUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: INFO_DB, sql });
}

async function dialogueUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: DIALOGUE_DB, sql });
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
  const result = await dbUpdate(
    `INSERT INTO setting (key, value) VALUES ('${esc(key)}', '${esc(value)}')
     ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
  );
  if (result.code !== 200) throw new Error(`写入 ${key} 失败`);
}

async function deleteSetting(key: string): Promise<void> {
  await dbUpdate(`DELETE FROM setting WHERE key = '${esc(key)}'`);
}

async function restoreSetting(key: string, value: string | null): Promise<void> {
  if (value === null) await deleteSetting(key);
  else await setSetting(key, value);
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

async function ensureCollection(code: string, name: string): Promise<void> {
  const result = await post("kv_lib", "kv_collection", {
    action: "put",
    code,
    name,
    description: `${code} test collection`,
    dim: 1024,
    model: "qwen3-embed",
  });
  if (result.code !== 200) throw new Error(`创建 ${code} 失败`);
}

function assistantContents(detail: Json): string[] {
  return (detail?.data?.messages?.list ?? [])
    .filter((message: Json) => message?.role === "assistant")
    .map((message: Json) => String(message?.content ?? ""));
}

function userContents(detail: Json): string[] {
  return (detail?.data?.messages?.list ?? [])
    .filter((message: Json) => message?.role === "user")
    .map((message: Json) => String(message?.content ?? ""));
}

async function main() {
  const pkg = "testenv/plugins/think_base";
  const manifest = JSON.parse(await Deno.readTextFile(`${pkg}/dytes.json`));
  const prompts = JSON.parse(await Deno.readTextFile(`${pkg}/think_base.json`));
  const skills = await Deno.readTextFile(`${pkg}/think_base.md`);
  const thinkBaseUuid = String(manifest.uuid);

  const replyPkg = "testenv/plugins/reply_base";
  const replyManifest = JSON.parse(
    await Deno.readTextFile(`${replyPkg}/dytes.json`),
  );
  const replySkills = await Deno.readTextFile(`${replyPkg}/reply_base.md`);
  const replyBaseUuid = String(replyManifest.uuid);
  const promptText = ["think_base", "测试", ...prompts].join(" ");

  const originalSupplier = await readSetting(SUPPLIER_KEY);
  const originalToken = await readSetting(TOKEN_KEY);
  const originalThink = await kvGet(THINK_COLLECTION, [THINK_KEY]);
  let uuid = "";

  try {
    section("1. 准备：供应商、think_base、chat_plugin");
    await setSetting(SUPPLIER_KEY, "deepseek");
    await setSetting(TOKEN_KEY, "test-token");
    await ensureCollection(THINK_COLLECTION, "思考");
    await ensureCollection(PLUGIN_COLLECTION, "聊天插件");

    await kvPut(THINK_COLLECTION, {
      key: THINK_KEY,
      value: "问候|系统信息",
      value_type: "text",
      text: promptText,
      rank: 1,
      meta: {
        category: "chat_think",
        plugin_uuid: thinkBaseUuid,
        plugin_name: "think_base",
      },
    });

    for (const [key, text] of [
      ["chat-plugin-think-greeting", "问候"],
      ["chat-plugin-think-sysinfo", "系统信息"],
    ]) {
      await kvPut(PLUGIN_COLLECTION, {
        key,
        value: JSON.stringify({
          name: "do_test_chat",
          lib_name: "do_test_chat",
          ffi_symbol: "chat",
          category: "chat",
        }),
        value_type: "json",
        text,
        rank: 1,
        meta: { plugin: "do_test_chat", category: "chat" },
      });
    }
    assert(true, "think_base 与 do_test_chat 插件记录已写入");

    // 清理旧对话行，确保本次是新建；消息库由测试环境自行回收。
    await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(thinkBaseUuid)}'`);
    await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(replyBaseUuid)}'`);

    section("2. 完整流程：发送消息");
    const sent = await post("chatllm", "chat", { prompt: "think_base 测试" });
    console.log(sent);
    assert(sent.code === 200, "chat 返回 200");
    assert(sent.data?.uuid === thinkBaseUuid, "uuid = think_base uuid");
    assert(sent.data?.plugin_uuid === thinkBaseUuid, "plugin_uuid = think_base uuid");
    assert(sent.data?.created === true, "created = true");
    assert(sent.data?.route === null, "route = null");
    uuid = String(sent.data?.uuid ?? "");

    const detail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    console.log(detail);
    assert(detail.code === 200, "chat_detail 返回 200");
    assert(
      detail.data?.dialogue?.system_prompt === skills.trim(),
      "system_prompt = think_base skills",
    );
    assert(userContents(detail).includes("think_base 测试"), "user 消息已写入");

    const assistant = assistantContents(detail);
    const last = assistant[assistant.length - 1] ?? "";
    console.log(`\n--- assistant ---\n${last}\n`);
    assert(last.includes("问候"), "包含“问候”插件结果");
    assert(last.includes("系统信息"), "包含“系统信息”插件结果");
    assert(!last.includes("do_chat_test"), "reply_base 已移除 do_chat_test 标记");
    assert(last === "问候\n\n系统信息", "最终内容为 问候 + 空行 + 系统信息");

    section("2.1 reply_base 系统对话");
    const replyDetail = await post("chatllm", "chat_detail", {
      uuid: replyBaseUuid,
      limit: 20,
    });
    console.log(replyDetail);
    assert(replyDetail.code === 200, "reply_base 对话返回 200");
    assert(
      replyDetail.data?.dialogue?.system_prompt === replySkills.trim(),
      "reply_base system_prompt = reply_base skills",
    );
    assert(
      replyDetail.data?.dialogue?.show_status === 2,
      "reply_base 对话 show_status = 2",
    );
    const replyUsers = userContents(replyDetail);
    const replyAssistants = assistantContents(replyDetail);
    const replyInput = replyUsers[replyUsers.length - 1] ?? "";
    const replyOutput = replyAssistants[replyAssistants.length - 1] ?? "";
    assert(replyInput.includes("问候do_chat_test"), "reply_base 收到第一段插件结果");
    assert(replyInput.includes("系统信息do_chat_test"), "reply_base 收到第二段插件结果");
    assert(replyOutput === "问候\n\n系统信息", "reply_base assistant 为清洗结果");

    section("3. 第二条消息：复用对话");
    const again = await post("chatllm", "chat", { prompt: "think_base 继续" });
    assert(again.code === 200, "续聊返回 200");
    assert(again.data?.uuid === thinkBaseUuid, "续聊 uuid 一致");
    assert(again.data?.created === false, "created = false");

    const detail2 = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    assert(
      detail2.data?.dialogue?.system_prompt === skills.trim(),
      "system_prompt 未变化",
    );
    assert(
      userContents(detail2).includes("think_base 继续"),
      "第二条 user 消息已写入",
    );
  } finally {
    section("4. 清理");
    try {
      await restoreSetting(TOKEN_KEY, originalToken);
      await restoreSetting(SUPPLIER_KEY, originalSupplier);
      assert(true, "配置已恢复");
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
          text: originalThink.text ?? "",
          rank: originalThink.rank ?? 1,
          meta: originalThink.meta ?? { category: "chat_think" },
        });
      } catch (error) {
        console.error("恢复 think-base 失败：", error);
        failed += 1;
      }
    }

    if (uuid) {
      await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(uuid)}'`);
    }
    await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(replyBaseUuid)}'`);
    assert(true, "测试对话已删除");
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
