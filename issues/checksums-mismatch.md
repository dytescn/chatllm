# [P1] `dytes.json` 的 `setup.json` checksum 与实际文件不一致

- 发现日期：2026-11-10
- 包：`chatllm`（`type: sys`）
- 线上版本：`1.0.14`
- 文件：`system/chatllm/1.0.14/windows/setup.json`

## 现象

使用 `dytes install` / `dytes update` 安装 Piksel 时：

```text
sys chatllm: 校验失败，已删除文件: .../libary/setup.json:
文件大小不匹配（期望 1265，实际 1295）
```

清单 `dytes.json` 记录：

```json
"checksums": {
  "setup.json": {
    "size": 1265,
    "sha256": "25ad30c03bd53da60f08eb2336fb20289451277ee47920ad7ecd4e98e846d4d2"
  }
}
```

实际下载的文件：

```text
size:   1295
sha256: ea8e68414c4aa89ea8fb6d12d6abd73a4a17029b39d4a52cd69292a966f2aa7c
```

## 影响

- `dytes` 校验失败后删除 `setup.json`，`chatllm` 安装/初始化中止；
- 依赖 `chatllm` 的对话、向量与检索能力不可用；
- `dytes install` 最终返回非 0。

## 修复建议

1. 基于最终上传的 `setup.json` 重新生成 `checksums.size` / `checksums.sha256`；
2. 发布新的 patch 版本（例如 `1.0.15`），不要覆盖 `1.0.14` 的版本化路径；
3. 刷新 `packages.funxdata.com` 对应 CDN 缓存；
4. 使用 `dytes update --type sys` 验证不再出现大小/哈希不匹配。

## 复现

```bash
curl -fsSL "https://packages.funxdata.com/system/chatllm/1.0.14/dytes.json" | grep -A8 checksums
curl -fsSL "https://packages.funxdata.com/system/chatllm/1.0.14/windows/setup.json" | wc -c
```
