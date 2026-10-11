# dd-run 扩展签名立项与选型（O14 / F12）

> **状态**：v1.1｜ 选型稿已按推荐方案（D1-A/D2-A/D3/D5）实施**批次一**（2026-10-11，`63750f1`）；批次二（签发链）待 CI secret、批次三（文档与审计入账）待批次二 ｜ **日期**：2026-10-11 ｜ **输入**：《O9–O14 优化方向规划》O14 节 · 修复实施方案 **F12 纲要** · `dd-host/src/trust.rs`（S-05 / R-12 / D2 源码级核对）· security-audit §4.4.2「明确不做①」
> **定位**：本文是 **F12 的立项载体**（《O9–O14 规划》§7 预留）——只做**动机、边界取证、选型决策表与验收框架**，不含实现；各决策点（D1–D5）给出推荐但不锁死，评审拍板后按 §8 批次实施。
> **立项判据**（沿用 future-features-plan §1 硬判据）：**可行**（零冻结契约改动 / 有既有代码惯例可复用 / 依赖克制 / 可通过单测与真机走查验收）× **无冲突**（不与协议 v1.0 冻结、README 非目标、在途项、已证伪项冲突）。逐项核查见 §2。

---

## 1. 结论总表（30 秒版）

| 项 | 一句话 |
|---|---|
| 目标 | 同时关闭两处已声明边界：① trust 台账「**用户同意 + 变更检测 ≠ 防篡改**」定性（trust.rs 模块文档自记）；② 非 Windows `assess` fail-open（**D2 缺口声明**，trust.rs:287–291） |
| 推荐方案 | 宿主**内置发行方 ed25519 公钥** + 清单旁挂 **`.sig` detached 签名**（签名对象 = 清单字节 + 清单/exe 双 SHA256 锚）+ 三态降级（验签通过 → 免同意；失败 → `Pending`+告警；无签名 → 现行同意流） |
| 协议影响 | 清单 schema v1.0 **零字段改动**（`.sig` 在清单之外）；`.sig` 不触碰 NDJSON 方法面与协商——按 protocol.md §13 预期**连 MINOR 递增都不需要**，登记性质为「扫描层分发契约」（manifest-schema.md 新节 + 设计文档 §8.2 注记）；v1.0 冻结不破坏 |
| 依赖增量 | `ed25519-dalek`（纯 Rust，连带 curve25519-dalek / signature trait，约 2–4 crate、零 C 依赖）——D1 选型定案后进 Cargo.lock |
| 改动量 | 大（trust/scan 判定链 + 打包链签名步骤 + 文档），**独立里程碑**；与 O6（跨平台）二选一作为下一主战役的定位不变 |
| 决策点 | D1 算法 / D2 载体 / D3 签名对象 / D4 信任模型 / D5 降级路径（§4，各含推荐） |

**一句话**：签名是唯一能同时闭合「非防篡改」定性与非 Windows fail-open 的方案，也是将来第三方分发生态的前置；本方案刻意做到**清单字节零改动**，把契约增量压缩到「`.sig` 文件的发现与校验规则」一条。

---

## 2. 冲突面核查矩阵

| 提案面 | 冻结协议 v1.0 | README 非目标 | 既有在途项 | 已证伪项 | 编号空间 | 结论 |
|---|---|---|---|---|---|---|
| 旁挂 `.sig` + 宿主校验 | 清单 schema/字节不动；`.sig` 发现规则走 v1.1 候选登记 | 不改（是「签名信任」不是「扩展商店」，无分发/搜索/评分面） | **承接 F12**（修复实施方案预留「单独立项」）；与 **R-12 钉扎**衔接而非替代（§4-D5） | 无冲突 | O14（规划文档）+ F12（修复方案）双编号衔接；security-audit 附录 **S-xx 续位**入账 | ✅ |
| 内置公钥 | 不涉及 | 不改 | 复用 `include_str!`/编译期常量惯例（首次引入，无先例冲突） | 无冲突 | 同上 | ✅ |
| 第三方作者签名 | 不涉及 | **不做**（需公钥分发渠道 = 商店雏形） | — | — | — | ❌ 本期不做（§7） |

