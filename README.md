# Vellio Rust / GPUI

阅读器现名为 **Vellio**（原 EasyRead Rust / GPUI）。为兼容现有安装，Rust crate / 开发二进制名 `easyread-gpui`、`EASYREAD_*` 环境变量、默认书库与设置路径及内部数据标识保持不变；已有书籍、批注和阅读进度无需迁移。

迁移已进入实现阶段：原生 GPUI 界面直接调用 Rust 文献库、PDF、批注、公式和 AI 服务，不依赖 Python HTTP 后端或 Electron。当前为可运行的第一个迁移增量，完整功能对齐尚在进行。

当前进度、剩余工作及验收门槛统一见 [当前里程碑](docs/milestones.md)，已对照 EasyRead v1.4.1（`f1899d1`）修正。历次实现见 [迁移执行记录](docs/migration-progress.md)；[旧可行性报告](docs/milestones-2026-10-03.md)、[Ely 对比](docs/ely-evaluation.md)、[原始 report.md](report.md) 仅供历史参考。原项目 `../easyread` 未修改。

**原生 GPUI 桌面版仍是正式阅读入口。** 另在 [`web-topcoat/`](web-topcoat/) 尝试独立的本机 Topcoat/DOM 阅读器纵向切片，复用 Rust PDF/EPUB 解析与笔记存储；它尚未达到桌面功能对齐，未提供公网在线服务。运行方法、已验证能力与缺口见 [Topcoat 试验说明](web-topcoat/README.md)。

## 运行原生界面

```sh
cargo run --locked --features desktop
```

桌面端现已锁定 **GPUI Kit 0.8.0**（默认后端 gpui-pre 0.3.9），采用新版窗口与主题入口，保留批注、公式、双语排版及 macOS 字体补丁，并减少后台任务等待时的无效刷新。已有书籍无需重新导入；迁移内容、补丁和测试边界见 [GPUI Kit 0.8.0 升级记录](docs/gpui-kit-0.8-upgrade.md)。此前生成的 `.app` 需重新打包，单独更新源码不会替换旧应用。

默认库独立于旧 EasyRead：macOS 为 `~/Library/Application Support/EasyReadGPUI/library`，Windows 为 `%LOCALAPPDATA%/EasyReadGPUI/library`，Linux 为 `~/.local/share/easyread-gpui/library`。可通过 `--library DIR` 或 `EASYREAD_LIBRARY` 指定位置。

界面提供 PDF / EPUB 导入、旧论文目录复制迁入、文献切换、中文/英文对照、目录、正文搜索、原页翻页、段落/图表题注修订、带引文的笔记、进度保存、AI 对话和翻译。顶部提供「原文 / 译文 / 双语」切换，新文献默认显示原文；双语右侧下拉菜单可选「原文在前 / 译文在前」，可可靠配对的内容逐段交替显示，图片只保留一份，不再拼接两份整页；模式与顺序按书自动保存。点击段落或选中文字后，会弹出「高亮、下划线、Notes、Question、Ask AI、En / 译文、复制」工具栏；原文模式下点击 En / 译文，在该段原文下展开译文，再次点击收起；译文模式下则展开或收起该段原文，不影响其他段落；双语模式也可用该按钮只收起或重新展开当前段落的第二语言。未译段落不重复显示对照。独立成行的公式及编号不参与翻译或双语配对，只显示一次；行内公式随各自语言的正文保留。笔记和问题自动带入引文，Ask AI 将引文加入对话输入框。页级 PDF 中可可靠对齐的段落对照插入被点击段落之后，不把整页或图片重复追加；无法可靠对齐的复杂内容保留整块对照，详见 [页内段落语言对照修复](evidence/paragraph-language-companions.md)。公式在后台通过 MiTeX / Typst 编译，阅读器将 Typst 的字形轮廓和规则线直接绘制为 GPUI 原生矢量，不使用 PNG、PDF 裁图或 SVG 图像。行内公式带真实基线参与正文换行，支持整体复制 TeX；含公式的文本选区能弹出工具栏，临时选区与保存后的高亮均覆盖公式所在的完整行高；公式处的彩色下划线与正文衔接，点击公式可打开已有批注，见 [公式选区与高亮修复](evidence/math-selection.md)；编译失败时保留源码。

