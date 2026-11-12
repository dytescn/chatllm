// ts/test_check.ts
// 测试：用户未设置 AI 供应商时，check 阶段直接返回设置提示与 /setting 路由。
//
// 运行：
//   deno run --allow-net ts/test_check.ts
//
// 说明：脚本临时删除 ai_supplier，结束后恢复原值。
//       check 不创建对话、不写消息，响应直接携带 content 与 route。

const INFO_DB = "./info";
const SUPPLIER_KEY = "ai_supplier";
const EXPECTED_TEXT = "请选择设置AI供应商";
const EXPECTED_ROUTE = "/setting";

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

async function readSupplier(): Promise<string | null> {
  const result = await dbQuery(
    `SELECT value FROM setting WHERE key = '${esc(SUPPLIER_KEY)}' LIMIT 1`,
  );
  if (result.code !== 200) return null;
  const value = rows(result)[0]?.value;
  return typeof value === "string" ? value : null;
}

async function clearSupplier(): Promise<void> {
  const result = await dbUpdate(
    `DELETE FROM setting WHERE key = '${esc(SUPPLIER_KEY)}'`,
  );
  if (result.code !== 200 && !String(result.msg ?? "").includes("no such table")) {
    throw new Error(`删除 ${SUPPLIER_KEY} 失败：${JSON.stringify(result)}`);
  }
}

async function restoreSupplier(original: string | null): Promise<void> {
  if (original === null) {
    await clearSupplier();
    return;
  }
  const result = await dbUpdate(
    `INSERT INTO setting (key, value) VALUES ('${esc(SUPPLIER_KEY)}', '${esc(original)}')
     ON CONFLICT(key) DO UPDATE SET value = excluded.value;`,
  );
  if (result.code !== 200) {
    throw new Error(`恢复 ${SUPPLIER_KEY} 失败：${JSON.stringify(result)}`);
  }
}

function assertBlocked(reply: Json, label: string) {
  console.log(reply);
  assert(reply.code === 200, `${label}：chat 返回 200`);
  assert(reply.data?.route === EXPECTED_ROUTE, `${label}：route = /setting`);
  assert(reply.data?.uuid === null, `${label}：不创建对话 uuid = null`);
  assert(reply.data?.created === false, `${label}：created = false`);
  const content = String(reply.data?.content ?? "");
  assert(content.includes(EXPECTED_TEXT), `${label}：content 包含设置提示`);
  assert(content.includes(EXPECTED_ROUTE), `${label}：content 包含 /setting`);
}

async function main() {
  const original = await readSupplier();
  console.log(
    `当前 ${SUPPLIER_KEY} = ${original === null ? "<未设置>" : JSON.stringify(original)}`,
  );

  try {
    section("1. 清除 AI 供应商配置");
    await clearSupplier();
    assert((await readSupplier()) === null, "ai_supplier 已清除");

    section("2. check 直接返回设置提示");
    assertBlocked(
      await post("chatllm", "chat", { prompt: "test_check 未设置供应商" }),
      "首次请求",
    );

    section("3. 再次请求仍然直接返回");
    assertBlocked(
      await post("chatllm", "chat", { prompt: "test_check 再次请求" }),
      "再次请求",
    );
  } finally {
    section("4. 清理");
    await restoreSupplier(original);
    assert((await readSupplier()) === original, "ai_supplier 已恢复");
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
