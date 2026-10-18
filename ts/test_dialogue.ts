// ts/test_dialogue.ts
import { call, section, assert } from "./client.ts";

async function main() {
  // 准备
  section("准备：新建一个对话");
  const c = await call("chat", { prompt: "更新测试", name: "待更新", keyword: "旧关键字" });
  console.log(c);
  const id = c.data?.uuid;
  if (!id) {
    console.error("未拿到 uuid，终止");
    return;
  }

  // 更新 title/keyword/is_pinned
  section("dialogue_update：title/keyword/is_pinned");
  const u1 = await call("dialogue_update", {
    id,
    title: "改名后",
    keyword: "新关键字",
    is_pinned: 1,
  });
  console.log(u1);
  assert(u1.code === 200, "更新返回 200");

  // 列表核对（在 list 里 find 该 id）
  section("dialogue_list 核对");
  const list = await call("dialogue_list", { page: 1, page_size: 100 });
  console.log(list);
  const row = (list.data?.list ?? []).find((r: any) => r.id === id);
  assert(!!row, "列表含该 id");
  assert(row?.title === "改名后", "title 已改");
  assert(row?.keyword === "新关键字", "keyword 已改");
  assert(row?.is_pinned === 1, "is_pinned 已置 1");

  // 只改 is_archived
  section("dialogue_update：只改 is_archived");
  const u2 = await call("dialogue_update", { id, is_archived: 1 });
  console.log(u2);
  assert(u2.code === 200, "归档返回 200");

  const list2 = await call("dialogue_list", { page: 1, page_size: 100 });
  const row2 = (list2.data?.list ?? []).find((r: any) => r.id === id);
  assert(row2?.is_archived === 1, "is_archived 已置 1");
  assert(row2?.title === "改名后", "其他字段未被覆盖");

  // 空 patch（只传 id）
  section("dialogue_update：只传 id（no-op）");
  const u3 = await call("dialogue_update", { id });
  console.log(u3);
  assert(u3.code === 200, "no-op 返回 200");
}

main();
