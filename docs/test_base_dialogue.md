# test_base_dialogue

> 对应测试文件：`tests/base_dialogue.rs`。
> 环境准备与建库流程见 `docs/testenv.md`。

## 1. 覆盖接口

- `patch_dialogue`（FFI-Symbol：`patch_dialogue`）：入参 `DialogueListReq`（`keyword` / `page` / `page_size`，允许空 body）。
- `update_dialogue`（FFI-Symbol：`update_dialogue`）：入参 `Dialogue`（`uuid` 定位，`title` / `show_status` 为待更新项）。

## 2. `patch_dialogue`

| 用例 | 输入 | 预期 |
| --- | --- | --- |
| `patch_dialogue_returns_visible_list` | 空 body | `code = 200`，`page = 1`；列表含 `1111…`，不含 `3333…`（`show_status = 2`）与 `4444…`（`is_archived = 1`） |
| `patch_dialogue_searches_by_title` | `{"keyword":"接口","page":1,"page_size":10}` | `code = 200`，结果非空且标题均含「接口」 |
| `invalid_json_returns_400102` | `{"page":"bad"}` | `code = 400102` |

## 3. `update_dialogue`

| 用例 | 输入 | 预期 |
| --- | --- | --- |
| `update_dialogue_updates_title` | `{"uuid":"1111…","title":"产品需求讨论（已更新）"}` | `code = 200`，`data.id = uuid`；再用 `patch_dialogue` 搜索「已更新」可查到；测试末尾恢复原标题 |
| `update_dialogue_rejects_bad_uuid` | `{"uuid":"not-a-uuid","title":"x"}` | `code = 400103` |
| `invalid_json_returns_400102` | `{"show_status":"bad"}` | `code = 400102` |

## 4. 运行

```bash
# 仓库根目录；数据测试会自动把 cwd 切到 testenv
cargo test --test base_dialogue

# 查看每个请求的返回
cargo test --test base_dialogue -- --nocapture --test-threads=1
```

每次调用会打印三行，便于核对：

```text
request : {"keyword":"接口","page":1,"page_size":10}
rc      : 0
response: {"code":200,"data":{"list":[…],"page":1,"page_size":10,"total":1},"msg":"success"}
```

## 5. 依赖与清理

- 依赖 `docs/testenv.md` 中的基础数据（`1111…`、`3333…`、`4444…`）。
- 更新用例会修改 seed 标题，测试结束前已恢复，可重复执行。