---

## 3. 现状边界（源码级取证，2026-10-11 核对）

1. **非防篡改定性**：`trust.rs` 模块文档自记——「本模块实现的是『用户同意 + 变更检测』，**不是防篡改**。能写 `extensions.d` 的攻击者同样能写 `trust.json`。真正意义上的防篡改需要扩展签名（宿主内置公钥 + 作者签名）」。security-audit §4.4.2「明确不做①」同口径。
2. **非 Windows fail-open**：`trust.rs` `assess` 首行 `if !hash_available() { Trust::AutoTrusted }`（注释「D2 缺口声明：非 Windows 不做门禁」）——无哈希能力平台全部自动信任。签名验签若为**纯 Rust 跨平台实现**（D1-A），此缺口顺带收口；若选 Windows CNG（D1-B）则依旧保留。
3. **R-12 静默重钉窗口**：首方 sidecar 首跑钉扎后，**升级**（宿主版本变化）时静默重钉——攻击者在升级窗口替换 sidecar 即可白拿新锚。签名验证以发行方私钥为锚，天然闭合该窗口（§4-D3/D5）。
4. **既有可复用惯例**：CNG 流式哈希（64 KiB/块，`sha256_file`/`sha256_bytes`）已是 trust.rs 既有设施；`assess` 短路判定表结构可直接扩展一行「签名分支」；`LoadedExtension` 已携带清单路径（`path`）与解析后 exe 路径（`command`），验签输入可由磁盘直接重读，CNG 流式哈希设施现成。

---

## 4. 选型决策表（D1–D5，各含推荐；评审可改判）

### D1 — 签名算法与实现

| 选项 | 说明 | 依赖 | 跨平台 | 评析 |
|---|---|---|---|---|
| **A. ed25519-dalek（推荐）** | 纯 Rust Ed25519 | 约 2–4 crate（curve25519-dalek / signature），零 C | ✅ 验签全平台可用 | 同时消解 D2 非 Windows 缺口；Windows CNG 不支持 Ed25519，故 CNG 复用先例在此不成立 |
| B. CNG ECDSA P-256 | 沿用 trust.rs 的 BCrypt 惯例 | 零新增（windows-sys 已在 lock） | ❌ 非 Windows 仍 fail-open | lock 零增量最优，但把 D2 缺口原样带进签名体系，与「关闭 fail-open」目标自相矛盾 |
| C. minisign/signify 外部工具链 | 签发用官方工具，运行时仍需自实现验签 | 同 A | ✅ | 签发端省事但运行时依赖量相同，且引入第二套格式规范；不推荐 |

**推荐 A**。代价是 Cargo.lock 首次引入密码学 crate（约 +1 MiB 编译产物，发行包体积影响需实测记录——O5-a 后体积纪律在案）。

> 事实核查（2026-10-11）：**Windows CNG 无 Ed25519 原生支持**——BCrypt 算法清单无 EdDSA 标识，`BCRYPT_ECDSA_ALGORITHM` 仅覆盖 NIST 曲线（P-256/384/521），Microsoft Q&A 明确「Ed25519/curve25519 接口不在 CNG 中」；2025 年 CNG 的 PQC 增补（ML-DSA/ML-KEM/SLH-DSA）亦不含 Ed25519。故「沿用 CNG 惯例做签名」必然退到 ECDSA P-256 并保留非 Windows 缺口，D1-B 的评析据此成立。

### D2 — 签名载体

| 选项 | 说明 | 评析 |
|---|---|---|
| **A. 旁挂 `<清单文件名>.sig`（推荐）** | 清单字节零改动；宿主扫描时同目录探测同名 `.sig` | schema v1.0 完全不动；F12 纲要既定方向（minisign 风格 detached）；代价 = `.sig` 的命名/位置/缺失语义需成文（§5） |
| B. 清单内嵌 `signature` 字段 | 签名自包含 | schema bump 至 1.1 + 全量内置/示例清单改写 + 解析迁移；`frozen` 字段先例表明内嵌字段可行，但迁移成本高于 A 且无对应收益 |

