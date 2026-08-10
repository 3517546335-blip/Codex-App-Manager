# Roadmap & 验收清单

> 配套 [`product-design.md`](./product-design.md)。状态：✅ 完成 · 🟡 进行中 · ⬜ 未开始。
> 本轮已完成 **macOS 更新引擎全链路**（真实数据逐环验证）。其余按工作流列出，每项带验收标准。

## 状态总览

| 工作流 | 状态 | 说明 |
|---|---|---|
| macOS 更新引擎（plan→download→verify→apply→gate→swap→rollback） | ✅ | 全链路真机/真数据验证，9 单测 + 3 个 demo bin |
| macOS Tauri 命令 + 最小前端 | ✅ | `mac_plan_update` / `mac_stage_update` + 面板（tsc+vite 构建通过） |
| macOS live 编排（真 `/Applications`） | ✅ | `perform_macos_update` 串接 reconstruct→gate→quit→原子替换→健康检查→relaunch/rollback；守卫命令 `mac_perform_update`（强制 `confirm`）+ UI 二次确认 + full-zip 解包（含单测）；BinaryDelta 运行时定位 + 发布 vendor 接线 |
| mirror 服务端（appcast/zip/delta 镜像 + manifest v3） | ⬜ | mac 增量上线的前置 |
| Windows 全链路 | ⬜ | **尚未开始**；设计见 §5，拷问提示词已备 |
| manager 自更新 + 分发 | 🟡 | **客户端已接**（updater+process 插件 + UI）；服务端 latest.json/托管/签名 TODO |
| provenance / 纳管 UX（mac） | ✅ | 存储 + managed/external/none 分类 + 显式同意纳管 + UI |
| `~/.codex` 边界 | ⬜ | 仅卸载时保留/清除 |

---

## 1. macOS live 编排（接真 `/Applications`）✅
把已验证的零件接到真实安装根。
- [x] `app/mac_update.rs` 加 `perform_macos_update`：detect 基准 → `download_and_verify`（EdDSA 钉死）→ `apply_delta`（delta）/ `unpack_app_zip`（full）→ `gate_reconstructed`（codesign/Team=OpenAI/Gatekeeper）→ `quit_codex`（绝不强杀）→ `swap_in_place`（同卷原子）→ 文件系统健康检查 → 成功 `relaunch` / 失败 `rollback`。
- [x] BinaryDelta 运行时定位（`commands::resolve_binary_delta`：`CODEX_BINARY_DELTA` 覆盖 → 包内 `resources/BinaryDelta`）+ `bundle.resources` 接线 + 发布 vendor 说明（`src-tauri/resources/README.md`，二进制不入库、CI/发布时落入）。
- [x] staging 落在**与安装根同卷**（`TMPDIR` 与 `/Applications` 同 `dev`，已实测）；跨卷由 `swap_in_place` 守卫拒绝。
- [x] Tauri 守卫命令 `mac_perform_update`：强制 `confirm: true`（否则后端拒绝）+ 前端 `window.confirm` 二次确认；仅在**真实**验签暂存后可用（模拟 build 预览态禁用真实替换）。
- [x] 全量路径：无 delta 时 `*.zip` →（`ditto -x -k` 保签名元数据）解包 → 取出 `.app` → 同一 gate/swap 编排（`unpack_app_zip` + `find_dot_app`，含 macOS 单测）。
- [x] 成功后写 provenance（`manager-installed`），安装自动转为「manager 托管」。
- **验收**：破坏性原语（gate→quit→swap→rollback→relaunch）已在真实 `/Applications` 与真实重建 bundle 上逐环验证（`mac_live_test` / `mac_rehearse`）；`perform_macos_update` 以相同顺序复用这些原语并通过编译 + full-zip 解包单测。🟡 **遗留**：本机已是最新（3575），真正「旧版→最新」的一键全程未在本机重跑（需可控旧版测试机；逻辑与素材级证明均已具备）。

