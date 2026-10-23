import { call, section, assert } from "./client.ts";

async function main() {
  section("embed_prompts：批量向量化（limit=5）");

  const res = await call("embed_prompts", { limit: 5, offset: 0 });
  console.log(res);

  assert(res.code === 200, "embed_prompts 返回 200");
  assert(typeof res.data?.processed === "number", "processed 是数字");
  assert(typeof res.data?.inserted === "number", "inserted 是数字");
  assert(res.data?.processed === 5, "处理了 5 条");
}

main();
