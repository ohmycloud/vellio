# Vellio 软件更新与发行

仓库：<https://github.com/ohmycloud/vellio>。更新源固定为此仓库的 GitHub Releases，不接收页面、模型回复或远程配置提供的其他源。

## 用户入口与当前边界

- 桌面端：Settings → **About / Updates**；设置底栏也有直达按钮；首页 **? → About Vellio / Updates**。
- Web 试验版：首页 **帮助与更新** 或 Settings → **About / Updates**。这里显示的是共用 Rust 核心的版本和桌面下载，不会自动替换正在运行的 Web 服务；Web 更新仍需停止服务、更新源码并重新编译。
- 显示当前版本、检查中、已是最新版、新版本与说明、暂无公开 Release、网络/限流错误。
- 自动检查默认开启。桌面启动时及运行期间按每日一次的缓存策略检查；Web 在进入 About 时按相同策略检查。失败后 3 小时才自动重试；手动 **Check for updates** 可绕过缓存与关闭自动检查的设置，但不改变自动检查开关。
- 只检查稳定版，跳过 draft / prerelease；按 SemVer 比较，不降级；不按字符串比较 `1.10.0` 与 `1.9.0`。
- 检查仅访问 GitHub 的固定公开 API，发送产品 User-Agent，不发送书籍、笔记、模型配置、API key 或账户凭据。GitHub 无认证 API 有限流，失败后可直接使用 All releases 链接。
- 更新配置和缓存位于本机设置位置旁的 `updates.json`，不放在可移动/云同步书库内。兼容路径：macOS `~/Library/Application Support/EasyReadGPUI/updates.json`，Windows `%LOCALAPPDATA%/EasyReadGPUI/updates.json`，Linux `~/.local/share/easyread-gpui/updates.json`。测试可通过现有 `EASYREAD_LOCATION_FILE` 将此位置隔离。

**本次是“检查 → 浏览器下载 → 退出后替换程序”，不是静默自更新。** 不执行远程安装命令，不自行提权，不覆盖正在运行的二进制，也不删除书库。

- macOS：下载 DMG，将 `Vellio.app` 替换到 Applications，退出旧程序后重新打开。
- Windows：下载 ZIP，退出旧程序，将完整包解压至程序位置。`Vellio.exe` 与 `pdfium.dll` 必须在一起。
- Linux：下载 TAR.GZ，退出旧程序后解压完整目录并启动 `Vellio/Vellio`。保留可执行权限；需要桌面会话、GPU/Vulkan 驱动及系统 X11/Wayland 等运行库，这不是完全静态的 AppImage。
- 书库位于独立的数据目录，替换程序目录即可，不要删除或用发行包覆盖书库。
- 源码开发运行与发行包使用相同检查功能；源码用户应更新源码并编译，而不是把下载文件写回源码目录。

没有对应平台/CPU 的包时，明确提示并提供 Releases 页面，不下载其他架构的包。当前工作流构建 macOS aarch64 / x86_64、Windows x86_64、Linux x86_64；检查器也识别 ARM64 Windows/Linux 或 macOS universal 的规范包名，但不会凭空声称这些包已构建。

## Release 资产约定

版本取 `VELLIO_VERSION` 编译环境变量，否则取根 `Cargo.toml` 的 `package.version`。标签工作流使用去掉 `v` 的标签作为 `VELLIO_VERSION`，并运行 `--version` 校验实际编入的版本。

```text
v0.1.0
  Vellio-0.1.0-macos-aarch64.dmg
  Vellio-0.1.0-macos-x86_64.dmg
  Vellio-0.1.0-windows-x86_64.zip
  Vellio-0.1.0-linux-x86_64.tar.gz
  上述各文件对应的 .sha256
```

选择器优先识别 macOS DMG / ZIP、Windows EXE / MSI / ZIP、Linux AppImage / TAR.GZ，均要求 `Vellio-{version}-{os}-{arch}.{extension}`。仅允许 HTTPS 的 `github.com/ohmycloud/vellio/releases/download/{tag}/{name}`，拒绝其他仓库、主机及不匹配的版本/架构。每个包包含匹配目标的 PDFium 及许可证；不将本机 ARM64 的 dylib 误放进 Windows/Linux/Intel 包。

