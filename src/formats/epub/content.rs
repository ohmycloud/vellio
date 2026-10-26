//! Walk spine content into stable source blocks and local image assets.
use super::*;

pub(super) struct Content {
    pub(super) blocks: Vec<Value>,
    pub(super) targets: BTreeMap<String, String>,
    pub(super) images: HashMap<String, (String, u32, u32)>,
    pub(super) warnings: BTreeSet<String>,
    pub(super) chapter: usize,
}
impl Content {
    pub(super) fn image<R: Read + Seek>(
        &mut self,
        archive: &mut Archive<R>,
        root: &Path,
        path: &str,
    ) -> Result<(String, u32, u32)> {
        if let Some(image) = self.images.get(path) {
            return Ok(image.clone());
        }
        let bytes = archive.read(path)?;
        let format = image::guess_format(&bytes)?;
        let extension = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::WebP => "webp",
            image::ImageFormat::Gif => "gif",
            _ => bail!("Unsupported EPUB image format: {path}"),
        };
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 100_000_000,
            "EPUB image dimensions exceed limit"
        );
        let relative = format!("images/epub-{}.{}", token(path), extension);
        fs::create_dir_all(root.join("images"))?;
        fs::write(root.join(&relative), &bytes)?;
        let src = relative;
        let image = (src, w, h);
        self.images.insert(path.to_owned(), image.clone());
        Ok(image)
    }
    pub(super) fn register(&mut self, node: Node<'_, '_>, href: &str, id: &str) {
        self.targets
            .entry(href.to_owned())
            .or_insert_with(|| id.to_owned());
        for n in node.descendants().chain(node.ancestors()) {
            if let Some(fragment) = n
                .attribute("id")
                .or_else(|| n.attribute(("http://www.w3.org/XML/1998/namespace", "id")))
            {
                self.targets
                    .entry(target_key(href, Some(fragment)))
                    .or_insert_with(|| id.to_owned());
            }
        }
    }
    pub(super) fn add(
        &mut self,
        node: Node<'_, '_>,
        href: &str,
        dom: &str,
        kind: &str,
        source: String,
        html: String,
    ) {
        if source.trim().is_empty() {
            return;
        }
        let id = format!("epub-{}", token(&format!("{href}:{dom}")));
        let caption = kind == "figure-caption";
        let kind = if caption { "para" } else { kind };
        let level = (kind == "heading")
            .then(|| {
                node.tag_name()
                    .name()
                    .strip_prefix('h')
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .flatten();
        self.register(node, href, &id);
        self.blocks
            .push(json!({"id":id,"type":kind,"page":0,"chapter":self.chapter,
            "source_kind":"epub-xhtml","en":source,"zh":"","html":html,"level":level,
            "source_location":{"href":href,"fragment":node.attribute("id"),"dom":dom}}));
        if caption {
            self.blocks.last_mut().unwrap()["source_role"] = json!("figure-caption");
        }
    }
    pub(super) fn walk<R: Read + Seek>(
        &mut self,
        node: Node<'_, '_>,
        href: &str,
        dom: &str,
        archive: &mut Archive<R>,
        root: &Path,
        cancel: &AtomicBool,
    ) -> Result<()> {
        cancelled(cancel)?;
        let tag = node.tag_name().name();
        if matches!(
            tag,
            "head" | "script" | "style" | "iframe" | "object" | "audio" | "video"
        ) {
            return Ok(());
        }
        if node.attribute("hidden").is_some() {
            return Ok(());
        }
        if matches!(tag, "img" | "image") {
            let src = node
                .attribute("src")
                .or_else(|| node.attribute("href"))
                .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(src) = src {
                match resolve(href, src).and_then(|(path, _)| self.image(archive, root, &path)) {
                    Ok((src, width, height)) => {
                        let id = format!("epub-{}", token(&format!("{href}:{dom}")));
                        self.register(node, href, &id);
                        let caption = figure_caption(node)
                            .map(|n| compact(&text(n)))
                            .unwrap_or_default();
                        self.blocks.push(json!({"id":id,"type":"figure","page":0,"chapter":self.chapter,
                            "source_kind":"epub-image","src":src,"width":width,"height":height,
                            "caption_en":caption,"caption_zh":"","alt":node.attribute("alt").unwrap_or(""),
                            "source_location":{"href":href,"dom":dom}}));
                    }
                    Err(e) => {
                        self.warnings.insert(format!("Image skipped: {src}: {e}"));
                        let alt = node.attribute("alt").unwrap_or("Image unavailable");
                        self.add(
                            node,
                            href,
                            dom,
                            "para",
                            alt.to_owned(),
                            format!("<p>{}</p>", escape(alt)),
                        );
                    }
                }
            }
            return Ok(());
        }
        if figure_caption(node) == Some(node) {
            return Ok(());
        }
        let semantic = matches!(
            tag,
            "h1" | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "pre"
                | "ul"
                | "ol"
                | "dl"
                | "table"
                | "blockquote"
        );
        if semantic {
            let caption = publisher_figure_caption(node);
            let heading = !caption && matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
            let source = if heading {
                compact(&text(node))
            } else {
                markdown(node, false)
            };
            self.add(
                node,
                href,
                dom,
                if caption {
                    "figure-caption"
                } else if heading {
                    "heading"
                } else {
                    "para"
                },
                source.trim().to_owned(),
                if caption {
                    caption_html(render(node, href))
                } else {
                    render(node, href)
                },
            );
            // Ordinary EPUB illustrations are block images. Mixed inline images
            // are retained after their containing semantic block, not discarded.
            for (i, image) in node
                .descendants()
                .filter(|n| n.has_tag_name("img") || n.has_tag_name("image"))
                .enumerate()
            {
                self.walk(
                    image,
                    href,
                    &format!("{dom}/image-{i}"),
                    archive,
                    root,
                    cancel,
                )?;
            }
            return Ok(());
        }
        if tag == "figcaption" && node.ancestors().any(|n| n.has_tag_name("figure")) {
            return Ok(());
        }
        let mut loose = String::new();
        for (i, child) in node.children().enumerate() {
            if child.is_text() {
                loose.push_str(child.text().unwrap_or(""));
            } else if child.is_element() {
                if !loose.trim().is_empty() {
                    let source = compact(&loose);
                    self.add(
                        node,
                        href,
                        &format!("{dom}/text-{i}"),
                        "para",
                        source.clone(),
                        format!("<p>{}</p>", escape(&source)),
                    );
                }
                loose.clear();
                self.walk(child, href, &format!("{dom}/{i}"), archive, root, cancel)?;
            }
        }
        if !loose.trim().is_empty() {
            let source = compact(&loose);
            self.add(
                node,
                href,
                &format!("{dom}/text-end"),
                "para",
                source.clone(),
                format!("<p>{}</p>", escape(&source)),
            );
        }
        Ok(())
    }
}
