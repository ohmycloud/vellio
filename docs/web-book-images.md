# Web：PDF / EPUB 插图修复与通用书籍定位

## 现象与原因

EPUB 的 Figure 图注上方显示破图和「文献插图」，或 PDF 只有图注、没有插图。此次确认截图对应 EPUB 的 `Figure 2-2. The eight pillars of agentic graph architecture mapped to the parts of the book` 的原图文件存在，并非导入丢图。

有三处显示路径问题：

1. Figure 的 `src="images/…"` 被直接输出到 `/read/{book_id}` 页面，浏览器按 URL 层级解析成 `/read/images/…`，遗漏书籍 ID。
2. PDF 图片保存在 `figures/`，但旧页面和资源路由仅允许 `images/`、`pages/`，导致 PDF 图块被过滤。
3. PDF 原页几何恢复返回 `data:image/png;base64,…` 内嵌裁图，原 HTML 清理策略会移除其 `src`。

## 修复

- 新增 `web-topcoat/src/book_images.rs`，渲染和资源路由共享目录、格式和路径检查；允许 `images/`、`figures/`、`pages/` 下的 PNG / JPEG / WebP / GIF，支持大写扩展名。
- 图片 URL 统一为 `/read/{book_id}/{asset_path}`，逐路径段转义空格、中文、`#`、`+` 等字符。旧 HTML 内的相对图片也统一到当前书籍；不重写普通超链接／章节锚点。
- 保留 `Workspace::asset` 的 canonicalize 边界检查，拒绝遍历、工作区外符号链接、源文件／JSON 访问；HTML 图片不能指向其他书籍或远端站点。
- 仅在阅读正文的清理路径保留有大小上限、PNG 签名前缀和 Base64 字符限制的内嵌 PNG；不开放 SVG、任意 `data:` 链接、HTML 或脚本。笔记的原清理策略不扩大。
- 按页批量读取 `pdf::code_presentations`，同时使用 HTML 和 `inline_figures`，不再丢弃已恢复图形的身份信息；内嵌图替代旧独立图块，避免重复显示。
- 双语整页回退中，按相同图片 URL 标记共享插图，仅在主语言显示；切换原文／译文／双语及双语顺序时保持一份图片。未翻译块仍显示完整图片。
- Figure 图片保留来源 `alt`，无描述时使用通用「插图」，不再硬编码「文献插图」。独立图片保持在图注前方；图注译文按 caption 字段判断，不能用通常为空的正文 `en` / `zh` 错判成未翻译。

这是显示层和资源路由修复。已有数据不迁移、不重写，已有原图的书籍不需重新导入。若图片物理缺失或导入器未提取出资源，此修复不会凭空生成图片；EPUB 导入器现有的格式限制和 import warnings 仍保留。

## 通用书籍定位

Vellio 支持 PDF / EPUB 书籍、技术书和论文，不限于学术文献。Web 首页使用「书库」「全部书籍」，计数采用通用的「项」，搜索说明涵盖书籍与论文；全文笔记显示「全书笔记」，AI 提问与翻译设置不再假定当前内容是一篇论文。

兼容性标识不改：`paper.json`、`paper_note`、`concurrent_papers`、API 路由、crate 名与环境变量仍保持原值，不因文案调整引入数据迁移。

## 验证

```sh
cargo test --locked --manifest-path web-topcoat/Cargo.toml
cargo build --locked --manifest-path web-topcoat/Cargo.toml
node web-topcoat/images-regression.mjs
# 可选：从已有 EPUB 导入目录复制 data/images 到隔离书库验证，不修改原书
EASYREAD_QA_BOOK=/path/to/imported/epub node web-topcoat/images-regression.mjs
node web-topcoat/smoke.mjs
node web-topcoat/ai-smoke.mjs
node web-topcoat/pdf-layout-regression.mjs
node web-topcoat/import-regression.mjs
node web-topcoat/controls-regression.mjs
node web-topcoat/note-code-regression.mjs
```

新增图片回归需要 Node ≥ 22、Chrome 和 `python3`，会动态生成带图片／图注的 EPUB 和 PDF，经过真实 `/import` 后验证：

- 图像 HTTP 返回 200、正确 MIME，并由 Chrome 实际解码（`naturalWidth > 0`），不以存在 `<img>` 作为通过条件。
- 原文／译文／双语均显示图片，原文在前／译文在前均不重复；包含未译、已译图注、已译 PDF 页与旧正文内图片。
- Figure 图片位于图注上方；PDF 原生 PNG 清理后仍可见，旧独立图块不再重复。
- 中文／空格／保留字符文件名、大写扩展名可访问；源文件、JSON、遍历及越界符号链接不可访问。
- 阅读前后导入数据不变。

本次 Rust 单测 15 项全部通过；图片、主流程、AI 模拟接口、PDF 排版、导入按钮、弹层和笔记代码块的浏览器回归均通过。本次还使用截图中 EPUB 的只读副本验收第 8 章的 Figure 2-2；修复后截图输出到 `/tmp/vellio-web-book-figure.png`。这里只验证了当前本机 Chrome，没有宣称所有书籍／图片格式、浏览器都已全面验收。