未指定文献启动时进入独立首页：暖白底色、顶部搜索与导入、左侧全部/在读/未读/已读/收藏和文件夹、最近阅读、右侧封面列表与进度。搜索标题、作者、摘要和文件夹名，排序菜单依次为 Recently opened、Recently added、Year published、Title，文字左对齐、当前项显示勾选；年份按新到旧，缺失年份放末尾。支持收藏、批量阅读状态和文件夹归类，也可将文献拖入自建文件夹。阅读页左上方的文献库按钮返回首页。首页主题、语言、文件夹与归类保存在库根目录 `.library-home.json`；阅读状态、收藏和最近打开时间保存在文献的 `item.json`，保留未知字段。顶部 AI 标识来自实际环境配置；翻译页数/失败页数和批注数来自已有数据。帮助与设置入口提供使用说明、库位置、模型信息及旧目录迁入。

新导入 PDF 会提取栅格图片和矢量图解，保留图内文字与坐标轴，图注独立保留为可选中、可批注的文本，不混入图片裁剪；嵌套 PDF 裁剪路径也参与图片边界计算，旧页面缓存自动重建，无需重新导入，见 [图注裁剪修复](evidence/pdf-caption-clipping.md)；没有可提取文字的页面在正文中显示原页图，不自动添加“未识别文字”题注；图注位于下一源页时保留其原有页码与顺序。图片题注水平居中，与图片共用中心线；后续正文继续左对齐。旧导入的这一自动提示在显示时隐藏，用户修订和真实题注继续保留，无需重新导入。已由 Rust 旧版导入的纯文字书可点击“补全图片”，先备份 `paper.json` / `layout.json` 到 `history`，再添加图片块，不修改原有文字、修订或笔记；重复执行不增加重复块。迁入的 Python 结构化书保留已有图片。正文默认使用随应用内置的 Noto Serif SC（宋体／思源宋体）、21px 字号、1.9 倍行距和 35 chars 行宽（约 735px）；双语中的第二语言对照和题注较小。顶部 aA 打开暖白阅读设置浮框，可实时调整字号（14–32px）、行宽（25–60 chars，以字号计算近似宽度）、行距（1.2–2.6），选择宋体（Noto Serif SC，默认）/ 系统衬线 / 无衬线字体，切换页边笔记 Show / Collapse；语言模式已移至顶部独立控件。字体及其他设置按书自动保存，重开恢复；旧版无衬线选择继续保留，旧版衬线选择使用默认宋体。Reset 恢复默认宋体与阅读设置，再次点击 aA、点击浮框外或 Esc 关闭。验收见 [阅读设置记录](evidence/reading-appearance.md)。PDF 硬换行仅在呈现时整理，存储原文与用户修订保留。已有 `pdf-text` 页块会在显示时从原 PDF 的等宽字体、字符坐标和颜色恢复代码块、缩进、空行及原始高亮，并按目录书签的目标页和真实层级恢复独立的无编号标题，标题与前后正文分段；字体一致的跨行标题也支持，无需重新导入。中文翻译与用户修订保持原样。Markdown 代码块也保留独立等宽排版和按语言的语法高亮。恢复仅适用于原 PDF 可读取、提取文本完全一致且代码独占等宽文本行的情况，扫描图像或混排代码仍使用原页核对。验收见 [代码格式与高亮](evidence/code-formatting.md)。完整语义分段、标题识别和图文穿插顺序仍待 R2 完成。实书验收见 [图片与排版记录](evidence/book-media.md)。

