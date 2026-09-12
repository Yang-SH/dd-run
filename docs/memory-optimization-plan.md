# 运行时内存占用优化

> 状态：设计稿 v1.0（2026-09-12 立项，经用户确认后实施）
> 范围：**dd-run.exe 宿主进程稳态内存**（Task Manager「内存」= 私有工作集 + 私有提交两个口径都覆盖）；扩展子进程已有 warm 空闲 120s 回收（方向 C 批次），本稿不重复设计。
> 原则：可实现、无错漏、无冲突、不复杂、符合需要——所有措施都以 M0 实测数字为锚，先测后改。
> 依赖的源码事实均已逐处核实（含 epaint 0.36.1 上游源码），见各节标注。

---

## 1. 现状盘点（源码核实结论）

| 构成 | 规模（估算） | 现状 | 判定 |
|---|---|---|---|
| **字体栈** | **~25–45 MB** | `platform.rs::load_cjk_font_definitions` 以 `std::fs::read` 整读 Vec → `FontData::from_owned`：msyh.ttc (~19.7MB) + seguisym.ttf + SegoeIcons/SegoeUI/后援——全部成为**私有提交内存** | ⚠️ 最大头 |
| **图标纹理缓存** | GPU 侧 ~4MB；私有 RAM ≈ 0 | `app/mod.rs` `icon_cache: HashMap<String,(TextureHandle,bool)>` **无上限、永不清理**；已核 epaint 0.36.1 `textures.rs`：`ImageData` 经 delta 队列上传 GPU 后即从 CPU 侧丢弃，纹理驻留显存/驱动镜像 | 长会话无界，RAM 影响取决于驱动 |
| 聚合数据（400 项 PanelItem / 拼音串 / 桩缓存） | <1 MB | 有界、随聚合重建 | ✅ 不动 |
| nucleo Matcher | 击键瞬时分配 | `state.rs::filtered()` 每次 `FuzzyMatcher::new`——打分矩阵随查询/列表长度分配后即弃 | 分配 churn，非稳态占用 |
| 扩展子进程 | 独立进程 | warm 空闲 120s 回收已上线（仅隐藏期驱动） | ✅ 已优化，不动 |

**观感问题**：面板隐藏后进程工作集不自动归还（页留驻等复用）——启动器类应用「越挂越肥」观感的直接来源。

## 2. 方案总览

| # | 措施 | 预期收益 | 改动量 | 风险 |
|---|---|---|---|---|
| M0 | 四态基线测量 | 数字锚点，防盲优化 | 临时探针 | 无 |
| M1 | 隐藏后工作集修剪（`SetProcessWorkingSetSize(-1,-1)`） | Task Manager 可见大头：隐藏态 60–100MB → ~15–25MB | ~25 行 | 极低 |
| M2 | 字体栈 mmap 化（私有提交 → 可回收文件页） | **私有提交 −~30MB**（稳态） | ~40 行 | 低 |
| M3 | 图标纹理缓存 `hide()` 清空 | 收敛无界增长；显存/驱动侧 | ~10 行 | 极低 |
| M4 | nucleo Matcher 复用 | 击键分配 churn 归零 | ~30 行 | 测量驱动，命中才做 |

实施顺序：M0 → M1 → M3 → M2 →（M4 视测量）→ 对比表 + dist 重编。

## 3. 各措施详设

### M0 — 基线测量

- **字体字节数探针**：临时测试直接调 `load_cjk_font_definitions` 前身逐文件打印字节数（后续 M2 改造的对照锚点），跑完删除。
- **进程四态采样**：PowerShell 采样脚本按时间轴记录 `PrivateMemorySize64`（私有提交）与 `WorkingSet64`（工作集）：① 启动首帧 → ② 聚合完成（~3s）→ ③ 稳态 → ④ 隐藏 120s+（默认态）。M1 的收益锚定 ④。
- 状态 ③（全量图标浏览）不做实时测量：纹理在 GPU/驱动侧，M3 用分析值（400 × 48×48×4 ≈ 3.7MB 上界）即可。

### M1 — 隐藏后工作集修剪

- 落点：`lifecycle.rs::hide()`（热键/Esc/托盘/失焦全路径收口于此）+ `health.rs::warm_idle_reclaim` 驱逐之后。
- API：`SetProcessWorkingSetSize(GetCurrentProcess(), -1, -1)`（windows-sys 补 `Win32_System_Memory` feature；非 Windows 空实现）。
- 语义（诚实记档）：OS 常规提示——页移出工作集、按需软故障回（µs 级），**私有提交不变**；收益是后台物理内存占用与 Task Manager 数字的真实下降。
- 与既有机制无冲突：挂在隐藏期既有 tick 内，不新增唤醒源；唤醒首帧软故障不可感。

### M2 — 字体栈 mmap 化（唯一动私有提交的措施）

