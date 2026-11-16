# chatllm C ABI 接口

`chatllm` 编译为 `cdylib`，导出 C ABI `post_*` 函数。
对话流程为 `think → chat_plugins → reply`；`check` 已移除，后续以插件形式提供。

## 1. 调用约定

```c
int (*)(const char *json_in, char *json_out);
```

| 约定 | 说明 |
| --- | --- |
| 输入 | NUL 结尾的 UTF-8 JSON；空指针 `400100`，非 UTF-8 `400101` |
| 输出 | 调用方提供 65,536 字节缓冲区；超限返回 `-2` |
| 返回值 | `0` 已写入；`-1` 输出指针为空；`-2` 缓冲区不足 |
| 响应 | `{"code":200,"data":{...},"msg":"success"}` |

## 2. 接口

| 导出函数 | 说明 | 文档 |
| --- | --- | --- |
| `post_chat` | 新建 / 续聊并执行对话流程 | [`ffi/chat.md`](ffi/chat.md) |
| `post_dialogue_list` | 对话分页列表 | [`ffi/dialogue_list.md`](ffi/dialogue_list.md) |
| `post_dialogue_update` | 更新对话字段 | [`ffi/dialogue_update.md`](ffi/dialogue_update.md) |
| `post_dialogue_search` | 按标题搜索 | [`ffi/dialogue_search.md`](ffi/dialogue_search.md) |
| `post_chat_detail` | 读取对话与消息 | [`ffi/chat_detail.md`](ffi/chat_detail.md) |

## 3. 错误码

| 码 | 含义 |
| --- | --- |
| `400100` | `json_in` 为空 |
| `400101` | 请求非 UTF-8 |
| `400102` | 请求 JSON 非法 |
| `400103` | uuid 非法 |
| `400300` | 创建对话失败 |
| `400301` | 追加 user 消息失败 |
| `400302` | 对话列表失败 |
| `400303` | 读取 / 更新对话失败 |
| `400304` | 插件未命中 / 信息缺失 / 调用失败 |

## 4. 相关文件

| 文件 | 内容 |
| --- | --- |
| `src/ffi/chat.rs` | FFI 入口 |
| `src/ffi/types.rs` | 请求结构 |
| `src/utils/run_flow.rs` | 流程编排 |
| `src/utils/taskflow.rs` | taskflow 读取 |
| `src/utils/think.rs` | `chat_think` 检索与回调 |
| `src/utils/chat_plugins.rs` | `chat_plugins` 逐条检索 → taskflow → 回调 |
| `src/utils/reply.rs` | `chat_reply` 与回复组装 |
| `src/utils/plugins_lib.rs` | designplugin_lib 的 FFI 客户端 |