“迁入旧论文”选择包含 `source.pdf`、`paper.json`、`reader.json` 的单篇目录，复制到新库；已有目标目录时拒绝覆盖。选区工具栏采用分组圆角布局：左侧高亮/下划线图标及黄、绿、蓝、红四色圆点，中间 Notes / Question / Ask AI，右侧 En / 译文与复制图标；点击高亮图标显示浅色圆点，点击 U 切换为饱和色圆角矩形；选色后分别保存高亮或彩色下划线；笔记卡也可给已有标记换色，保存沿用旧字段（红色仍存为 `pink`）。点击正文中的已有高亮或下划线，会打开 My highlight 浮框：切换样式、四色换色（当前颜色外圈）、Add note、Question、Remove highlight；下划线为 2px。点击 Add note 或 Notes 后，在正文右侧页边显示 My note 编辑区，包含引文、Notes / Question 切换、四色选项和自动保存提示；Esc 关闭。引文内侧竖条随四色圆点即时换色，保存后的卡片及重新打开的编辑区保持该颜色，验收见 [笔记竖条颜色](evidence/note-quote-color.md)。换色同步已有来源标记，笔记不会遮盖其原有下划线。正文支持颜色与下划线交叠；粗体、斜体、等宽符号等混合字体段落的高亮覆盖完整行高，重排后相邻行仍连续，不改写已有批注，验收见 [高亮行间连续性](evidence/highlight-line-continuity.md)。重复引文的精确选区偏移、失效引文自动重锚及跨块批注仍待完善。

正文的“原页定位”按旧 `layout.json` 定位页码与区域；多栏 `boxes` 分别描框，栏间空白不参与命中。点击原页区域可跳回对应正文，滚动正文时原页跟随阅读位置。新导入 PDF 当前每页一个块，只能提供页级定位，不代表已完成语义段落识别。原生界面每 400ms 检查并保存变化的阅读位置（block id + 段内逻辑像素偏移），重开恢复正文位置和独立的编辑对象。底部状态栏和手动公式/进度按钮已移除，正文多出 32 逻辑像素的高度；公式仍在后台自动编译，任务状态与取消按钮放在顶部，阅读位置自动保存到 `.native-reader.json`。

PDF 阅读正文及英文对照会隐藏页末 `Licensed to 姓名 <邮箱>` 授权标记，并恢复文字编码的无序列表标记（包括 `¡` 加制表符）以及邻接正文的圆形／方形矢量标记。列表依据字符坐标恢复条目、续行和嵌套缩进，随后正文保持独立段落。独立的短编号小节标题保留标题层级和段落边界，后续正文从新段落开始。处理仅发生在显示层，已导入书籍无需重新导入；原始 PDF、提取文本、用户修订及其版本哈希保留。原页视图仍显示原 PDF。验收见 [PDF 页脚与列表修复](evidence/pdf-reading-cleanup.md) 与 [矢量列表修复](evidence/pdf-vector-lists.md)。

阅读界面不再常驻左边栏：顶部依次显示青色书籍 App 图标、目录图标、书名和当前章节，右侧依次显示 原文 / 译文 / 双语（带顺序菜单）、Page / Notes / Ask AI / Aa；前三项分别配文件、对话气泡和放射星形图标，选中时图标与文字同步为青色，按钮使用淡青色圆角背景。目录按钮打开可滚动浮框，章、节、小节按层级缩进，编号以灰色显示并与标题保持统一间距，选择章节跳转并收起；浮框外点击、再次点击目录按钮或 Esc 关闭。章节优先使用已有结构化标题；页级 PDF 使用原文件的本地书签，没有标题/书签时按页导航。PDF 书签目前定位到对应正文页块，同页多个小节尚未精确定位到字符。点击 App 图标打开文献库菜单，保留切书、正文搜索、导入、迁入、补全图片和翻译入口。窗口标题同步书名。正文和页边笔记作为一组，将剩余横向空间按左侧两份、右侧一份分配，使左侧留白更宽；打开侧栏时按可用宽度调整。验收见 [顶部导航记录](evidence/reader-navigation.md)。

