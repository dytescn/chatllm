import { call, section, assert } from "./client.ts";

async function main() {
  section("sessions_post：创建");
  const created = await call("sessions_post", {
    title: "测试会话",
    model: "qwen3",
    system_prompt: "你是助手",
  });
  console.log(created);
  assert(created.code === 200, "创建返回 200");
  const id = created.data?.id;
  assert(typeof id === "string" && id.length === 36, "生成 UUID id");
  if (!id) return;

  section("sessions_get：按 id 获取");
  const got = await call("sessions_get", { id });
  console.log(got);
  assert(got.code === 200, "get 返回 200");
  assert(got.data?.title === "测试会话", "title 正确");
  assert(got.data?.model === "qwen3", "model 正确");

  section("sessions_list：列表");
  const list = await call("sessions_list", { page: 1, page_size: 100 });
  console.log(list);
  assert(list.code === 200, "list 返回 200");
  assert((list.data?.list ?? []).some((r: any) => r.id === id), "列表含该 id");

  section("sessions_update：全量更新");
  const updated = await call("sessions_update", {
    id,
    title: "更新后的标题",
    model: "qwen4",
    system_prompt: "",
  });
  console.log(updated);
  assert(updated.code === 200, "update 返回 200");
  assert(updated.data?.title === "更新后的标题", "title 已更新");
  assert(updated.data?.model === "qwen4", "model 已更新");
  assert(updated.data?.system_prompt === "", "system_prompt 已清空");

  section("sessions_patch：部分更新");
  const patched = await call("sessions_patch", {
    id,
    system_prompt: "新的系统提示",
  });
  console.log(patched);
  assert(patched.code === 200, "patch 返回 200");
  assert(patched.data?.system_prompt === "新的系统提示", "system_prompt 已更新");
  assert(patched.data?.title === "更新后的标题", "patch 未覆盖 title");

  section("sessions_get：不存在返回 404");
  const missing = await call("sessions_get", { id: "missing_id" });
  console.log(missing);
  assert(missing.code === 404, "返回 404");

  section("sessions_get：缺 id 返回 400102");
  const bad = await call("sessions_get", {});
  console.log(bad);
  assert(bad.code === 400102, "返回 400102");
}

main();
