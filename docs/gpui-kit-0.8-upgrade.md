# GPUI Kit 0.8.0 桌面迁移

## 版本与范围

依据官方 [发布历史](https://github.com/longbridge/gpui-kit/releases)，逐项核对
[0.7.0](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.0)、
[0.7.1](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.1)、
[0.7.3](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.3) 和
[0.8.0](https://github.com/longbridge/gpui-kit/releases/tag/v0.8.0)。

根 `Cargo.toml` / `Cargo.lock` 从 GPUI Kit 0.6.6 升级并锁定到 **0.8.0**。
`gpui-base`、`gpui-component` 同步到 0.8.0；默认平台后端按该版本的上游
精确依赖使用 **gpui-pre 0.3.9**，不是自行替换为 0.3.10。

没有启用可选 GPUI Fast、JavaScript Shell 或 WebView。锁文件及 vendored
Base 的独立开发依赖可能包含 GPUI Fast，不代表桌面应用启用了它；以
`cargo tree --locked --features desktop` 的实际依赖树为准。

本次不修改书库 schema、PDF/EPUB 原文件、批注 ID、修订和阅读进度格式；
已有书籍不需要重新导入。Topcoat Web 子项目仍使用自己的 manifest / lock。

## 应用接入与优化

- **窗口入口**：使用 `gpui_kit::open_window`，由 Kit 自动包裹 Base `Root`。
  保留 `gpui_kit::init`、应用自己的关闭处理、快捷键和后台服务，不再在生产
  启动路径手工再包一层 Root。新版 Root 托管弹层、焦点遍历和选择复制。
- **主题更新**：字体设置改用 `Theme::update`，统一同步 Component 与 Base
  主题并刷新窗口，避免手工更新两个主题投影。等宽字体按实际安装情况选择
  JetBrains Mono / Cascadia Mono / Consolas / Menlo / DejaVu Sans Mono /
  Liberation Mono，最后才回退系统字体。
- **阅读排版**：改用上游按标题级别调用的 `with_heading` 闭包，保留现有
  正文、标题、题注、双语 companion 的字号、字重、行距和缩进。
- **无障碍**：阅读设置中的字号、行宽、行距滑块提供 `accessibility_label`。
- **减少空刷新**：AI/PDF 后台工作器的 80ms 轮询在没有新事件且通道未关闭时，
  不再进入 UI entity 更新并调用 `notify`；真正的进度、流式增量、错误和完成
  仍成批交付。400ms 输入/阅读位置保存逻辑不变。
- **复用上游改进**：保留 0.8 的段落/行内缓存、不可变已绘制复制投影、流式
  revision 协调、减少静止帧写状态、输入/焦点/菜单/Tooltip 的修复。已有变高
  虚拟聊天列表和阅读分页不重写。没有声称特定滚动帧率或 CPU 降幅。

## 本地补丁重基

### `vendor/gpui-base` → 0.8.0

以已发布的 0.8.0 源码为基础，迁回现有批注颜色/ID/点击与行位置、段落几何、
软换行选区 affinity、原生公式及 inline 对象、带标记代码、companion 样式和
图片比例等扩展；不是只改版本号继续使用旧渲染器。

实际回归发现并修复：

1. **HTML 段落缓存身份**：HTML 没有 Markdown source-span key，原先共用的
   `"p"` ID 在新版 retained flow 缓存下会让不同段落交换布局、遗漏公式或批注，
   甚至重复消费只能绘制一次的公式元素。现在按 retained paragraph state
   提供稳定且互不冲突的 ID，并增加独立身份测试。
2. **切换语言后拖选**：布局 revision 的确认改读当前 selection snapshot，
   避免订阅延迟留下的旧 `is_selecting` 标志在下一次松手时清空新选区。
   原有真实鼠标拖选用例保留原坐标与“必须为段内部分文字”的断言。
3. **软换行复制**：不可变已绘制布局保留本地 downstream affinity，软换行
   第一字仍可选中；线性行边界计算只对硬换行预留额外宽度，与逐字符计算一致。
4. **HTML 换行及代码**：沿用 0.8 `<br>` 的行内/末尾语义；`<pre>` 保留本地
   原生 CodeBlock 的语言、空白、颜色、批注与内嵌图片，并修复 minifier 在
   嵌套 `<code>` 首行重复插入换行的合并问题。

详细补丁与回归命令见
[`vendor/gpui-base/EASYREAD-PATCH.md`](../vendor/gpui-base/EASYREAD-PATCH.md)。

### macOS 字体补丁 → `vendor/gpui-pre-apple` 0.3.9

新版把 CoreText 栅格化从 macOS crate 移到共享 Apple 后端，因此旧的
`vendor/gpui-pre-macos` 0.3.6 源码已移除，应用改 patch Apple crate；实际 macOS
平台仍由 registry 的 `gpui-pre-macos` **0.3.9** 提供。

`web-font-smoothing` 继续显式关闭 CoreGraphics 字体增粗和颜色相关 dilation，
保留灰度抗锯齿、正常字形度量、布局和 emoji 行为；不写系统偏好。Standalone
manifest 显式启用 Metal 像素缓冲锁定所需 CoreVideo `CVBase`，使原生字体测试
无需依赖应用其他 crates 的 feature unification。

详细说明见
[`vendor/gpui-pre-apple/EASYREAD-PATCH.md`](../vendor/gpui-pre-apple/EASYREAD-PATCH.md)。

## 验证

本机 macOS ARM64 / Rust 1.99.1，升级时完整工作区（含此前已有的未提交功能）：

- `cargo build --locked --features desktop`：通过。
- `cargo test --locked --all-targets --features desktop`：**274 通过，12 ignored，
  0 失败**；依赖外部实书的 ignored 用例未计入通过数。
- `RUST_MIN_STACK=16777216 cargo test --locked --features desktop-ui-tests --bin easyread-gpui -- --test-threads=1`：
  **107 通过，6 ignored，0 失败**。覆盖批注/拖选、公式、标题/题注、内联编辑、
  EPUB 页表稳定性、PDF 排版、聊天/笔记、模型菜单、导航及主题切换。
- vendored Base 独立 `--features test-support --lib`：**1469 通过，0 失败**。
- Apple `web_antialiasing` 原生测试：**2 通过**；检查深浅文字零 dilation，
  以及真实 Charter 字形 1×/2× 的实心内部与抗锯齿边缘。
- `cargo clippy --locked --all-targets --features desktop -- -D warnings`：通过。
- `cargo test --locked --manifest-path web-topcoat/Cargo.toml`：**19 通过**。
- `git diff --check`：通过。

完整 `cargo fmt --check` 仍报告本次升级前工作区里 PDF 与部分桌面测试文件的
格式差异；未为此批量格式化无关的未提交工作。此次迁移没有重置已有改动。
构建另有上游 `block 0.1.6` future-incompatibility 提示及 debug 大二进制的
`__eh_frame` linker 警告，均未导致构建/测试失败。

这些是本机自动化绘制和事件回归，不等同于 Windows/Linux 实机验收、发行
签名/公证或完整的人工交互验收，也未做升级前后的端到端性能基准。

### 提交隔离验证

为避免把此前的 Web、AI、PDF 与打包改动一并提交，窗口接入等混合文件按
本次升级的改动单独暂存，并从暂存区导出独立快照验证：

- `cargo check --locked --features desktop`：通过。
- 桌面 UI 测试：**97 通过，2 ignored，0 失败**；数量不含上述仍未提交功能的
  新增用例。旧的首页图标测试引用了已被 Git 忽略的 SVG 设计稿，该既有测试
  素材仅复制到临时验证目录，没有在本次升级中提交整个设计稿目录。

## 运行

退出旧桌面进程，在项目根目录重新编译运行：

```sh
cargo run --locked --features desktop
```

若使用之前打出的 `.app`，它仍是旧二进制；需要重新运行
`sh scripts/package_macos.sh` 生成本地 app。无需迁移或重新导入书籍。