页边笔记保存并关闭编辑区后，仍以暖色竖线卡片显示在引文右侧，包含类型、创建时间、引文和笔记正文；Note 显示 Ask AI for feedback，Question 显示 Ask AI to answer，并支持 Note / Question 双向切换和 Answered / Unanswered 标记；随正文滚动，同一 PDF 文本块内的多条笔记按原生文字位置排列并避让。旧 `highlight` / `underline` 记录只要有笔记正文也按个人笔记显示。顶部 Notes 打开全书批注抽屉：Annotations / Paper notes、All / My note / Highlights / AI / Unanswered 筛选、页码和章节分组、Edit / Make it a question / Delete；Ask AI 按 EasyRead 的 220ms 缓动曲线从右向左滑入。宽屏下右栏宽度为窗口的 44%（最大 720px），正文同步让位，形成独立滚动的两列；可拖动分隔线调整宽度、双击恢复，窄屏采用右侧覆盖面板。笔记卡的反馈 / 回答按钮直接发起请求，流式显示回答，并将成功回答保存为该笔记的回复；Question 随之标为 Answered，已回答的问题再次点击则打开追问，不重复发送。模型配置失败时保留草稿与引用以便重试。对话顶部支持新建、切换、改名和确认删除，底部固定引用标签、模型 / 回答方式选择和发送 / 停止按钮；模型菜单以两行显示名称与来源、默认模型说明和当前勾选，并支持仅当前对话生效的思考强度选择与恢复默认，强度会随对话保存，不修改全局配置。回答方式菜单显示标准回答 / 简明回答 / 精读的标题与详细说明、当前勾选及额外正文上下文的用量提示，长说明自动换行。Enter 发送、Shift+Enter 换行。选区 Ask AI 把引用加入可移除、去重的标签，而不是粘入输入文本；无手动引用时使用当前阅读段落。标准回答只带附近段落和明确引用，只有问到笔记 / 标记时才带相关标记，精读 / 简明模式附正文节选。回答可复制或放到页边；删除对话不删除已有页边讨论。详见 [Ask AI 对齐验收](evidence/ask-ai-panel.md)。Ask AI 回答和页边 AI 回复共用正文的数学公式渲染，支持 `$…$`、`$$…$$`、`\(…\)`、`\[…\]`；独立公式另起一段并适配列宽，代码中的公式示例保持原样，历史回答无需重问。桌面端在后台编译原生矢量公式，流式回答等待分隔符闭合；不修改已保存的回答文本。详见 [AI 公式修复](evidence/ask-ai-math.md)。Notes 列表中卡片左侧的长竖线及高亮引文左侧的短竖线均可点击，正文平滑滚动至对应段落/引文，鼠标停留在竖线时目标正文块显示 `#ECECE6` 圆角背景；悬停笔记正文则只为对应原文段落显示 `#ECECE6` 背景，侧栏保持打开。离开长、短竖线或收起 Notes 后立即恢复原背景，不保留点击目标底色；悬停不改变阅读位置；手动滚动正文可中止跳转动画。页级 PDF 尚未语义分段，因此背景标示整个对应页文本块。引文无法匹配时定位段落开头，原文引文可自动显示英文对照。旧引文粘贴表单和黄色笔记卡已移除；阅读时默认关闭抽屉。验收见 [选区与笔记记录](evidence/selection-toolbar.md)。

修订草稿按论文、段落或图表题注隔离，切换面板/段落/论文及重开后恢复；旧侧栏笔记草稿字段保留兼容读取。草稿、引文上下文、阅读位置、中英对照/原页显示偏好和当前面板保存在各论文的 `.native-reader.json`。页边笔记在输入停顿后自动写入 reader.json，连续修改沿用同一 ID；Esc、换段、换书或正常关窗会提交最后输入，空白新编辑区不会创建记录。保存成功且磁盘内容一致后才清除对应草稿，保存过程中新增的文字继续保留。正文版本变动时提示核对并保留旧草稿。正常关闭会刷新恢复文件；异常退出可能丢失最后约 400ms 的输入。窗口宽度、字体或正文重排会影响阅读像素偏移的视觉精度，尚未采用字符级阅读锚点。恢复文件损坏时保留原文件并提示错误，不覆盖。详见 [草稿与阅读恢复验收](evidence/reader-recovery.md)。

