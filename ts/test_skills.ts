import { call, section, assert } from "./client.ts";

async function main() {
  const uid = `skill_${Date.now()}`;

  section("skills_post：创建");
  const created = await call("skills_post", {
    uid,
    content: "hello skill",
    version: 1,
  });
  console.log(created);
  assert(created.code === 200, "创建返回 200");
  assert(created.data?.uid === uid, "uid 正确");
  assert(created.data?.content === "hello skill", "content 正确");
  assert(created.data?.version === 1, "version 为 1");
  assert(typeof created.data?.id === "number", "id 是数字");

  section("skills_get：按 uid 获取");
  const got = await call("skills_get", { uid });
  console.log(got);
  assert(got.code === 200, "get 返回 200");
  assert(got.data?.uid === uid, "get 命中同一 uid");

  section("skills_list：列表");
  const list = await call("skills_list", { page: 1, page_size: 100 });
  console.log(list);
  assert(list.code === 200, "list 返回 200");
  assert(Array.isArray(list.data?.list), "list 是数组");
  assert((list.data?.list ?? []).some((r: any) => r.uid === uid), "列表含该 uid");

  section("skills_update：全量更新");
  const updated = await call("skills_update", {
    uid,
    content: "updated content",
    version: 2,
  });
  console.log(updated);
  assert(updated.code === 200, "update 返回 200");
  assert(updated.data?.content === "updated content", "content 已更新");
  assert(updated.data?.version === 2, "version 已更新为 2");

  section("skills_patch：部分更新");
  const patched = await call("skills_patch", { uid, version: 3 });
  console.log(patched);
  assert(patched.code === 200, "patch 返回 200");
  assert(patched.data?.content === "updated content", "patch 未覆盖 content");
  assert(patched.data?.version === 3, "patch 已更新 version 为 3");

  section("skills_get：不存在返回 404");
  const missing = await call("skills_get", { uid: "missing_uid" });
  console.log(missing);
  assert(missing.code === 404, "返回 404");

  section("skills_post：缺 content 返回 400102");
  const bad = await call("skills_post", {});
  console.log(bad);
  assert(bad.code === 400102, "返回 400102");
}

main();
