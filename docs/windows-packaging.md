# Windows 打包

在仓库根目录执行：

```powershell
.\scripts\package-windows.ps1
```

脚本按现有 CI 流程构建前端和 Rust release 二进制，先暂存到 `dist/windows/app`，再调用现有 NSIS 脚本生成安装包。版本号默认读取 `tauri.conf.json`。

输出文件：

```text
dist/windows/CodexPlusPlus-<版本>-windows-x64-setup.exe
```

只重新封包、不重新编译时使用：

```powershell
.\scripts\package-windows.ps1 -SkipBuild
```

前置条件：Windows x64 环境，Node 依赖已安装，Rust stable 可用。脚本自动查找 `makensis.exe`，包括 Tauri 本地缓存，不需要单独配置 PATH。用户级环境变量使用 `RUSTUP_HOME=E:\rust\rustup` 和 `CARGO_HOME=E:\rust\cargo`；新开的终端会自动继承，旧终端重新打开后生效。