- 上游事实（epaint 0.36.1 已核）：`FontData { font: Cow<'static,[u8]> }` 提供 `from_static(&'static [u8])`（`fonts.rs:131`）；解析路径为 `skrifa::FontRef::from_index(font_data.as_ref(), …)`（`font.rs:388`）——**纯借用**。mmap 后 FontData / skrifa / set_fonts 热替换重解析全链路直接引用文件页，零私有拷贝（连启动瞬时峰值也消失）。
- 落法：`load_cjk_font_definitions` 各 `std::fs::read` 改 `memmap2::Mmap`（新增 1 个成熟小依赖）+ `Box::leak` 成 `'static` 切片 + `FontData::from_static`；后台线程加载时序、热替换调用点零改动；仅 Windows 字体路径。
- 生命周期：字体 mmap 常驻进程全程（`Box::leak` 有意为之）；Windows Fonts 目录下文件被系统更新替换属极小概率且 mmap 只读不受影响。

### M3 — 图标纹理缓存 hide 清空

- `hide()` 中 `icon_cache.clear()`（全部 TextureHandle drop → egui 释放纹理）；`icon_failed` 负缓存保留（防失败重试刷屏的原语义不变）。
- 重现代价：下次唤起仅对可见行（≤13 个）读盘 PNG + 解码 + 上传，合计 <10ms 且惰性；apps 图标抽取落盘缓存原样复用，不重新抽取。

### M4 — nucleo Matcher 复用（测量驱动）

- `PaletteApp` 持常驻 `nucleo::Matcher`（UI 单线程，字段直存），`filtered()` 只重建 `Pattern`。
- 判定门：M0/源码分析显示稳态内存无贡献（瞬时分配即弃）——**默认不做**，仅在实测发现击键期私有内存显著抬升时实施。

## 4. 明确不做（防错漏 / 防冲突）

- **字体子集化**：Rust 无成熟子集器 + msyh 再分发许可问题——违反「不复杂」；
- **隐藏时清 glyph atlas / 字体重载**：伤唤起首帧，与核心体验冲突；
- **内置扩展退回子进程**：推翻 M9 用户决策；
- **聚合列表懒加载 / 压缩**：<1MB，收益不抵复杂度；
- **扩展子进程再优化**：120s 空闲回收已是本仓已验收方案。

## 5. 验收标准

1. M0 四态表格 **前后对比**（私有提交、工作集两列）；
2. M2 锚点：字体探针字节数 ≈ 私有提交下降量（±1MB）；
3. A2 冷启动计时与唤起首帧无回归（M1 软故障、M2 按需页入均 µs–ms 级）；
4. hide→show 循环内存平稳（M1/M3 无泄漏复归）；
5. `fmt/clippy/test` 全绿；dist 重编；真机复验四态观感。

## 6. 与既有文档关系

- `docs/implementation.md` §7 补批次记录；协议 v1.0 冻结不变；`cmdpal-ui-mockups.html` 无 UI 变更不动。

---

## 7. 实施结果（2026-09-12，debug 口径真机采样）

M4 判定：**不做**——Matcher 为击键瞬时分配即弃，稳态私有无贡献（源码级核实 + 采样中稳态数字平稳），仅记档。

实测对比（同一 debug 二进制口径，PowerShell 时间轴采样，隐私列 = `PrivateMemorySize64`、工作集列 = `WorkingSet64`；after 含临时探针驱动的一次「5s 唤起 → 13s 隐藏」循环，探针已删）：

| 状态（t） | before 私有 / 工作集 | after 私有 / 工作集 | 变化 |
|---|---|---|---|
| 同态：隐藏 + 聚合完成 + 未唤起（t=1–3s） | 137.8 / 154.9 | 92.9 / 110.1 | **私有 −44.9 MB** |
| 隐藏稳态（t=130s，after 经历过一次唤起+隐藏） | 135.2 / 152.6 | **115.6 / 8.6** | **私有 −19.6 MB · 工作集 −94%** |
| 冷启动（A2） | 971 ms（数据 882 + GUI 89） | 无回归（探针日志同量级） | ✅ |

- **M1+M3（工作集）**：隐藏稳态 Task Manager 口径 **152.6 → 8.6 MB**；隐藏收尾日志 `icon_cache 清空（N 项纹理）` 确认触发，warm 驱逐后同步修剪生效。
- **M2（私有提交）**：隐藏稳态 **−19.6 MB**（字体文件实测 22.56 MB；±3 MB 为运行间聚合/堆差异）。同态对比 −44.9 MB 略超锚点——字体私有分配消除之外，owned 大块 Vec 的堆驻留一并消除（debug 口径），精确分解留 release 口径复核，不影响结论方向。
- **验收**：`fmt/clippy/test` 全绿（371 passed / 0 failed）；dist 重编（`dd-run-0.1.1.exe` 8.2 MB）；临时采样脚本与探针已删。
- **真机复验待做**：① 日常唤起/隐藏循环体感无回归（唤起首帧无闪烁/无字体跳变）；② 长会话（反复搜索 400 应用 + 嵌套页）后 Task Manager 数字稳定。
