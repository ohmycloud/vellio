# Vellio Topcoat PDF / EPUB 书籍阅读器实验（仅限本机）

独立于桌面 GPUI 的 **Topcoat 0.11.0** 服务端渲染纵向切片（Rust ≥ 1.99）。面向 **PDF / EPUB 书籍、技术书与论文**，不是仅限学术文献的阅读器。直接调用根 crate 的 Rust PDFium/EPUB 导入、`ReaderState`、`Workspace::reader_ops`、PDF 页几何 HTML 恢复和笔记装饰算法；浏览器使用真实 DOM 文字选区，不再使用 GPUI/WASM Canvas。**没有引入 Axum 或 Dioxus。**

```sh
cd web-topcoat
cargo run
# 默认 http://127.0.0.1:3000
# 可指定独立测试书库：VELLIO_WEB_LIBRARY=/path/to/test-library cargo run
```

## 源码组织

`src/main.rs` 保留启动、应用状态和共享请求/显示类型；HTTP handler 按首页、阅读、批注、AI、译文和资源拆分到 `src/routes/`，所有 URL 显式声明。`src/document.rs` 负责输入 HTML 清理和阅读显示块转换，相关回归位于 `src/document/tests.rs`；现有控件和模型设置模块继续独立维护。共享引擎的目录及依赖边界见 [项目结构与维护指南](../docs/architecture.md)。

## Topcoat 0.11 升级

