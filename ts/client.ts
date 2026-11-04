// ts/client.ts —— chatllm FFI 调用与测试辅助
const BASE = "http://127.0.0.1:44944";

export type Json = Record<string, any>;

let failed = 0;

export function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

export function assert(cond: boolean, message: string) {
  if (cond) {
    console.log(`✅ ${message}`);
  } else {
    console.error(`❌ ${message}`);
    failed += 1;
  }
}

export function summary() {
  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  return failed;
}

/** 调用 chatllm 的一个 FFI-Symbol。 */
export async function call(symbol: string, body: Json = {}): Promise<Json> {
  const response = await fetch(`${BASE}/chatllm`, {
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
    throw new Error(
      `${symbol} 返回非 JSON（HTTP ${response.status}）：${text}`,
    );
  }
}
