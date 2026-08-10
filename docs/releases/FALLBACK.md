<p align="center">
  <a href="https://codexapp.agentsmirror.com">
    <img src="https://raw.githubusercontent.com/Wangnov/Codex-App-Manager/main/assets/banner.svg" alt="Codex App Manager" width="100%">
  </a>
</p>

## 📦 安装与升级 · Install & Upgrade

**已经安装?** 打开“设置 → 关于”,点击“检查管理器更新”即可升级。
**Already installed?** Open Settings → About and select Check for Manager Updates.

| 平台 · Platform | 下载 · Download |
| --- | --- |
| macOS · Apple Silicon | 在下方 Assets 选择名称含 `macOS_aarch64.rar` 的文件 |
| macOS · Intel | 在下方 Assets 选择名称含 `macOS_x86_64.rar` 的文件 |
| Windows · x64 | 在下方 Assets 选择名称含 `Windows_x64.rar` 的文件 |

**Windows 签名状态:** Windows x64 安装包当前没有 Authenticode 代码签名,首次运行可能出现 SmartScreen 提示。
**Windows signing status:** The Windows x64 installer is not Authenticode-signed yet, so SmartScreen may warn on first run.

**核验下载:** 本页 Assets 带有 `SHA256SUMS`;Windows 用 `Get-FileHash .\CodexDesktopManager_x64-setup.exe -Algorithm SHA256`,macOS 用 `shasum -a 256 CodexAppManager_aarch64.dmg`,再与 `SHA256SUMS` 比对。
**Verify downloads:** This release includes `SHA256SUMS`; on Windows run `Get-FileHash .\CodexDesktopManager_x64-setup.exe -Algorithm SHA256`, and on macOS run `shasum -a 256 CodexAppManager_aarch64.dmg`, then compare with `SHA256SUMS`.

> RAR 均无密码且只包含对应安装器；`.app.tar.gz` / `.sig` / `latest.json` 是自动更新器工件。
> RAR files are password-free and contain one installer; `.app.tar.gz` / `.sig` / `latest.json` belong to the auto-updater.