### D3 — 签名对象（关键设计）

**签名 = `清单字节 SHA256` + `entry.command 目标 exe SHA256` 的捆绑**（写进 `.sig` 的 trusted-comment 区，minisign 惯例），私钥对「捆绑串」签名。理由：

- 只签清单字节不够——exe 可单独替换；
- 捆绑双锚与 R-12 既有「清单+exe 双哈希」锚点**同构**，trust.rs 判定链可平滑扩展；
- exe 在打包后不可变（清单位于 `extensions.d/`，相对路径解析既有 §7 规则 8）——捆绑在签发时刻闭包。

### D4 — 信任模型与密钥保管

- **发行方公钥编译期内置**（`include_str!` 于 dd-host，随宿主分发——宿主本身即信任根，与「护栏非沙箱」定位一致）；
- 私钥：Release 工作流 GitHub Actions secret 签发 + 本地 `package.sh` 经环境变量兜底（真机验证路径）；**私钥泄露处置（轮换）另记运维文档，不在本期**；
- **第三方作者公钥导入：本期不做**（§7）——签名体系本期只覆盖首方分发物。

### D5 — 判定链整合与降级路径（三态）

`.sig` 存在性三态，插入 `assess` 判定表**最前**（短路）：

| 状态 | 判定 | 与现状对比 |
|---|---|---|
| `.sig` 存在且验签通过、双哈希一致 | `AutoTrusted`（**免同意**） | 首方 sidecar 的 R-12 首跑「待批准」被免掉——发行方背书即信任；钉扎重验保留作纵深防御 |
| `.sig` 存在但验签失败 / 双哈希不符 | `Pending` + 设置页告警（复用 `sidecar_tampered` 告警位先例） | **比现状严**（fail-closed）：现状篡改仅同版才有告警 |
| `.sig` 不存在 | 走现行 `assess` 全表（用户同意流），**零变化** | 不比现状差（O14 规划既定降级路径） |

---

## 5. 分发格式（草案，实现批次定稿）

```
dist/
  dd-run-<ver>.exe                 ← 公钥编译期内置
  extensions.d/
    com.ddrun.filesearch.json      ← 清单（字节不变，schema 1.0）
    com.ddrun.filesearch.json.sig  ← 新增：detached 签名（文本格式）
    dd-ext-search.exe              ← sidecar（被 D3 捆绑锚覆盖）
```

`.sig` 文本格式（minisign 风格草案）：算法标识行 + untrusted comment + base64 签名 + trusted-comment（内嵌双 SHA256 十六进制锚）。签名动作挂 `tools/package.sh` 尾部（本地 `DDRUN_SIGN_KEY` 环境变量）与 release.yml（Actions secret），双路径同一实现（脚本化，不引外部二进制）。

---

## 6. 验收判据（框架级；实施批次细化为单测 + 真机走查双口径）

1. 合法签名的首方扩展（filesearch sidecar）**免确认拉起**（此前 R-12 首跑需待批准）；
2. 清单或 exe **篡改任一字节** → `Pending` + 设置页告警，面板不拉起（先红后绿可复现）；
3. 删除 `.sig` → 现行同意流逐条回归（S-05 既有 12 条验收判据零变化）；
4. `trust.json` 损坏/删除不改变签名判定结果（签名锚独立于台账）；
5. 非 Windows：验签路径编译并单测通过（D2 缺口收口项）；真机腿随 O6；
6. 升级重钉窗口闭合：带旧 `.sig` 的新版 sidecar 被拒（签名与内置公钥/捆绑锚不符）。

## 7. 明确不做（防错漏 / 防冲突）

- **第三方作者签名生态 / 扩展商店 / 公钥分发渠道**：README 非目标，本期签名体系仅覆盖首方分发物；
- **证书链 / PKI / 吊销列表（CRL）**：单发行方模型下无意义；吊销由 trust.json `Deny`（既有）承担；
- **私钥轮换与保管的完整运维方案**：另记运维文档，本期仅约定 secret 通道；
- **非 Windows 打包链签名**：验签跨平台可用（D1-A），签发仍仅 Windows/CI；
- **协议方法面改动**：`.sig` 全程不触碰 NDJSON 方法面与 `initialize` 协商。

