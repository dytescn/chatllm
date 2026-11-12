// ts/client.ts —— chatllm FFI 调用与测试辅助（Deno FFI，不走 HTTP）
import { post } from "./ffi.ts";

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

/** 调用 chatllm 的一个 FFI-Symbol（同步 FFI，保持 async 以兼容旧调用）。 */
export async function call(symbol: string, body: Json = {}): Promise<Json> {
  return post("chatllm", symbol, body);
}
