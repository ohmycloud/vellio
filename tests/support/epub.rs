//! Original, reproducible EPUB fixtures; no commercial book content.
use std::{
    fs::File,
    io::{Cursor, Write},
    path::Path,
};
use zip::{ZipWriter, write::SimpleFileOptions};

pub fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        let options = SimpleFileOptions::default().compression_method(if *name == "mimetype" {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        });
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

pub fn fixture(path: &Path, epub2: bool) {
    let version = if epub2 { "2.0" } else { "3.0" };
    let navigation = if epub2 {
        r#"<item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#
    } else {
        r#"<item id="toc" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="book">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book">vellio-original-fixture</dc:identifier>
<dc:title>EPUB 可行性验证</dc:title><dc:creator>Vellio QA</dc:creator><dc:language>zh-CN</dc:language><meta name="cover" content="cover"/></metadata>
<manifest><item id="second" href="Text/second.xhtml" media-type="application/xhtml+xml"/>
<item id="first" href="Text/first.xhtml" media-type="application/xhtml+xml"/>{navigation}
<item id="cover" href="Images/cover.jpg" media-type="image/jpeg"/>
<item id="diagram" href="Images/diagram.png" media-type="image/png"/>
<item id="css" href="Styles/book.css" media-type="text/css"/></manifest>
<spine toc="toc"><itemref idref="first"/><itemref idref="second"/></spine></package>"#
    );
    let container = br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let first = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>第一章</title><link rel="stylesheet" href="../Styles/book.css"/></head>
<body><section id="chapter-one"><h1 id="start">第一章：流式阅读</h1>
<p id="intro">中文与 English 混排，<strong>粗体</strong>、<em>斜体</em>和 <code>let x = 1;</code>，实体 &amp; 与 &lt;。</p>
<p><a href="second.xhtml#target">跳到第二章</a> / <a href="https://example.com/">外部链接</a> / <a href="javascript:alert(1)" onclick="bad()">不安全链接</a></p>
<h2 id="details">结构化内容</h2><ul><li>第一项</li><li>第二项<ul><li>嵌套项</li></ul></li></ul>
<ol><li>步骤一</li><li>步骤二</li></ol><blockquote><p>独立引文。</p></blockquote>
<pre><code class="language-rust">fn main() {
    println!("hello EPUB");
}</code></pre>
<pre data-code-language="python"><code class="kn">from</code> datetime <code class="kn">import</code> date

<code class="k">def</code> today():
    <code class="k">return</code> "中文" # comment
</pre>
<pre>def legacy():
    return "unlabeled code"
</pre>
<table><thead><tr><th>名称</th><th>数值</th></tr></thead><tbody><tr><td>Alpha</td><td>42</td></tr></tbody></table>
<figure id="diagram"><img src="../Images/diagram.png" alt="测试图片"/><figcaption>图 1：原生图片</figcaption></figure>
<div class="browsable-container figure-container" id="publisher-diagram"><img src="../Images/diagram.png" alt="figure"/><h5 class="figure-container-h5"><span class="num-string">Figure 2.1</span> A caption with an <em>italic</em> word and <strong>explicit emphasis</strong>.</h5></div>
<h5 id="real-subheading">Figure 9.1 is discussed in this section</h5>
<script>window.bad = true;</script><iframe src="https://example.com/"></iframe></section></body></html>"#;
    // Some publishers encode figure captions as h6 rather than figcaption.
    let first = if epub2 {
        first.replace("figcaption", "h6")
    } else {
        first.to_owned()
    };
    let second = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>第二章</title></head><body><h1 id="target">第二章：链接目标</h1><p>第二章正文，按 spine 顺序显示。</p><p><a href="first.xhtml#intro">回到第一章</a></p></body></html>"#;
    let nav = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>目录</title></head><body><nav epub:type="toc"><ol>
<li><a href="Text/first.xhtml#start">第一章</a><ol><li><a href="Text/first.xhtml#details">结构化内容</a></li></ol></li>
<li><a href="Text/second.xhtml#target">第二章</a></li></ol></nav></body></html>"#;
    let ncx = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap><navPoint id="one"><navLabel><text>第一章</text></navLabel><content src="Text/first.xhtml#start"/><navPoint id="details"><navLabel><text>结构化内容</text></navLabel><content src="Text/first.xhtml#details"/></navPoint></navPoint><navPoint id="two"><navLabel><text>第二章</text></navLabel><content src="Text/second.xhtml#target"/></navPoint></navMap></ncx>"#;
    let rgb = image::RgbImage::from_fn(160, 100, |x, y| {
        if !(10..150).contains(&x) || !(10..90).contains(&y) || (x == 80 && y == 50) {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([40 + (x / 2) as u8, 100 + (y / 2) as u8, 120])
        }
    });
    let mut png = Cursor::new(Vec::new());
    rgb.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut jpeg = Cursor::new(Vec::new());
    rgb.write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
    archive(
        path,
        &[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", opf.as_bytes()),
            // Deliberately place the second chapter first in the archive/manifest.
            ("OEBPS/Text/second.xhtml", second.as_bytes()),
            ("OEBPS/Text/first.xhtml", first.as_bytes()),
            (
                if epub2 {
                    "OEBPS/toc.ncx"
                } else {
                    "OEBPS/nav.xhtml"
                },
                if epub2 {
                    ncx.as_bytes()
                } else {
                    nav.as_bytes()
                },
            ),
            ("OEBPS/Images/diagram.png", png.get_ref()),
            ("OEBPS/Images/cover.jpg", jpeg.get_ref()),
            ("OEBPS/Styles/book.css", b"body { font-size: 999px; }"),
        ],
    );
}
