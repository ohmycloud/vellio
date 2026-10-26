# 项目结构与维护指南

项目分为共享 Rust 阅读引擎、原生 GPUI 应用和独立 Web 实验三个入口。本次整理只调整源码组织、模块可见性和测试位置，不修改书库 schema、持久化 ID、业务流程或界面设计，也不引入新的依赖或额外 crate。

## 目录导航

```text
src/
├── lib.rs                 # 共享引擎入口与兼容的公开模块导出
├── main.rs                # CLI 参数、无界面任务与桌面启动入口
├── app/                   # 全局偏好、单书会话、版本与更新
├── reader/                # 阅读数据模型、布局、导航、段落编辑与翻译配对
├── library/               # 书库/工作区持久化、操作日志、首页目录数据
├── ai/                    # 模型连接、身份、批量翻译、带上下文的对话
├── annotations/           # 引文定位、源 HTML 修饰、高亮及导航锚点
├── rendering/             # 字体、公式、代码高亮、图片与原生表格
├── formats/
│   └── epub/              # 有界离线 EPUB 导入与安全 XHTML 转换
├── pdf/                   # PDFium 生命周期、导入、图片与阅读呈现 API
│   └── recovery/          # 按字形/几何证据恢复文本、标题、链接、公式与表格
└── desktop/               # 仅 desktop feature 启用的 GPUI 界面
    ├── mod.rs             # 私有 ReaderView 状态、事件类型与模块声明
    ├── bootstrap.rs       # 资源、字体、平台集成与窗口启动
    ├── init.rs            # 视图构造与订阅
    ├── tasks.rs           # 后台任务和 UI 线程事件回传
    ├── state.rs           # 状态刷新、会话恢复与修订保存
    ├── view.rs            # 阅读界面顶层组合
    ├── text.rs            # 保留式富文本视图与阅读 HTML 组合
    ├── blocks.rs          # 正文块与列表行渲染
    ├── pdf_loading.rs     # PDF 排版预取、队列与图片加载
    ├── source_navigation.rs / navigation_menu.rs
    ├── selection.rs / annotation_editor.rs / margin_editor.rs
    ├── note_actions.rs / note_cards.rs / note_navigation.rs / notes_panel.rs
    ├── home/              # 书库首页：状态、动作、行、工具栏、菜单、页面
    ├── settings/          # 设置：生命周期、模型、阅读、侧栏、书库、快捷键
    ├── chat/              # AI 面板：状态、动作、呈现、尺寸交互
    └── tests/             # 桌面交互与渲染回归

tests/
├── rust_core.rs           # 引擎集成回归入口，不再承载全部测试正文
├── core/                  # AI、存储、公式、PDF 文本/表格/图片/图注等回归
├── support/               # 按文本/表格/图片/文档分类的共享 PDF 夹具
└── *.rs                   # EPUB、安全、设置、翻译等独立集成测试

web-topcoat/src/
├── main.rs                # 服务启动、共享请求/显示类型和应用状态
├── document.rs            # HTML 清理与阅读显示块转换
├── document/tests.rs      # 文档转换与安全清理回归
├── routes/                # 首页、阅读、批注、AI、译文、静态资源 HTTP handler
└── *.rs                   # 模型设置、阅读控件、目录、公式、图片等现有组件
```

桌面原有的 `code`、`epub`、`math`、`reading_mode`、`typography`、`updates` 等专门模块也位于 `src/desktop/`。它们与新拆出的模块共同组成界面层，不是共享引擎的依赖。

## 依赖边界与兼容性

```text
CLI main.rs ──────────┐
GPUI desktop/ ────────┼──→ lib.rs 公开门面 ──→ 共享引擎各领域模块
Web Topcoat ─────────┘
```

- `lib.rs` 保留既有公开路径，例如 `easyread_gpui::model`、`store`、`settings`、`formula` 和 `pdf_layout_cache`。`app/`、`reader/`、`library/`、`rendering/` 是私有实现目录，通过显式重导出提供原接口；调用方不需要跟随文件移动修改 import。
- 共享引擎不依赖 GPUI 视图，也不依赖 Topcoat。跨桌面、CLI 和 Web 的规则放在共享引擎，不要复制到两个前端。
- 桌面子模块以 `impl ReaderView` 承载聚合后的界面职责。`ReaderView` 暂时仍是一个私有状态所有者，避免在结构重构中同时改写事件、订阅和状态生命周期。共享实现使用父模块导入；方法只开放到 `desktop` 内部，不将视图实现变成公共 API。
- `home/`、`settings/`、`chat/` 各自保留状态与共享样式，子文件负责相应动作或 UI。不要让这些模块重新成为新的大文件，也不要为了跨界面调用而把内部字段改成全局公开。
- 模块入口负责声明、共享类型和对外导出，业务实现放进有明确名字的子模块。不要用 `include!` 把大文件伪装成小文件，也不要用 `part1.rs`、`misc.rs` 或通用 `utils.rs` 代替职责划分。