macOS ARM64 的 PDFium 动态库已随本地工程提供。PDF 引擎为 C++ PDFium，通过 Rust `pdfium-render` 调用；应用服务层为 Rust。其他平台需要提供相应动态库，并设置 `EASYREAD_PDFIUM_LIBRARY`。详情见 [PDFium 来源](assets/pdfium/README.md)。

PDF 插图及图片整页的外部白底在显示时适配阅读器底色，保留图内文字和封闭彩色区域。点击图片打开大图预览，可放大、缩小、恢复适合窗口及滚动查看，按 Esc 返回原阅读位置。已有书籍无需重新导入。对于源 PDF 将独立位图非等比例放置的情况，阅读视图按内嵌图片的原始比例显示，点击放大共用修复后的图片；含叠加文字、组合对象、裁剪、旋转或镜像的图仍保留原页几何，避免丢失内容。旧显示缓存自动重建，不改写书库数据。共用同一 Figure 图注的并排子图现在作为整体保留源位置、间距、相对大小和图内标题，不再各自放大并上下排列；普通字体的编号图注也保持独立居中段落，旧导入无需重做。独立图注或没有图注的图片不据此合并。验收见 [图片底色与放大](evidence/pdf-image-preview.md)、[位图比例修复](evidence/pdf-bitmap-ratio.md) 和 [并排子图修复](evidence/pdf-shared-figure-panels.md)。

PDF 中有数学字体与几何证据的公式会恢复为 TeX，再交给 Typst 原生矢量排版，覆盖独立公式以及正文中的变量、上下标、根号、分式和求和上下限。公式裁图和图片预览路径已撤掉；v13 排版缓存自动重建，无需重新导入，也不改写原始 PDF、书库文本或批注记录。PDF→TeX 仍是保守的结构恢复，不是通用 OCR：复杂矩阵、多行对齐、扫描公式和字符顺序交错的区域暂不保证自动恢复，无法确认的内容保留既有文本呈现。有明确 TeX 的矩阵、分段式与多行公式可由同一 Typst 原生渲染器排版。拆分字体的小数、科学计数和数字比值保持完整数学对象；紧接较小子标题的章节标题也保持独立段落。重复变量在页码等上下文被移除后仍按保留下来的上下文定位，避免整页行内公式恢复被撤销；旧译文的 `1/ √ dk` 也可按原文证据恢复为完整分式。详见 [PDF 行内 / 独立公式修复](evidence/pdf-display-equations.md)、[行内分式恢复修复](evidence/pdf-inline-fraction-recovery.md) 和 [小数与相邻标题修复](evidence/pdf-inline-decimals-and-headings.md)。

设置齿轮打开六标签设置面板：Model、Reading、Sidebar、Cloud library、Keyboard shortcuts 和 About / Updates。支持 API 环境密钥引用、Claude Code / Codex CLI 本机登录；可分别指定翻译和 Ask AI 默认模型，并配置页批次和并行度。阅读偏好与功能开关、可编辑快捷键可保存或取消，侧栏文件夹显示/置顶即时生效。云书库操作复制完整书库到选定同步目录下的 `Vellio/library` 并保留原目录，成功后恢复该位置。详情与测试边界见 [设置面板验收](evidence/settings.md)。

PDF 规则表格现在会在阅读视图中恢复行列、原始列宽、空白表头和斜体标签，保留可选文字、源链接及批注；无需重新导入。有逐格几何证据的无表头续页按数据行显示，表格底色和边线不再重复显示为图片。恢复依据原 PDF 的横线和字符坐标，扫描及复杂表格仍可通过原页核对。详见 [PDF 表格修复](evidence/pdf-table-layout.md)。只有表头下方一条横线、通过连续竖线分列的规则表格也支持原生恢复，保留源列宽、分隔线和可复制单元格，并隐藏旧导入截图；v12 排版缓存自动重建，无需重新导入。验收见 [单横线表格修复](evidence/pdf-single-rule-tables.md)。