`.sha256` 用于下载完整性核对，**不是**独立的发布者签名，也不是自动执行下载内容的信任依据。

## GitHub Actions

`.github/workflows/release.yml`：推送 `v*` 标签或手动执行 → 四平台原生 runner → 单元/打包契约测试 → 安装匹配 runner 的固定 `pypdfium2==5.13.0` wheel 中的 PDFium（构建工具，不把 Python 打进软件）→ `scripts/build_release.py` 体积优化编译 → 打包 → 上传 artifact。

标签构建全部成功后创建/补充 **draft Release**。先下载各包，在对应系统做 PDF/EPUB 导入、阅读、笔记及更新入口验收，补全发行说明，然后人工 Publish。草稿期间客户端不会收到更新。普通 `workflow_dispatch` 只生成 artifact，不发布 Release。

此代码在本机验证了版本/架构选择、原生 UI、Web 页面和模拟打包契约；**不能据此声称 Windows/Linux 原生包或 CI 已运行通过**。首次发行必须检查实际 runner 结果、签名、操作系统依赖与干净机器启动情况。Linux 使用 Ubuntu 22.04 构建以控制 glibc 基线，不保证更早系统可用。

## 发布构建的体积优化

参考 [Zed PR #65168](https://github.com/zed-industries/zed/pull/65168)（提交 `8e15c45afc57f8756bc030061ea9757ca8bc5af4`）：

- `.cargo/bundle-config.toml` 仅由发布脚本显式加载，不改变普通开发构建。应用 crate 使用 `opt-level = "z"`、`codegen-units = 1`，运行时依赖使用 `opt-level = 3`，保留 GPUI、Typst、PDF／图像处理等热路径的依赖性能优化。本项目只有一个应用 crate，因此采用包级覆盖，不照搬 Zed 的工作区通配覆盖，也避免将构建脚本和过程宏无谓地提升为 O3。去除调试信息，但不采用 `panic = "abort"` 或删除运行时资源来换取体积。
- macOS 用 `cargo rustc --bin easyread-gpui`，只向最终可执行文件传入 `-C link-arg=-Wl,-unexported_symbol,__R*`，不污染依赖或动态库链接。该模式针对 Rust v0 的 Mach-O 导出名，不隐藏 GPUI 所需的 Objective-C／C 符号；不是随意隐藏所有导出。
- Windows／Linux 只使用该发布 profile，绝不传入 Apple 链接器参数。
- macOS 打包额外在可执行文件的暂存副本上执行 `xcrun strip -x`，仅去掉本地符号表；Objective-C／C 外部符号仍保留，Cargo 原始产物不改写。`--keep-symbols` 可保留本地符号名用于诊断（不等于包含 DWARF 的调试包）。这是 Zed 构建／链接策略之外的额外减包措施；显式 `--dev` 的开发包不 strip。
- 字体、PDFium、MiTeX 和许可证继续完整保留，库文件也不被当作可执行文件 strip。strip 必须在签名前完成，失败则停止打包；签名、公证和校验和生成顺序不变，签名后不再改写二进制。

```sh
VELLIO_VERSION=0.1.0 python3 scripts/build_release.py --target aarch64-apple-darwin --jobs 2
python3 scripts/package_release.py --target aarch64-apple-darwin --version 0.1.0 --pdfium-dir assets/pdfium
# 本机 .app 默认使用相同优化，仍为 ad-hoc 签名；--release 也是相同路径：
sh scripts/package_macos.sh
# 更快构建、体积更大的开发包必须显式选择：
sh scripts/package_macos.sh --dev
```

本机打包入口以前不带参数会选择 debug，容易让已经存在的 release 优化失效。现在默认 release，并在 strip 后签名、严格校验，再输出构建模式和最终大小；构建／strip／校验失败不会报告打包成功。`profile.dev.debug = 0` 只是不生成 DWARF 调试信息，不等于优化代码或清除本地符号表。普通 `cargo build` / `cargo build --release` 的配置不变，首次通过本机脚本打 release 包会花更多编译时间。不修改 `/Applications` 中的现有安装或用户书库。

Intel macOS 及 Windows／Linux 必须使用匹配 target 的 PDFium wheel，不能沿用上述本机 ARM64 的 `assets/pdfium`。对比时分别报告可执行文件、展开后的 `.app` 和压缩安装包体积，并以相同源码的普通 release 为基线，不把 debug → release 的缩减冒充为上述优化效果。

### 当前版本：排除误打 debug 包的实测

GPUI Kit 0.8.0、macOS ARM64、Rust 1.99.1。现场旧 `target/Vellio.app` 的文件总大小为 **322,732,305 bytes（322.73 MB / 307.78 MiB）**，其中主程序 **315,174,704 bytes**，PDFium **7,191,008 bytes**。旧脚本不带参数会将 `target/debug/easyread-gpui` 放进包，这是约 323 MB 的主因，不是图标或许可证太大。

用当前源码经默认打包入口重新生成的 release `.app` 为 **131,250,705 bytes（131.25 MB / 125.17 MiB）**，主程序 **123,693,104 bytes**，PDFium 大小不变；整个包缩小 **59.33%**。新 DMG 为 **72,234,111 bytes（72.23 MB / 68.89 MiB）**。此处是现场 debug 旧包与当前 release 的交付大小对照，不能冒充相同源码下的新编译参数优化基准，也不代表内存、启动速度或帧率收益。

- 本机 app：`target/Vellio.app`。
- DMG 与 SHA-256：`target/package-size-current/Vellio-0.1.0-macos-aarch64.dmg` 及同名 `.sha256`。
- 现场测量：`target/package-size-current/report.json`；这些均是忽略的本地构建产物。
- 发布构建／打包契约测试 **17 项通过**，涵盖默认 release、显式 `--release`／`--dev`、参数拒绝、strip 不修改 Cargo 原产物、签名前 strip、构建失败保留旧包及 strip／签名校验失败不误报成功。
- 实际挂载本次 DMG 后，校验 SHA-256、严格签名完整性、`--version`、中文分式／根号／求和 SVG、PDF/EPUB 导入、原生窗口启动冒烟均通过。测试从工程目录外运行，使用隔离 HOME 和合成测试书库，关闭自动更新，不调用付费模型、不打开用户书库；QA 进程及挂载均已结束。
- 字体、PDFium、MiTeX/Typst、代码语法高亮和许可证未删除；未改全局 Cargo release 配置、未使用 `panic = "abort"` 或静默启用 GPUI Fast。尚未做完整手工阅读验收、跨平台验证、Developer ID 签名或公证，也未覆盖 `/Applications` 中的旧安装。

### 此前 release 优化实验与验收边界

以下为此前发布优化实验的历史记录，不是当前版本的重新测量，也不是将 debug → release 的差值算作发布优化收益。

macOS ARM64、Rust 1.99.1，以同一份源码的普通 release 为基线；两组使用同一个 PDFium、图标、许可证及 ad-hoc 签名流程，不把开发构建体积混入对比。可执行文件 / 展开 `.app` / 压缩 DMG 分别从 **140.31 / 147.37 / 73.52 MiB** 降到 **116.69 / 123.75 / 68.45 MiB**，缩减 **16.83% / 16.03% / 6.90%**。仅 Zed 构建／链接调整的可执行文件为 136.06 MiB，其余来自本地符号表剥离。

最终文件仍保留基线的全部 15 个非 Rust v0 外部符号，Rust v0 导出数由 14,134 降到 0。DMG 挂载后通过严格签名校验、`--version`、含中文的分式／根号／求和 SVG 导出、PDF／EPUB 导入和原生窗口启动冒烟；SHA-256 核验通过。构建／打包契约测试包含 strip 先于签名、失败不签名、不修改 Cargo 原始产物和保留符号选项。

该次实验的本地明细输出到 `target/package-size-qa/report.json`，包输出到 `target/package-size-qa/optimized-stripped/`；这些是可清理的、忽略的构建产物，不进入源码仓库。窗口启动冒烟不代替完整 UI／性能基准；Windows、Linux、Intel macOS 的真实构建和启动仍待对应 runner 验收。这里没有使用发行证书或进行 Apple 公证，也没有覆盖已安装的应用和用户书库。

## macOS 签名：参考 EasyRead，但不照搬 Electron 更新器

EasyRead 的 `scripts/mac_sign.cjs` 将固定 P12 临时导入独立钥匙串、签完恢复钥匙串列表并删除临时证书；这里的 `scripts/mac_sign.py` 沿用该隔离方式，先签 PDFium 再封装 app，并执行 `codesign --verify --deep --strict`。

GitHub repository Settings → Secrets and variables → Actions：

| Secret                        | 用途                               |
| ----------------------------- | -------------------------------- |
| `MAC_SIGN_P12`                | 固定签名证书和私钥的 P12，base64 编码         |
| `MAC_SIGN_P12_PASSWORD`       | P12 导出密码                         |
| `MAC_SIGN_IDENTITY`           | 证书完整名称；未设时为 `Vellio Self-Signed` |
| `APPLE_ID`                    | 可选：Apple Developer 账户，公证用        |
| `APPLE_APP_SPECIFIC_PASSWORD` | 可选：公证专用密码                        |
| `APPLE_TEAM_ID`               | 可选：公证 Team ID                    |

三项 Apple 凭据必须全设或全不设。使用 `Developer ID Application: …` 证书时启用 hardened runtime / timestamp；有完整公证凭据时通过 `notarytool` 上传、等待、公证后 staple。正式签名/公证过程必须在拥有凭据的 macOS runner 验收。

**自签名证书不等于 Apple Developer ID 或公证**，不能承诺绕过 Gatekeeper。参考 EasyRead 可以使用固定自签名证书，但普通用户无安全警告的发行应使用 Developer ID 和 Apple 公证。若复用已有 EasyRead P12，须显式把 `MAC_SIGN_IDENTITY` 设为那张证书的真实名称，不能仅改应用名字假装新证书。不要将 P12、密码、私钥、钥匙串提交到 Git。

标签发行时缺少 `MAC_SIGN_P12` 会失败，不会悄悄发布临时签名包。手动/本机开发打包可退回 ad-hoc 签名。GPUI 不是 Electron；本次没有使用 Squirrel、`electron-updater`，也没有依靠“新旧证书相同”实现二进制自动覆盖。

## Windows 签名与 Linux

可选 Secrets：`WINDOWS_SIGN_PFX`（base64 PFX）、`WINDOWS_SIGN_PFX_PASSWORD`。存在时用 Windows SDK `signtool` 对 `Vellio.exe` 做 SHA-256 Authenticode 签名并验证，再压 ZIP。没有证书时生成未签名便携包，Windows 可能显示 SmartScreen 提示；本项目不自带或伪造受信任的 Windows 发行证书。

Linux TAR.GZ 当前附 SHA-256 校验文件，不冒充具备发行版包管理器签名或签名自动更新的 AppImage。

## 本地测试

```sh
cargo test --locked --lib updates::tests
RUST_MIN_STACK=16777216 cargo test --locked --features desktop-ui-tests --bin easyread-gpui updates_tests
python3 -m unittest discover -s scripts -p 'test_release*.py'
cargo test --locked --manifest-path web-topcoat/Cargo.toml
cargo build --locked --manifest-path web-topcoat/Cargo.toml
python3 -m unittest discover -s scripts -p 'test_web_updates.py'
cargo run --locked -- --version
```

测试的 HTTP 服务是随机 loopback 端口的显式桩，不访问真实 GitHub；UI 测试注入固定 Release，不打开浏览器、不调用安装器、不写用户设置。真实 GitHub 发布检查可能受网络或 API 配额影响，不把此类失败当“没有新版本”。