## 2. mirror 服务端（mac 增量前置）⬜
见 [`product-design.md` §4.4](./product-design.md)。
- [ ] 镜像 `appcast.xml`：**重写 enclosure URL** 指向 R2/S3，**保留 EdDSA 签名原样**。
- [ ] 镜像全量 `Codex-darwin-<arch>-<ver>.zip`（latest）+ 最近 ~5 版 `*.delta`。
- [ ] arm64 与 x64 对称处理；预留 beta 通道结构。
- [ ] manifest 升 v3：`manager.payloads` + `manager.fileManifests`（Windows β 用）+ `channel` 维度 + `minManagerVersion`。
- [ ] download-router 增加 appcast/zip/delta 路由（CN→S3 预签名，其他→R2）。
- **验收**：manager 把 appcast 源切到 mirror 域后，CN 网络可达；`mac_plan_update` 读 mirror appcast 得到与官方一致的 delta 计划；EdDSA 验签仍通过（签名未被破坏）。

## 3. Windows 全链路 ⬜（**尚未开始**）
设计见 [`product-design.md` §5](./product-design.md)；真机验证提示词见本仓 issue/笔记（已交给 Windows 助手）。决策已锁：自动择优、不提权不改策略、便携非侵入补偿、信任锚=Authenticode 发布者=OpenAI。
- [ ] 能力检测：App Installer / `AllowAllTrustedApps` / AppXSvc / `Add-AppxPackage` 可用性 → 判定映射。
- [ ] MSIX 侧载路径（开箱可用时）：侧载 + 同签名新版就地升级 + 失败精确识别后回退。
- [ ] 便携路径（保底）：解包 MSIX→`%LOCALAPPDATA%\Programs\Codex`→跑 exe；HKCU 补偿（开始菜单/卸载项/provenance）。
- [ ] 运行中替换：关闭 Codex→替换→重启；处理文件占用。
- [ ] 信任锚：`WinVerifyTrust` 校验发布者=OpenAI。
- [ ] metered 网络检测（WinRT）供静默下载护栏。
- [ ] 增量 α→β：全量起步 + manifest 预埋逐文件哈希；β 按中央目录 Range 抽取变化条目。
- **验收**：①普通个人机（商店被墙）能侧载装/更；②锁定机能便携装/更且登录正常；③两类都能正确检测已装版本并只在有新版时更新；全程不提权、不改系统策略。

## 4. manager 自更新 + 分发 ⬜
见 [`product-design.md` §8](./product-design.md)。
- [ ] Tauri updater 接 `…/manager/latest.json`（minisign 签名，提示后更新）。
- [ ] 安装包托管 R2/S3 + GitHub Release + `agentsmirror` 落地页。
- [ ] Mac 公证（已有证书）；Win 暂不签 → 落地页 SmartScreen 绕过指引 + 自证。
- **验收**：CN 网络能下到 manager 并完成一次自更新；Mac 无 Gatekeeper 拦截。

## 5. 横切 ⬜
- [ ] provenance store（bundle 之外）：记录 managed/external + 来源，驱动纳管。
- [ ] 纳管 UX：检测外部安装 → 显式同意接管流。
- [ ] `~/.codex`：仅卸载时"保留/清除"；其余预留不做。
- **验收**：发现官方安装能在用户确认后纳管；卸载默认保留 `~/.codex`，可选清除。

---

## 已交付的可运行验证
- `cargo test -p codex-mac-engine` → 9 passed（appcast/plan/verify/swap，含 `swap_and_rollback_roundtrip`）。
- `cargo test`（manager）→ 5 passed，含新增 `unpack_app_zip_surfaces_bundle` / `find_dot_app_prefers_codex`（full-zip 解包分支）。
- `cargo run -p codex-mac-engine --bin mac_plan -- 3511` → 真 appcast 出 delta 计划（省 95.5%）。
- `cargo run -p codex-mac-engine --bin mac_fetch -- 3511` → 下真 18MB delta + EdDSA 验签通过。
- `cargo run -p codex-mac-engine --bin mac_rehearse` → 真 bundle 沙盒彩排 gate→swap→rollback。
- `cargo run -p codex-mac-engine --bin mac_live_test` → 真 `/Applications/Codex.app` 上 gate→（退出）→原子替换→（重启）。
- `npm run build` → 前端 tsc+vite 构建通过（含「应用更新」二次确认 + 应用结果卡片）。
- `cargo clippy --all-targets -- -D warnings` → manager + engine 两 crate 均干净。
- `codex review --base <pre-slice>` → 8 轮对抗式 review 收敛至「无阻断问题」；逐轮修复：重启前不删备份、退出前预检同卷+失败重启、确认文案用真实路径、sim 变更作废暂存、未检测安装禁用应用、缺工具/应用失败双重回退全量、确认目标 TOCTOU 校验、自更新与 perform 串行化、provenance 保存失败上报。