## PDF 与 EPUB 的特殊约束

### PDF

`pdf/mod.rs` 是公开门面：

| 模块 | 职责 |
| --- | --- |
| `runtime.rs` | 唯一 PDFium 实例和串行访问锁 |
| `import.rs` | 原文导入、旧书图片补全、原页裁剪 |
| `presentation.rs` | 呈现 DTO、单页/批次读取与原文一致性校验 |
| `outline.rs` | 本地书签与目标定位 |
| `geometry.rs` | 裁剪、背景、文本面板和可见区域 |
| `figures.rs` | 图片区域、标签、图注关联与位图比例 |
| `layout_cache.rs` | 可丢弃、可重建的阅读呈现缓存 |
| `recovery/pipeline.rs` | 协调字形提取和各识别器，形成阅读呈现 |
| `recovery/{headings,paragraphs,links,images,typography,...}.rs` | 单一职责的几何识别和 HTML 恢复 |
| `recovery/math/`、`recovery/table/` | 数学结构与表格的专门算法 |

所有 PDFium 句柄仍留在串行工作范围内。新增读取入口复用同一锁，不要在某个识别模块另建 PDFium 实例。阅读恢复和缓存只影响显示，不得顺便重写源文本、批注、修订或原文件。

### EPUB

- `archive.rs`：ZIP 资源大小限制、路径校验、实体规范化和离线 XML 解析。
- `markup.rs`：允许列表内的 XHTML、Markdown 源文及图注识别。
- `content.rs`：遍历内容、稳定 block/目标定位、提取本地图片。
- `mod.rs`：包结构、spine/nav 组织和公开导入/源文接口。

新增 EPUB 标签或资源支持时，安全校验不能绕过 `archive` 和 `markup`，也不能执行出版方脚本或远程 DTD。

## 新代码放在哪里

| 需求 | 优先位置 |
| --- | --- |
| 阅读 schema、正文块、文本哈希 | `reader/model.rs` |
| 书库文件、事务、reader 操作与迁移 | `library/store.rs` |
| 首页排序/筛选/状态等可共享规则 | `library/library_home.rs` |
| 引文定位、高亮、数学/链接源文修饰 | `annotations/` |
| 翻译配对、段落修订和原文/译文布局 | `reader/` 中的对应模块 |
| AI 传输、批次策略、对话持久化 | `ai/` |
| 字体、公式向量排版、代码高亮 | `rendering/` |
| 窗口事件、菜单、按钮与绘制 | `desktop/` 中对应的 UI 模块 |
| Web HTTP handler | `web-topcoat/src/routes/` |
| Web 输入 HTML 清理和显示转换 | `web-topcoat/src/document.rs` |

Web handler 使用**显式绝对路由**；尤其首页声明 `#[page("/")]`，不要依赖模块路径推断 URL，否则移动文件会改变外部路径。嵌入资源使用以 `CARGO_MANIFEST_DIR` 为基准的路径，避免目录深度变化影响字体、图标、夹具和 JS/CSS。

文件大小不是唯一拆分标准：以一个主要职责为目标，接近 600–800 行时检查是否混入了另一个职责。单个声明式界面组合可能稍长，但不应将状态、后台任务、解析算法和测试混放。此次整理后 `src/`、`tests/` 和 `web-topcoat/src/` 中的 Rust 文件均少于 1000 行。

## 测试组织与验证

- 单模块算法测试与实现就近放置；PDF 恢复测试按结构、字体、标题/代码、链接、正文、表格分类。
- 跨存储/导入/呈现的测试放在 `tests/core/`，由原 `rust_core` 测试目标统一运行。
- 合成 PDF 夹具放在 `tests/support/`；保留原 `support::xxx_fixture` 入口，避免每个测试自行构造一套。
- GPUI 测试仍由 `desktop-ui-tests` 控制；渲染回归在 `desktop/tests/rendering_tests/` 按 EPUB、公式、选区、表格、图片和加载流程分组。
- 依赖本机实书的 `#[ignore]` 验收保持原有条件；没有通过移动测试或扩大忽略范围来隐藏失败。

```sh
cargo fmt --all --check
cargo test --locked --all-targets
cargo test --locked --all-targets --features desktop
RUST_MIN_STACK=16777216 cargo test --locked --features desktop-ui-tests --bin easyread-gpui
cargo clippy --locked --all-targets --features desktop -- -D warnings
cargo fmt --manifest-path web-topcoat/Cargo.toml --check
cargo test --locked --manifest-path web-topcoat/Cargo.toml
```

测试模块名称在分组后有所变化；已有函数名与原 `rust_core` 测试目标保持不变，按函数名过滤的常用命令仍适用。
