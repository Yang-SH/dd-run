# 宿主 exe 代码签名信任链评估（O16）

> **状态**：生效中（v1.0，评估记档——**采不采纳由维护者拍板**） ｜ **版本**：v1.0 ｜ **最后更新**：2026-11-22
> **关联**：[altrun-对照优化方案-2026-11-21.md](./altrun-对照优化方案-2026-11-21.md)（§5.2 提案） · [altrun-对照优化实施计划-2026-11-22.md](./altrun-对照优化实施计划-2026-11-22.md)（批次 D-4） · [extension-signing-plan.md](./extension-signing-plan.md)（扩展 `.sig` 链路——与本文并行不悖） · [INDEX.md](./INDEX.md)

---

## 1. 结论先行

| 维度 | SignPath.io（OSS 计划） | Azure Trusted Signing |
| --- | --- | --- |
| 成本 | **免费**（SignPath Foundation 签发的开源证书；OSS 计划需申请审核） | 门槛：需 Azure 订单（曾为 $9.99/月 Basic + 证书费；政策随时间变动，**签约前以 Azure 官方页实测为准**） |
| 门槛 | 开源项目申请 + GitHub App 连接 + 组织验证（个人账号可申请 OSS 计划；审核周期数天~数周） | Azure 租户 + Identity Validation（组织/个人法定身份核验，材料较重） |
| 集成面 | SignPath.io：CI 经 `SignPath` CLI / GitHub App——`release.yml` 在 zip 产出处加一步签名（exe 先签再打包，或签 zip 内主 exe）；`tools/package.sh` 本地产出不经签名（仅 CI 发版链签名） | Trusted Signing：`Azure Trusted Signing` GitHub Action 或 `signtool` + ACS 集成；`release.yml` 同点接入；需要 Azure 资源与连接串（secret 管理） |
| SmartScreen 声誉 | Authenticode 签名即消除「未知发布者」警示；SmartScreen 下载声誉仍需安装量积累（两方案同——**声誉属文件/发布者历史，不属证书品牌**） | 同左 |
| 维护面 | 第三方 SaaS（账号 + GitHub App 权限）；OSS 计划年度复核 | Azure 资源生命周期（region 不可迁移、identity 续期） |
| 风险 | OSS 审核不通过（备选：自购 OV 证书 + 硬件令牌/云 HSM，年费数百美元级） | 政策变动（曾对个体开发者收紧）；依赖 Azure 可用性 |

**评估建议（供拍板，不替决策）**：**优先申请 SignPath OSS 计划**（零成本、流程轻、与 GitHub Actions 集成成熟；dd-run 已满足开源前提——MIT + 公开仓库）；Azure Trusted Signing 作为备选（若 OSS 审核被拒或需要更快的企业级通道）。现有信任链（扩展 ed25519 `.sig` + GitHub attestation）**零改动、两者并行**。

## 2. 评估范围与现状（实测取证）

- **现有信任面**（本评估不触碰）：扩展信任 = 清单旁挂 ed25519 `.sig`（发行方公钥 + 内容哈希锚，extension-signing-plan v1.4 已落地发版链）；构建来源 = GitHub attestation（build provenance，release.yml 随 SHA256SUMS 附着）。**均不解决宿主 exe 的 SmartScreen「未知发布者」拦截**——这正是本评估的对象。
- **集成点**（实测）：`release.yml` 构建 → `tools/portable_zip.ps1` 打包 → softprops action 附着资产（约 :165–170）。签名插入点 = **打包前对 `dist/dd-run-<ver>.exe` 签名**（ Authenticode 内嵌于 exe，zip 原样携带；SHA256SUMS 在签名后生成——哈希随产物自动一致，无新链路）。
- **对照基线**：ALTRun（zhugecaomao/ALTRun）经 SignPath Foundation 免费签名（README:221「Code signing policy」节）——同类 OSS 启动器先例成立。

## 3. 两候选集成草案（采纳后展开为实施方案）

### 3.1 SignPath OSS（建议首选）

1. github.com 申请 SignPath OSS 计划（开源项目信息 + GitHub 组织/个人验证）→ 获 SignPath 项目与签名证书（Foundation 签发）。
2. 连接 GitHub App（SignPath App 安装到仓库）+ 仓库 secret 配置 API token。
3. `release.yml` 打包步骤前插入：`SignPath CLI`（或 HTTP API）对 `dist/dd-run-<ver>.exe` 提交签名请求（策略 = 「release 制品」），产物写回同路径。
4. 本地 `tools/package.sh` 产出**不签名**（签名仅发版链——本地构建无证书私钥，与现行「本地构建不签 `.sig`」同口径）。
5. 风险与回退：审核拒绝 → 转 Azure 备选或维持现状（现有信任面不变，非阻塞项）。

### 3.2 Azure Trusted Signing（备选）

1. Azure 订阅创建 Trusted Signing 账户 + Identity Validation（法定身份材料）。
2. `release.yml` 用 `Azure/trusted-signing-action`（或 signtool + ACS 凭证）签名 exe；凭证经 OIDC/secret。
3. 成本与政策以签约时 Azure 官方页实测为准（个体开发者档曾收紧——记风险）。

## 4. 决策清单（维护者拍板项）

- [ ] 是否申请 SignPath OSS 计划（建议：是）
- [ ] 被拒后的备选顺序（建议：Azure Trusted Signing → 维持现状重评）
- [ ] 签名产物口径（建议：仅发版链 exe；zip/其它资产不签）

## 5. 版本演进

| 版本 | 日期 | 变更 |
|---|---|---|
| v1.0 | 2026-11-22 | 首版：O16 评估记档（方案 §5.2 / 实施计划批次 D-4）——SignPath OSS vs Azure Trusted Signing 对照（成本/门槛/集成点/声誉/维护面/风险）+ 集成草案 + 决策清单；建议 = 优先 SignPath OSS（**不替维护者拍板**）；现有 `.sig`/attestation 链路零改动 |
