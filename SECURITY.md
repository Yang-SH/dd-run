# 安全策略

## 报告漏洞

**不要**用公开 issue 报告安全漏洞。请使用 GitHub 的[私密安全报告](https://github.com/Yang-SH/dd-run/security/advisories/new)（Security → Report a vulnerability），或通过仓库主页联系方式联系维护者。

报告时请尽量附：复现步骤 / 影响面（本地提权、越界读写、逃逸到扩展进程外等）/ 版本或 commit。收到后 7 天内确认，修复发布前不公开细节。

## 信任模型（简述）

- **宿主 vs 扩展**：扩展是独立进程（或随宿主分发的 in-process 内置件），经 NDJSON-over-stdio 的 JSON-RPC 与宿主通信（协议见 `docs/protocol.md`）。扩展不可信：stdout 逐行有界解析（单帧上限 1 MiB）、调用有超时、崩溃计数熔断。
- **扩展信任分级**：首次运行需用户批准；批准状态落本机信任台账（fail-closed——台账损坏则全部回退「待批准」）。发行方签名（Ed25519 清单旁挂 `.sig`）验签通过即免同意。
- **攻击面收口**：`open_url` scheme 白名单、reveal 路径字符门控、剪贴板上限 + 溯源、图标读盘/解码双上限、危险命令二次确认等——完整清单与威胁模型见 `docs/security-audit-2026-09-23.md`。

## 支持版本

| 版本 | 状态 |
| --- | --- |
| 最新 Release（v0.2.x） | ✅ 接收安全修复 |
| 更早版本 | ❌ 不回补，请升级 |

## 依赖漏洞

CI 的 `audit` job 跑 `cargo audit`（RustSec advisory-db）+ `cargo machete`（死依赖），发现 vulnerability 即红。