## 8. 批次一落地记录（2026-10-11，`63750f1`）

- **依赖实测**：`ed25519-dalek 2.2`（default-features=false，仅 std/fast）——Cargo.lock 实测 +23 条目，其中实际依赖图约 14 包（`sha2` 为 Ed25519 算法内必需，不可裁剪）；其余为 resolver 保留条目。发行包体积实测随批次二 dist 重打包记录；
- **新增 `dd-host::signing`**：`.sig` 文本格式定稿（untrusted 行 + 88 字符 base64 签名 + trusted 行内嵌双 SHA256 hex 锚；手写 base64 编解码，防为 88 字符引入 base64 crate）；捆绑消息带域分隔前缀 `dd-run-ext-sig-v1`（防哈希错位重放）；
- **三态整合**：签名分支先于台账/钉扎链短路；`Assessment` 新增 `sig_invalid` 告警位（设置页展示随批次三接线）；**D5 细化：用户 Deny 优先于签名免同意**（与 R-12 ⑤ 同款——签名不覆盖用户明确拒绝）；
- **占位公钥**：`PUBLISHER_PUBLIC_KEY_HEX` 由 openssl 现场生成、种子即弃——当前任何 `.sig` 均判 Invalid（预期 fail-closed 姿态），批次二轮换真钥；
- **非 Windows 口径修正**：ed25519 验签已跨平台就绪，但捆绑锚的 SHA-256 仍 Windows-only（trust.rs 既有缺口）→ `check_signature` 在非 Windows 恒 `Absent`、现行 fail-open 不变。**D1-A 对 D2 的收口是"就绪"而非"完成"**：哈希跨平台化随 O6；
- **测试**：新增 12 条（signing 单测 6 + trust 三态整合 6），先红后绿实证（恒 Absent 桩下 3 条签名依赖腿 FAILED → 恢复 623/0）；批次一覆盖验收框架 §6 之 1/2/3/4/6 的单测腿（判 5 非 Windows 腿与端到端内置钥腿随批次二/三）；
- **批次二待办**：真钥生成与 CI secret、`package.sh`/release.yml 签发步骤、`PUBLISHER_PUBLIC_KEY_HEX` 轮换、端到端内置钥腿、发行包体积实测；**批次三**：manifest-schema.md「分发签名」节 + protocol.md 登记 + 设计文档 §8.2 注记 + security-audit S-xx 续位 + 设置页 `sig_invalid` 告警展示。

## 9. 与既有文档关系 + 实施批次建议

- 本文 = **F12 的立项载体**（《O9–O14 规划》§7 衔接）；立项通过后修复实施方案 F12 节回写落地注记，security-audit 附录以 **S-xx 续位**将「扩展签名」编号入账（沿用 S-01–S-11 后续位）；
- 协议侧：manifest-schema.md 新增「分发签名」节（`.sig` 发现/校验/缺失语义）+ protocol.md **v1.1 候选流程**登记（清单字节零改动的说明随附）；cmdpal-platform-agnostic-design.md §8.2 协议注释同步；
- 建议批次：**批次一** trust.rs 签名分支 + ed25519-dalek 接入 + 单测（判定表三态先红后绿）；**批次二** package.sh / release.yml 签发链 + `.sig` 分发；**批次三** manifest-schema/protocol/design 文档 + security-audit 附录 + CHANGELOG。批次一/二可独立交付；
- 里程碑定位：与 **O6（跨平台）二选一**作为下一主战役——D1-A 落地后 O6 的非 Windows 门禁前置已消除一半，两者存在正向耦合但无相互阻塞。

---

*依据：trust.rs / manifest.rs / package.sh 源码核对（HEAD `1f4756d`，2026-10-11）；本文为选型稿，全部决策点（D1–D5）待评审定案后生效。*