PDF 正文及题注现在保留行内粗体、斜体和等宽字形；等宽标识符按正文大小加粗显示，真正的代码块保留代码字号。独立术语与缩进说明恢复为定义条目，续行保留悬挂缩进并随阅读字号缩放；复制时保留叠加的代码与强调格式。恢复仍以原 PDF 和存储原文一致为前提，无需重新导入，不修改原文、批注或用户修订。验收见 [字体与术语缩进](evidence/pdf-typography-definitions.md)。

PDF 提示框支持浅色底、独立边线、短正文和跨页续文，恢复为可选的原生段落；图注排除在邻近图形截图范围之外。代码背景色保留为文字高亮，正文里的 `__init__` 等符号按原字符显示。实书扫描及回归证据见 [提示框与图注](evidence/pdf-callout-captions.md)。

PDF 排版改为提前准备：当前页优先，沿实际翻页方向预取后续 16 个可恢复页，并保留反方向 4 页的准备窗口，再在后台继续准备全书；每批最多 4 页，批间重新按当前位置排优先级。临近页面同时提前解析正文／英文对照的文本 AST，中文译文和用户修订无需等待原文 PDF 恢复。排版结果写入每篇文献的 `.native-pdf-layout-cache`，重开可直接复用；缓存按 PDF 路径、大小、修改时间、页面原文和排版版本区分，损坏时重建，不修改 PDF、`paper.json` 或批注记录。该缓存可删除，删除后会重新准备。首次打开未缓存的书或瞬间跳到尚未准备的远处页面，仍可能短暂显示准备提示。实书性能与测试边界见 [排版预取与持久缓存](evidence/pdf-layout-prefetch.md)。

## Rust CLI

```sh
cargo run --locked -- --library ./my-library --import /path/to/paper.pdf --headless
cargo run --locked -- --library ./my-library --import /path/to/book.epub --headless
cargo run --locked -- --library ./my-library --list
cargo run --locked -- --library ./my-library --migrate-copy /path/to/old-paper --headless
cargo run --locked -- --library ./my-library --paper PAPER_ID --export ./state.json --headless
cargo run --locked -- --library ./my-library --paper PAPER_ID --ops ./ops.json --headless
cargo run --locked -- --library ./my-library --paper PAPER_ID --complete-images
cargo run --locked -- --formula '\frac{1}{2}' --formula-out ./formula.svg
cargo run --locked -- --state fixtures/state.json --check-state
```

`ops.json` 是原格式操作数组，支持 `edit`、`note`、`note_del`、`paper_note`、`progress`。写入保留未知 JSON 字段、使用旧锁协议、原子替换和修订备份，并记录操作日志。日志是审计记录，当前不作为自动恢复 WAL。

## EPUB 初步支持与可行性验证

导入入口按文件签名识别 PDF / EPUB。EPUB 2/3 的无 DRM 流式书籍按 OPF spine 顺序解析 XHTML，复用原生正文、选区、批注和修订界面；支持常用标题、强调、列表、引文、代码、简单表格和 PNG / JPEG / WebP / GIF 图片。目录优先读取 EPUB 3 nav 或 EPUB 2 NCX，书内链接跳到对应内容块，图片支持现有大图预览路径。原 EPUB 完整保存为 `source.epub`，图片保存到书籍目录内。失败或取消不发布半成品，重复导入保留已有数据。

