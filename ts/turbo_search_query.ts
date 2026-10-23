import { call, section } from "./client.ts";

async function timedCall(body: unknown, label: string) {
  const start = performance.now();
  const res = await call("turbo_search_query", body);
  const elapsed = performance.now() - start;
  console.log(`\n[${label}] 耗时 ${elapsed.toFixed(1)} ms`);
  return res;
}

async function main() {
  section("turbo_search_query：向量语义检索");

  const queries = [
    "人工智能的发展历程",
    "健康生活方式和饮食建议",
    "如何减轻工作压力",
  ];
  const topK = 5;

  for (let i = 0; i < queries.length; i++) {
    const res = await timedCall(
      { query: queries[i], top_k: topK },
      `第 ${i + 1} 次 query=${queries[i]}`,
    );

    if (res.code !== 200) {
      console.error(`❌ turbo_search_query 返回 ${res.code}: ${res.msg}`);
      process.exitCode = 1;
      continue;
    }

    const results = res.data?.results ?? [];
    const top = results[0];
    console.log(
      `  结果 ${results.length} 条，top1: ${top?.name ?? "-"} (score=${top?.score ?? "-"})`,
    );
  }
}

main();
