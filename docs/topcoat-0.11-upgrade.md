# Web 阅读器：Topcoat 0.11 升级记录

发布依据：[tokio-rs/topcoat v0.11.0](https://github.com/tokio-rs/topcoat/releases/tag/v0.11.0)。仅调整 `web-topcoat/`；不改桌面功能、书库 schema、批注锚点或模型密钥存储。

## 已采用的功能

### 依赖与运行环境

- `web-topcoat/Cargo.toml` 固定 `topcoat = "=0.11.0"`，更新独立的 `Cargo.lock`，声明 `rust-version = "1.99"`。
- 保留已有 `multipart`、`anyhow`，增加 `l10n-fluent`；不新增前端框架、Tailwind 构建流程或客户端 runtime。
- 开发 CLI 建议与依赖一致：`cargo install topcoat-cli --version 0.11.0 --locked`。无需重新创建项目。

### Popover 与元素 ID

目录、双语顺序、Aa、模型卡片的操作菜单由 `<details>` 改为原生 `<button popovertarget>` / `<div popover="auto">`。

- 浏览器负责外部点击关闭、Esc、同层菜单互斥、焦点恢复及 top layer，避免祖先 overflow 裁剪。没有添加自己的 document 点击监听或模拟 `aria-expanded` 状态。
- `assets/popover.css` 将 0.11 UI registry 的 `FloatingAnchor::Invoker`、底部 Start / End 对齐、`position-try-fallbacks` 策略改写为项目现有 CSS；不直接安装带另一套主题的 Topcoat UI 组件。
- 不支持 anchor positioning 时保留可用的居中弹层；支持原生 Popover API 是浏览器要求。窄屏限制最大宽高，内容可滚动。
- 新关联 ID 由 `element_id` 生成。模型循环既有 `#[key(model.id.clone())]`，也为循环内显式调用传入 `cx.keyed(model.id.clone())`；普通 Rust 表达式不会自动使用 `view!` 循环的上下文，这是防止菜单串 ID 的关键。
- 双语顺序按钮通过 `popovertargetaction="hide"` 关闭菜单；实际排序与 localStorage 持久化仍使用原来的逻辑。
- 从模型菜单进入编辑对话框时，先关闭弹层，再打开 modal 并聚焦名称，避免焦点回到 modal 后方。
- 阅读器全局 Esc 处理在存在顶层弹层时不关闭底下的页边笔记或译文编辑器。再次按 Esc 才沿用阅读器原来的关闭／提交行为。

折叠脚注、首页内联创建文件夹及批量删除确认等不是此次改造目标，仍保留原有 `<details>` 行为。笔记高亮选区操作栏也没有改成会改变选区／焦点的原生弹层。

### Fluent：先完整覆盖 Aa 控件

`src/reader_controls.rs` 将 Aa 提取为组件。两份 FTL 各有 9 条消息，位于 `src/reader_controls/l10n/en.ftl` 和 `zh-CN.ftl`。

- 请求 layer 在渲染前用 `accept_language` / `negotiate` 选择语言，放入 `Cx`，不是在 layout 内设置一个页面取不到的上下文。
- 英文区域语言回落到 `en`；支持简体中文；不匹配、空或无可接受语言时使用 `zh-CN`。
- 控件自己的 `lang` / `dir` 与翻译匹配，标题通过 `aria-labelledby` 关联；HTML 响应追加 `Vary: Accept-Language`，不覆盖可能已有的 Vary。
- 未声明整站国际化：其余书库／阅读器文案和设置页语言不变；不翻译用户笔记、模型输出或源文献，不改桌面的语言设置。
- 当前没有额外的语言切换开关，语言由请求头决定。新增消息应配合 `topcoat l10n check` 检查完整性。

## 刻意保持不变的路径

v0.11 的 edited form controls、导航竞态、hydration、signal 文本等修复属于 Topcoat browser runtime。本项目当前为 SSR 加自定义异步增强，没有启用该 runtime，所以不能声称这些修复自动改善已有局部更新路径。

笔记、全文笔记和译文修订继续走既有保存队列、清理后的服务端 HTML 和受控 DOM patch，保留草稿、光标、滚动位置、段落 ID 与公式图片复用；不引入 `Expr::dynamic`，不以重渲染冒充“清空草稿”。保留原本的权限／部署边界：仅本机、无账户隔离、不要公开服务。

没有为了追新添加 navigation_menu（当前没有多级站点导航）、日期／数字 locale 格式（不改持久化值和引文）、`topcoat new`（已有项目）或 memoize 改写（当前路径不需要）。

## 验证

在仓库根目录运行，浏览器测试需要 Node ≥ 22 与 Chrome；非 macOS 通过 `CHROME` 指定可执行文件。所有测试使用隔离的临时书库，不读写真实桌面书库。

```sh
cargo test --locked --manifest-path web-topcoat/Cargo.toml
cargo build --locked --manifest-path web-topcoat/Cargo.toml
(cd web-topcoat && topcoat l10n check)
node web-topcoat/controls-regression.mjs
node web-topcoat/smoke.mjs
node web-topcoat/ai-smoke.mjs
node web-topcoat/pdf-layout-regression.mjs
node web-topcoat/note-code-regression.mjs
```

本次本地验证：

| 检查 | 结果 |
| --- | --- |
| Web Rust 单测 | 11 通过 |
| Fluent 检查 | en 9/9、zh-CN 9/9；0 错误、0 警告 |
| 新增控件回归 | 1400×900、530×500、390×360 的 3 个阅读菜单均在视口内；外部点击、菜单互斥、Esc 与焦点恢复、父容器裁剪、下层草稿保留、Aa 偏好持久化、模型重排 ID 稳定、进入 modal 的焦点、禁用 JS 后目录翻页通过 |
| 既有主回归 | 局部批注、草稿、失败重试、公式选区／标记像素、书库／文件夹、PDF 双语段落与译文修订通过 |
| AI 模拟接口回归 | 模型设置、反馈／回答／追问、数学显示、翻译同步及取消通过；不请求付费外部模型 |
| PDF 排版回归 | 公式编号、脚注折叠、列表悬停竖线、多字号及窄列通过 |
| 笔记代码块回归 | 页边和抽屉代码保留源码与语法色、不折行、隐藏滚动条仍可横向滚动通过 |

验证环境：Rust 1.99.1、Topcoat CLI 0.11.0、本机 headless Chrome。尚未宣称完成 Safari / Firefox 跨浏览器验收或真实 PDF 语料的全面排版验收。