这是**语义重排**，不是浏览器或出版社版式的一比一还原：不执行脚本，不加载外部图片，不应用出版社 CSS 和内嵌字体；DRM 与固定版式 EPUB 明确拒绝，SVG / MathML 完整呈现、复杂合并单元格、CFI 和非线性附录浏览尚未实现。混在段落内的图片目前放在该内容块之后。EPUB 没有 PDF 式固定原页，源数据仍保持 `page=0`、`meta.pages=[]`，原页按钮隐藏；阅读界面现在根据窗口、字体、字号和行距进行**动态分页**，显示当前页 / 总页数，支持页脚按钮、← / →、PageUp / PageDown、`[` / `]` 和滚轮 / 触控板翻页。长段落按原生排版的实际行边界跨页，spine 章节从新页开始；后台解析与渐进测量期间只显示“正在排版”，不展示临时总页数；整书排版完成后一次性发布固定页面表，普通翻页不重新测量整书或重算总页数。临时页码不写入原书，导航和批注仍使用块 ID，阅读位置保留块内偏移。实现与边界见 [EPUB 动态分页](docs/epub-pagination.md)。spine 章节文件编号不等同于书中的印刷章号。为兼容 schema-2，原文暂存于既有 `en` 字段，真实语言记录在 `meta.language`，该字段不意味着原文一定是英文。原文使用清理后的 HTML，翻译和用户修订仍走 Markdown 呈现。

EPUB 代码块现在保留出版社的 `data-code-language` / `language-*` 标记，整块进行语法高亮；已有书籍没有标记时，按明确语法保守识别 Python、Rust、JSON、JavaScript 和 shell；Python 样例占明显多数的 EPUB 还可为赋值、普通调用等短片段补充显示层语言提示，不把张量打印结果、Traceback 或普通输出一律当成 Python，也不覆盖显式语言。无需重新导入，规则及实书验证见 [Python EPUB 高亮补充](evidence/epub-python-highlighting.md)。Figure 图片与 PDF 共用显示层白底适配，让边缘连通的大片白底透出阅读器底色，保留图内封闭白色与彩色区域；大图预览共用处理后的图片，不改写书库原图。修复及回归记录见 [EPUB 代码与 Figure 显示](evidence/epub-code-figures.md)。

可复现的原创测试 EPUB（不需要 PDFium 或商业书籍）：

```sh
cargo test --locked --test epub --test epub_security
cargo test --locked --lib epub_pagination
cargo test --locked --features desktop-ui-tests --bin easyread-gpui native_epub_pages
cargo test --locked --features desktop-ui-tests --bin easyread-gpui completed_epub_page_map
cargo test --locked --features desktop-ui-tests --bin easyread-gpui epub_imported_semantics
cargo run --locked --example epub_qa
# 上一命令会输出打开独立 target/epub-qa/library 的命令
```

实际样本、原生窗口级绘制测试与支持边界见 [EPUB 导入与渲染验证](evidence/epub-import.md)。QA 原书副本、书库和完整输出仅写到 `target/epub-qa/`，不纳入 Git。

## AI 配置与范围

设置 `EASYREAD_API_BASE`（默认 `https://api.openai.com/v1`）、`EASYREAD_MODEL`、`EASYREAD_API_KEY` 后，从终端启动应用。当前实现 OpenAI-compatible Chat Completions SSE，以及设置中的 Claude Code / Codex CLI 非交互适配；CLI 使用本机登录，测试仅使用模拟 CLI。原项目的全部模型提供商及 v1.4.1 新增的 Grok Build / Antigravity / Cursor CLI 尚未迁入。密钥不写入论文数据。

```sh
cargo run --locked -- --library ./my-library --paper PAPER_ID --ask '解释主要结论' --thread t-example
cargo run --locked -- --library ./my-library --paper PAPER_ID --translate
cargo run --locked -- --library ./my-library --paper PAPER_ID --translate-pages 2,3,7
```