依据 [v0.11.0 发布说明](https://github.com/tokio-rs/topcoat/releases/tag/v0.11.0)，已落地：

- **原生弹层**：目录、双语顺序、Aa 和设置页的模型操作菜单使用 `popover="auto"`，外部点击／Esc 自动关闭；进入浏览器 top layer，不受父级 overflow 裁剪。共享 `assets/popover.css` 沿用新版 floating 的 invoker anchor、底部对齐和自动翻转策略；不支持 CSS anchor positioning 时居中显示。没有引入 Tailwind 或替换现有阅读器主题。
- **稳定 ID**：菜单与触发器通过 `element_id(cx)` 连接，Aa 标题通过稳定 ID 关联；模型列表以模型 ID 作为 key，重排不会串联到其他菜单。不改变批注／段落的持久化 ID。
- **局部中英本地化**：Aa 是首个完整接入 Fluent 的控件。按 `Accept-Language` 协商 `en` / `zh-CN`，无匹配时回退简体中文；FTL 位于 `src/reader_controls/l10n/`。只对 Aa 设置准确的 `lang` / `dir`，不会把未翻译的书库、阅读器、设置页或文献内容宣称为已完成国际化。HTML 响应包含 `Vary: Accept-Language`。
- **编辑状态兼容**：没有启用 Topcoat browser runtime，原有异步笔记和译文修订仍使用受控局部 DOM 更新；新版 runtime 的“保留用户已编辑输入”不会自动作用于此路径，也不需为了清空表单引入 `Expr::dynamic`。Esc 优先关闭顶层菜单，不顺带关闭／提交下方编辑器；原有草稿、保存队列和冲突检测保持不变。

目录菜单的开关及章节链接无需 JavaScript；Aa 实时排版与双语顺序保存仍需 JavaScript。需要支持原生 Popover API 的现代浏览器。安装匹配的开发 CLI 可运行 `cargo install topcoat-cli --version 0.11.0 --locked`，然后在 `web-topcoat/` 中执行 `topcoat l10n check`（当前两种语言各 9 条消息）。更完整的升级范围与验证见 [升级记录](../docs/topcoat-0.11-upgrade.md)。

Mac ARM64 本地 PDFium 使用根项目的 `assets/pdfium/libpdfium.dylib`；其他系统须配置与 `pdfium-render` 绑定匹配的 `EASYREAD_PDFIUM_LIBRARY`。本试验与桌面书库默认隔离（`web-topcoat/vellio-web-library/`）；若主动将 `VELLIO_WEB_LIBRARY` 指向桌面书库，可读取相同的 schema-2 `paper.json` / `reader.json`，但**建议先备份**，不要同时在两个程序里编辑。

**软件更新**：首页「? 帮助与更新」及 Settings → About / Updates 显示当前 Rust 核心版本、手动检查、可关闭的每日自动检查、新版本说明和对应服务端系统/CPU 的桌面包下载。更新源固定为 `ohmycloud/vellio` 的稳定 GitHub Releases，网络/限流错误不会冒充最新版；只打开浏览器下载，不执行安装。Web 服务自身仍需更新源码、停止服务并重新编译，桌面下载不会替换 Web 服务。签名、发行包约定与边界见 [软件更新与发行](../docs/updates-and-releases.md)。

**插图修复**：独立 Figure 图片及正文内的图片统一使用带书籍 ID 的资源 URL，支持 EPUB 的 `images/`、PDF 的 `figures/` 与原页 `pages/`。PDF 几何排版的安全内嵌 PNG 可正常显示，并抑制被内嵌图替代的旧图块；双语整页回退也会按当前主语言去重。保留原有图片说明作为无障碍替代文本，不再硬编码「文献插图」。已有书籍只需重启服务并刷新，无需重新导入，不改写 `paper.json`／原图／批注。物理缺失的图文件或导入器尚不支持的格式不属于此路径修复，详见 [图片修复与通用书籍定位](../docs/web-book-images.md)。

**原书目录**：顶部「目录」菜单不再混入页码网格、「跳转到」输入框或章节位置编号。EPUB 使用导入时保留的原始 EPUB3 nav／EPUB2 NCX，PDF 从源文件读取书签及子书签；保留原书标题、顺序和多级嵌套，子章节默认展开，长标题换行、不截断，不因译文或正文修订改名。EPUB 链接定位到对应章内的原始 block（包括非标题目标），PDF 书签跳转到目标页；JavaScript 增强当前目录项定位，以及同章跳转后关闭菜单。没有导航／书签时明确标注按正文标题生成；连可用标题也没有时显示空状态，不以页码冒充目录。已有书籍重启服务并刷新即可，不改 schema、不需重新导入。

已实现的桌面功能对应项：

- **书库首页**：235px 分类侧栏、封面/题名/阅读进度的列表，检索（包含所属文件夹名称）、最近阅读/添加/年份/标题排序，在读/未读/已读/收藏/待解答/回收站筛选；收藏、改名、状态变更、移入/移出回收站；创建/重命名/删除文件夹，并可用拖拽或行内表单加入/移出；多选支持批量归档、设置状态、复制引文、移入/移出回收站。元数据和阅读进度使用现有 `Library::list` / `library_home::summary`，文件夹与回收站沿用桌面的 `.library-home.json` 结构。
- **阅读**：首页仅保留一个「导入」按钮，点击后打开文件选择窗口，选中 PDF/EPUB 自动上传并进入阅读器，取消选择不提交；导入期间禁用按钮避免重复提交，返回首页后恢复可用。本机导入 PDF/EPUB（32 MiB 请求上限），按页/章阅读、原书多级目录（EPUB 导航／PDF 书签，含子章节）、原文/译文/双语模式和双语显示顺序（默认原文在前；同一本书在当前浏览器保存顺序）、插图及右侧 PDF 原页；保存阅读进度后从书库继续阅读。56px 工具栏、35 字宽正文、左 2／右 1 留白及页边笔记竖线参照桌面。Aa 调字号、行宽、行距、字体和页边显示；设置保存在**当前浏览器 localStorage**，不与桌面同步。
- **翻译与修订**：阅读顶部已移除「确认费用」和「翻译全文」控件；整书翻译后端接口仍保留，进行中的任务仍可从工具栏查看进度及取消。后台任务与桌面一样调用 `ai::translate_batched`，复用已选翻译模型、批量页数、并发数和费用确认阈值，结果写入 `paper.json` 并自动刷新当前阅读页。同一本书同一时刻只启动一个 Web 翻译任务，超出桌面阈值的文献需要明确确认费用。从选中文字后的悬浮框可在译文所在段落原位修订该段译文或图表题注；即使 PDF 整页存为一个 block，编辑框也只装入目标段落，保存/还原仅合并该段并保留周边文字、公式和排版。提交复用 `Workspace::reader_ops("edit")`，通过文本哈希及旧修订快照拒绝冲突，支持还原修订。
- **Ask AI**：配置模型后可从工具栏或选中文段提问；笔记卡片提供「Ask AI for feedback」，问题卡片提供「Ask AI to answer」，点击会直接发送而非只预填草稿。已解答的问题只打开追问，不重复请求原问题；未配置模型时保留禁用按钮及配置提示。调用 `ai::ask_with_context`，保存到该书 `chat.json`，反馈与问题答复关联原笔记写入 `discussion.json`；只有问题答复更新解答状态，反馈不会把笔记改成问题。回答与页边 AI 回复支持 `$…$`、`$$…$$`、`\(…\)`、`\[…\]`；独立公式单独居中、按列宽缩放，代码示例保留源码，历史回答不需重问、不修改存储内容，见 [AI 公式修复](../evidence/ask-ai-math.md)。当前 Web UI 仅使用独立的 `web-reader` 对话线程，提交后等待完整结果（没有逐 token 流式显示或桌面版多线程管理）。模型密钥留在服务器环境变量中，不进入 HTML。
- **批注**：选中文字或单击已有高亮/下划线时打开同一个操作悬浮框，普通文字与公式行为一致，不再另弹「编辑笔记 / 删除标记」框；单击未标记的普通段落只收起操作框、不弹框；高亮或下划线，黄/绿/蓝/红四种颜色；选择样式后点击颜色直接保存，无需再打开对话框确认；已移除重复的「应用高亮/下划线」按钮。高亮/下划线切换使用与桌面相同的荧光笔、U 下划线图标及浅色选中底，保留悬停提示与无障碍标签；高亮颜色显示圆点，下划线颜色显示细线。「笔记 / 问题」使用与桌面同结构的页边编辑器，而非「添加 + 保存/取消」弹窗：My note / My question、时间、彩色引文边线、Markdown/数学公式输入提示、Notes / Question 类型切换、四色选择和 Answered / Unanswered 状态。正文、类型、颜色、解答状态均在约 700ms 后自动保存；Esc 或关闭按钮会提交最后的改动，不是丢弃。仅打开或切换空草稿不会创建笔记；小窗口使用非模态侧边编辑器。右侧卡片和 Notes 抽屉中的 Edit 都打开相同的页边编辑器，不再展开旧的类型/标记/颜色下拉表单。卡片悬停或键盘聚焦时显示水平对齐的 Edit、Make it a question / Make it a note、Delete，操作行预留高度，不会挤动卡片。互转只修改类型，保留正文、颜色、解答状态与关联标记；触屏设备直接显示操作。正文中点击已有标记会选中整条引文并打开统一操作栏，点击颜色修改原标记、不重复创建笔记；点击「笔记 / 问题」可打开或续写对应的页边笔记。由高亮打开时单独关联笔记，不把原高亮转成问题，也不重复叠加标记；编辑器改色会同步关联的原标记颜色。删除仍在页边笔记中操作。Notes 抽屉可浏览**全书**批注、按类别筛选、跨页跳转和保存全书笔记。多条同块笔记按文字实际渲染位置排序及避让，而不是按创建时间排序。
- **无刷新编辑**：高亮、下划线、笔记/问题的创建、修改、删除，以及全文笔记和译文修订均异步提交。复用服务端清理后的阅读页结果，只更新受影响的段落和批注条目，不重新导航或重建阅读器；保留当前滚动位置、抽屉及筛选状态、已打开的编辑框和未保存字段，复用已解码的公式图片以避免闪动。写入按顺序执行，旧自动保存结果不覆盖新草稿或光标。页边笔记在首次写入前分配稳定 ID，重试不会重复创建；同一编辑会话按修订序号拒绝迟到的旧写入（包括关闭页面时的最后一份草稿）。保存失败保留选区/草稿；写入成功但读取显示失败时，仅重试读取，不重复创建标记。禁用 JavaScript 时删除、类型互转、全文笔记等仍可用原有表单提交；页边编辑器需要 JavaScript。
- **公式**：复用桌面的 `formula::layout`（MiTeX/Typst）完整字形边界与基线生成无额外页边距的 SVG，以 `<img>` 作为不执行脚本的视觉层加载行内及独立公式，避免根号、分数及下标截断。公式尺寸随 Aa 字号缩放；DOM 保留 TeX 文本用于复制及批注锚定；含公式段落按实际基线合并选中行，并用连续背景层补齐公式上方及相邻行之间的空白，不跨段落外边距涂色，也不覆盖首尾未选中的文字。已保存的高亮同样按行连续铺色；下划线按完整行绘制在公式下探部分以下，不在公式处断开，也不被高亮层遮盖。公式内部保留批注元数据，点击已标记公式与点击已标记普通文字一样打开统一操作栏。支持高分数、紧行距、反向选择及字号/行宽变化后的重新排版。TeX 编译失败时保留可见的 TeX 源文。不将来源文档的 HTML/CSS 直接注入：`src/document.rs` 在 Topcoat `Unescaped` 输出前使用 ammonia 清理，只保留受限的公式/批注属性。

**模型配置（桌面 ↔ Web）**：Web 首页右上角「⚙ 模型设置」进入独立设置页。Model 页按桌面版布局显示档案卡、翻译及 Ask AI 默认模型；点击卡片选择翻译模型，菜单可编辑连接、设为 Ask AI、删除，另可新建 API / Claude Code / Codex CLI 模型。翻译批次、分段并行、确认费用阈值、推理力度及单句模型测试真实可用，保存写入桌面书库的 `.settings.json`，下次桌面打开时可读取；Web 每次加载读取新配置。保留桌面尚未识别的 JSON 字段，检查旧版本号以拒绝过期表单。不要同时在桌面和 Web 编辑设置：桌面运行时可能持有未保存的设置草稿。Web 的 1–4 篇同时翻译上限写入扩展字段 `concurrent_papers`，**当前仓库桌面执行器不支持该上限，也不支持截图中的 Fast 模式**；界面明确标注，不将其伪装成已同步能力。

Reading 标签现可保存桌面阅读偏好，并作为没有本地 Aa 覆盖时的 Web 默认排版；**Sidebar / Cloud library / Keyboard shortcuts 标签目前仅说明各自的桌面功能，尚非完整迁移**。默认按桌面书库规则定位 `.settings.json`（先看 `EASYREAD_LIBRARY`，再看桌面保存的书库位置，最后用系统默认目录）；若桌面以 `--library` 使用其他路径，启动 Web 时设置 `VELLIO_DESKTOP_LIBRARY=/path/to/desktop/library`。Web 仍使用独立书库，不会修改桌面书籍。桌面档案只存密钥的**环境变量名**，Web 进程仍须能读取对应变量；缺少桌面档案时保留 `EASYREAD_MODEL`、`EASYREAD_API_KEY`、`EASYREAD_API_BASE` 兼容回退。只在可信的本机使用；调用可能产生费用，尚无账户/配额/计费隔离，服务重启不会恢复进行中的任务。

PDF 原文尝试复用桌面的 `pdf::code_html` 几何恢复；不匹配时回退到普通文段。独立公式与编号采用同一网格行垂直居中，窄列下缩放公式、不将编号挤到下一行。页底有短分隔线、较小字号和脚注标记时，脚注作为独立段落恢复；Web 在原文、译文和双语模式均将其放入默认收起的「本页脚注」，避免跨页阅读时正文被脚注译文穿插。旧译文只有在段落结构及脚注编号匹配时才继承分组；无法确认的内容不猜测、不隐藏。仍按 PDF 源页翻页，本次不合并跨页段落，也不改写已保存的译文或批注锚点。PDF 即使存储为整页文本块，悬停竖线也只跟随当前段落，不会在点击后留下一条贯穿整页的线；无序列表的竖线额外向左避让圆点，间距随字号缩放，普通段落位置不变。对有可靠结构的 PDF 译文复用桌面的 `translation_layout` 与 `PairLayout`，按「英文段落 → 对应译文」交错呈现（浏览器可切换顺序）；悬浮栏的「En / 译文」可逐段展开/收起另一种语言，不切换全页模式，也不影响其他段落。原文模式下修订会在对应译文的位置打开该段编辑器；取消恢复先前的展开状态。公式/图片只保留一次。结构不匹配的旧译文不会按序号猜测对应段落，页面会显示说明并保留整页对照；无法可靠定位对应段落时禁用局部语言切换和从原文直接修订，可在译文模式选定具体段落修订。笔记锚点仍采用原有整页 block ID，因此跨块选区及某些复杂 PDF 的锚定仍受解析粒度限制。

浏览器回归使用隔离测试书库，验证首页状态/收藏/改名/回收站/文件夹创建、重命名、删除、拖拽与搜索、双语顺序持久化、译文修订/冲突拒绝/还原、多选批量归档/状态/回收站、PDF 导入、悬浮框与双态笔记、桌面式页边笔记/问题切换、空草稿、解答状态、颜色联动、自动保存/关闭提交/失败重试/迟到写入/并行输入及窄窗口布局、卡片悬停操作行对齐与稳定性、卡片/抽屉/跨页 Edit 的统一入口、编辑/颜色/下划线、MiTeX SVG 行内/独立公式、同块笔记先后与标记几何、PDF 多段双语交错/悬停竖线的浏览器几何、逐段 En/译文展开收起、整页 block 中的原位段落修订/取消/还原/失败重试、跨页跳转及外观设置持久化（需 Node ≥22、Chrome；非 macOS 设置 `CHROME`）。截图写入 `/tmp/vellio-topcoat-home.png`、`/tmp/vellio-topcoat-qa.png`、`/tmp/vellio-topcoat-math-selection.png` 与 `/tmp/vellio-topcoat-math-tight-selection.png`（含高分数、根号、紧行距及连续选区）；保存后的标记截图为 `/tmp/vellio-topcoat-saved-underline.png`、`/tmp/vellio-topcoat-saved-highlight-only.png` 与 `/tmp/vellio-topcoat-saved-marks-tight.png`，悬浮框截图为 `/tmp/vellio-topcoat-underline-toolbar.png`。页边编辑器截图为 `/tmp/vellio-topcoat-margin-note-draft.png`、`/tmp/vellio-topcoat-margin-question-draft.png`、`/tmp/vellio-topcoat-margin-note.png`、`/tmp/vellio-topcoat-margin-question.png` 和 `/tmp/vellio-topcoat-margin-note-narrow.png`。卡片操作和 Edit 入口截图为 `/tmp/vellio-topcoat-note-card-actions.png` 与 `/tmp/vellio-topcoat-note-card-edit.png`。回归还会读取 PNG 像素，检查保存后的高亮是否露出纸色、下划线是否被填色遮盖，并验证长页深处操作不导航/不跳顶、公式图片复用、抽屉/草稿保留、保存期间继续滚动、并行输入和失败后的安全重试：

```sh
cargo build --manifest-path web-topcoat/Cargo.toml
node web-topcoat/smoke.mjs
node web-topcoat/images-regression.mjs    # PDF/EPUB 真实导入、插图解码与图注位置、三种模式/双语顺序、路径编码与资源访问边界
node web-topcoat/import-regression.mjs    # 单一导入按钮：鼠标/键盘打开原生选择器、取消不提交、选中自动导入、返回首页恢复
node web-topcoat/contents-regression.mjs # EPUB3 nav／EPUB2 NCX／PDF 三级目录：原题、层级、同章/跨章定位、返回、窄屏、无 JS；不显示页码跳转。可用 EASYREAD_QA_BOOK 指定已有 EPUB3，读取源 EPUB 独立对照目录
node web-topcoat/controls-regression.mjs  # 0.11：弹层窄屏/外部点击/Esc/焦点/无 JS、稳定 ID、Fluent 协商与底层草稿
node web-topcoat/pdf-layout-regression.mjs  # 自动生成两页 PDF，验证编号几何、旧译文脚注分组、三种模式、窄列、批注后展开状态，以及 14/21/32px 下列表竖线与圆点的间距；可用 EASYREAD_QA_BOOK 指定 Attention 的导入书目录进行只读副本验收
node web-topcoat/ai-smoke.mjs   # 本地模拟模型 + Chrome：档案设置/翻译/提问及笔记反馈/问题回答/追问按钮、真实 Ask 路径的行内/独立公式 SVG 解码与列宽；不请求外部模型
```

**还不能称为完整桌面功能对齐，也不是生产级在线 App**：首页尚无桌面文件夹置顶/隐藏、批量翻译或完整封面；阅读器尚无截图中 Fast 模式、全部设置标签的完整功能、AI 流式输出/多对话管理、复杂表格与公式排版逐篇验收、跨块选区、EPUB 动态分页或桌面级逐段富文本编辑、即时全量编辑状态/讨论回复的手工编辑。笔记支持 Markdown 呈现，但编辑仍是纯文本输入框。没有账户/权限、多用户隔离或跨设备同步。Topcoat 目前仍属实验性框架；此服务拒绝非回环 `HOST`，不要经反向代理公开。
