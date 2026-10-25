# 贡献指南

> 语言：中文为主（与仓库文档一致）；提交信息用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)（`feat:` / `fix:` / `docs:` / `ci:` / `refactor:` …，CHANGELOG 由提交历史归类）。

## 环境搭建

1. 安装 Rust（任意近期版本即可）——仓库根的 `rust-toolchain.toml` 会自动钉扎 **1.95.0**（即 `rust-version` MSRV），clone 后首次 `cargo` 命令触发自动安装，无需手动步骤。
2. Linux / macOS：clone 后 `cargo test` 开箱即用（CI 同款命令；§5.7 起 `config.toml` 不再钉扎 build.target，构建直接用宿主三元组）。
3. **Windows 本地日常构建**需要 gnu 宿主工具链覆盖（默认按 channel 解析出的 msvc 工具链无链接器——本机未装 VS Build Tools；与 CI Windows job 同款变量）：

   ```bash
   rustup toolchain install stable-x86_64-pc-windows-gnu
   RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu cargo build
   # 或等价的 cargo +stable-x86_64-pc-windows-gnu build
   ```


## 提交前三关（与 CI 逐字一致）

CI（`.github/workflows/ci.yml`）是唯一门禁口径，本地请用相同命令：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

三平台（windows / linux / macOS）clippy 与 test 均为阻塞门禁；Windows job 另跑 `node tools/docscan.mjs`（文档一致性：链接可达 + INDEX 行数 + 引用路径存在性）。

注意：三关与 CI 均为 debug profile——**「全绿」≠「出得了包」**，交付/发布/体积测量前实跑 `bash tools/package.sh`。

改文档后 INDEX 行数无需手改：`node tools/docscan.mjs --write-index` 自动回填（CI 门禁仍用严格模式）。

## 仓库纪律

- **`Cargo.lock` 零新增**：不加新依赖；确需引入时在 PR 说明理由与体积影响。
- **行尾 CRLF**：仓库无 `.gitattributes`，新文件请保持 CRLF（`git diff` 出现整文件重写说明行尾被刷，须还原）。
- **文档同步**：`docs/INDEX.md` 是全部文档的唯一导航；行为变更须同步对应契约文档（`docs/protocol.md` / `docs/manifest-schema.md`）与 `CHANGELOG.md`（只加新条目，历史条目不改）。
- **扩展开发**：见 `docs/extensions.md` 与 `examples/python-minimal/`；清单 schema 见 `docs/manifest-schema.md`，协议见 `docs/protocol.md`。

## 提交 / PR

- 提交信息：Conventional Commits，主题行一句话说清「做了什么、为什么」。
- PR 保证三关全绿；涉及发布产物（`tools/package.sh`、`release.yml`）的改动请说明对产物集合 / 体积的影响。
- 安全相关问题**不要**走公开 issue——见 [SECURITY.md](./SECURITY.md)。