翻译按有界并行的页批次补充未译段落/标题，保留 block id、已有译文及 `reader.json` 用户修订。漏项或空译文不会算成功：有效部分先保存，漏页单独补译一次，仍失败的页/块记录在 `job.json`，后续批次继续执行；再次翻译只发送剩余未译块，取消不保存当前响应。首页单选一篇 PDF → Translation →「选未译页翻译…」可勾选源页，已译页不列出；CLI `--translate-pages 2,3,7` 提供相同路径，页码从 1 开始，无效页在调用模型前拒绝。EPUB 没有固定源页，使用普通 `--translate`。首页按正文实际译文与任务记录显示覆盖，不再信任旧的误记完成页。native PDF 翻译现在从原页恢复的标题/段落/图注骨架构建输入，使用不可改写标记保护 TeX、图片位置、代码与格式；丢失结构标记会报失败，不保存扁平结果。正文 `zh` 保留语义 Markdown，原生布局附带源/译文哈希保存，用户修订仍优先。已有保留逐行边界的旧译文可在显示层恢复独立标题、居中题注、配图顺序与源页确认的数学，不重译、不改写原数据；已完全丢失边界的译文不猜测恢复。修复与边界见 [译文排版修复](evidence/translation-layout.md)。该流程仍是中文文本翻译，不等同于原项目的完整多模态重排、跨页续段或多语言流程；任务可靠性验证见 [翻译回归记录](evidence/translation-milestone.md)。

## 软件更新与发行

项目仓库为 <https://github.com/ohmycloud/vellio>。桌面 Settings → **About / Updates**、设置底栏及首页问号菜单均可进入更新页；Web 设置也有对应入口。支持手动检查和可关闭的每日自动检查，显示当前版本、新版说明与匹配系统/CPU 的下载按钮；离线、GitHub 限流、暂无 Release 会分别处理。更新检查不上传书籍、笔记或模型配置。

当前采用 **检查 → 浏览器下载 → 退出后替换程序**，不静默覆盖运行中的应用，也不修改书库。已补充 macOS Intel / Apple Silicon、Windows x64、Linux x64 的原生打包 workflow；macOS 参考 EasyRead 使用独立临时钥匙串签名，可配置 Developer ID 公证；Windows 支持可选 Authenticode 签名。发布流程先生成草稿 Release，需各平台实际验收后人工发布，并非已发布的现成安装包。版本/包名约定、所需 GitHub Secrets、签名限制及测试范围见 [软件更新与发行](docs/updates-and-releases.md)。Web 服务自身仍需更新源码并重新编译，不会被桌面安装包替换。

## 验证与本地 app

```sh
cargo test --locked --all-targets --features desktop
RUST_MIN_STACK=16777216 cargo test --locked --features desktop-ui-tests --bin easyread-gpui
cargo clippy --locked --all-targets --features desktop -- -D warnings
cargo fmt --check
cargo run --locked --example rust_smoke
cargo run --locked --example reader_r3_qa
cargo run --locked --features desktop -- --library target/rust-smoke/library --paper 156c77a35261
sh scripts/package_macos.sh
```

测试会启动随机 loopback HTTP 端口作为明确的 AI 测试桩，不调用真实模型。`rust_smoke` 使用 Rust 生成的 PDF 和合成旧数据，重建测试库中的 paper.json；测试数据位于 `target/rust-smoke`。示例结果写入 `evidence/rust-smoke.json`、`evidence/rust-state.json`、`evidence/rust-formula.svg`。本地开发 app 位于 `target/Vellio.app`，捆绑 PDFium，字体与 MiTeX/Typst 资源已内嵌。该包仅使用本地临时签名，未经发行签名和公证，尚未作为发行版发布。

macOS 应用包采用橙喙鹈鹕配雾蓝灰背景的图标（方案 A），覆盖 16–1024 px 的普通与 Retina 尺寸，16 px 使用强化眼睛与嘴线的小尺寸版本。文献库顶部同步显示全彩图标，浅色 / 深色主题都不重新着色。可编辑源文件与生成方法见 [应用图标](assets/icons/README.md)。

`reader_r3_qa` 在独立 `target/reader-r3-qa` 库重建两页双栏/四色验收数据，并输出启动命令；不使用用户库。实机结果见 [R3 验收记录](evidence/reader-r3.md)。旧 `scripts/backend_smoke.py` 和旧 `evidence/backend-*` 仅保留为迁移前的兼容性参考，不是新应用运行依赖。
