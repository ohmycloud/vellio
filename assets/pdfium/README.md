# PDFium runtime asset

本地验证提供 macOS ARM64 的 `libpdfium.dylib`，PDFium build **7999**（153.0.7999.0），来自此前验证安装的 `pypdfium2 5.13.0` wheel 内的原生二进制（origin: pdfium-binaries）。这里只复用原生库，没有导入、启动或打包 Python。

SHA-256: `33c98063af28c0b7cbf8227f4422bf5c15942df2455cf7f0a5dce3dc601d52b0`。

Rust wrapper 固定 `pdfium-render 0.9.4`，绑定 ABI feature `pdfium_7881`，已在该 build 上执行抽字、四种旋转、字符坐标、WebP 渲染测试。程序按以下顺序选择库：显式 `--pdfium` / `EASYREAD_PDFIUM_LIBRARY`，可执行文件同目录，本工程 `assets/pdfium`。

PDFium 为 Chromium 的 C++ 引擎；“Rust 化”指应用和服务层移除 Python，不声称 PDF 底层引擎是纯 Rust。Linux/Windows/macOS x86_64 原生库与安装包尚需按各目标提供及验证。不同进程内不得混装不兼容的 ABI；同一进程使用首个成功绑定的库。

原生 PDFium 及其构建依赖的许可证保存在 [licenses](licenses)。来源参考：[pdfium-render 官方仓库](https://github.com/ajrcarey/pdfium-render)、[PDFium 二进制发行项目](https://github.com/bblanchon/pdfium-binaries)、[pypdfium2](https://github.com/pypdfium2-team/pypdfium2)。
