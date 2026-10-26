// PDF/EPUB figures: real imports, scoped URLs, safe native PNGs, no duplicates.
// Optional EASYREAD_QA_BOOK=/path/to/imported/epub copies only its data/images for read-only QA.
// cargo build --locked --manifest-path web-topcoat/Cargo.toml && node web-topcoat/images-regression.mjs
import {spawn, spawnSync} from 'node:child_process';
import {createServer} from 'node:net';
import {readFileSync, writeFileSync, mkdtempSync, mkdirSync, cpSync, symlinkSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

const delay = ms => new Promise(done => setTimeout(done, ms));
const directory = mkdtempSync(join(tmpdir(), 'vellio-images-'));
const library = join(directory, 'library');
mkdirSync(library);
// A small EPUB with an actual raster image and a semantic Figure caption.
const epub = join(directory, 'illustrated-book.epub');
const generated = spawnSync('python3', ['-c', `
import sys,zipfile,struct,zlib,binascii,pathlib
root=pathlib.Path(sys.argv[1]).parent
chunk=lambda kind,data:struct.pack('>I',len(data))+kind+data+struct.pack('>I',binascii.crc32(kind+data)&0xffffffff)
w,h=160,80
pixels=b''.join(b'\\x00'+bytes([40,120,160])*w for _ in range(h))
png=b'\\x89PNG\\r\\n\\x1a\\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')
(root/'picture.png').write_bytes(png)
with zipfile.ZipFile(sys.argv[1],'w') as z:
 z.writestr('mimetype','application/epub+zip',compress_type=zipfile.ZIP_STORED)
 z.writestr('META-INF/container.xml','<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
 z.writestr('OEBPS/package.opf','<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">illustrated-book</dc:identifier><dc:title>Illustrated book</dc:title><dc:language>en</dc:language></metadata><manifest><item id="ch" href="Text/ch.xhtml" media-type="application/xhtml+xml"/><item id="pic" href="Images/picture.png" media-type="image/png"/></manifest><spine><itemref idref="ch"/></spine></package>')
 z.writestr('OEBPS/Text/ch.xhtml','<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title></head><body><h1>Illustrated chapter</h1><figure><img src="../Images/picture.png" alt="A blue diagram"/><figcaption>Figure 1. A book illustration.</figcaption></figure><p>The next paragraph.</p></body></html>')
 z.writestr('OEBPS/Images/picture.png',png)
`, epub], {encoding: 'utf8'});
assert.equal(generated.status, 0, generated.stderr);

function illustratedPdf() {
  const stream = text => `<< /Length ${Buffer.byteLength(text)} >>\nstream\n${text}\nendstream`;
  const pixels = '2878a02878a02878a02878a0>';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 5 0 R >> >> /Contents 6 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>',
    `<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length ${pixels.length} >>\nstream\n${pixels}\nendstream`,
    stream('BT /F1 12 Tf 72 700 Td (This book chapter explains a diagram.) Tj ET\nq 360 0 0 180 100 450 cm /Im1 Do Q\nBT /F1 12 Tf 100 420 Td (Figure 1. A colored diagram.) Tj ET'),
  ];
  let pdf = '%PDF-1.4\n'; const offsets = [0];
  objects.forEach((object, index) => {offsets.push(Buffer.byteLength(pdf)); pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;});
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  pdf += offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('');
  return Buffer.from(pdf + `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
}
const listener = createServer();
await new Promise(done => listener.listen(0, '127.0.0.1', done));
const port = listener.address().port;
await new Promise(done => listener.close(done));
const server = spawn(resolve('web-topcoat/target/debug/vellio-web-topcoat'), [], {
  env: {...process.env, VELLIO_WEB_LIBRARY: library, VELLIO_DESKTOP_LIBRARY: library,
    HOST: '127.0.0.1', PORT: String(port), EASYREAD_MODEL: ''}, stdio: 'ignore',
});
try {
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(base)).ok) break; } catch {}
    await delay(100);
  }
  async function upload(bytes, filename, type) {
    const form = new FormData(); form.append('book', new Blob([bytes], {type}), filename);
    const response = await fetch(`${base}/import`, {method: 'POST', body: form, redirect: 'manual'});
    assert.equal(response.status, 303);
    return response.headers.get('location').split('/').at(-1);
  }
  const epubId = await upload(readFileSync(epub), 'illustrated-book.epub', 'application/epub+zip');
  const pdfId = await upload(illustratedPdf(), 'illustrated-book.pdf', 'application/pdf');
  const epubRoot = join(library, epubId), pdfRoot = join(library, pdfId);
  const epubPaper = JSON.parse(readFileSync(join(epubRoot, 'paper.json'), 'utf8'));
  const epubFigure = epubPaper.blocks.find(block => block.type === 'figure');
  assert(epubFigure?.src.startsWith('images/'));
  epubFigure.caption_zh = '图 1. 书籍插图。';
  const pdfPaper = JSON.parse(readFileSync(join(pdfRoot, 'paper.json'), 'utf8'));
  const pdfFigures = pdfPaper.blocks.filter(block => block.type === 'figure');
  assert(pdfFigures.some(block => block.src.startsWith('figures/')), 'real PDF import must produce a figure crop');
  for (const [id, figures] of [[epubId, [epubFigure]], [pdfId, pdfFigures]]) {
    for (const figure of figures) {
      const response = await fetch(`${base}/read/${id}/${figure.src}`);
      assert.equal(response.status, 200, figure.src);
      assert.match(response.headers.get('content-type'), /^image\//);
      assert((await response.arrayBuffer()).byteLength > 0);
    }
  }
  // Existing books may contain spaces, non-ASCII names, uppercase extensions,
  // and captions in HTML. They must use the same resolver as figure blocks.
  const special = '图 1 #+.PNG';
  cpSync(join(directory, 'picture.png'), join(epubRoot, 'images', special));
  epubPaper.blocks.push({id: 'legacy-inline', type: 'para', chapter: 1, page: 0,
    source_kind: 'epub-xhtml', en: 'Legacy inline illustration.', zh: '',
    html: `<p>Legacy inline illustration.<img src="images/${special}" alt="Legacy image"/></p>`});
  writeFileSync(join(epubRoot, 'paper.json'), JSON.stringify(epubPaper));
  const before = readFileSync(join(epubRoot, 'paper.json'), 'utf8');
  writeFileSync(join(directory, 'private.png'), 'not a book image');
  symlinkSync(join(directory, 'private.png'), join(epubRoot, 'images', 'outside.png'));
  for (const path of ['source.epub', 'paper.json', 'images/outside.png', 'images/%2e%2e%2fpaper.json', 'figures/no.svg']) {
    assert.equal((await fetch(`${base}/read/${epubId}/${path}`)).status, 404, path);
  }
  const cases = [[epubId, 1, 2], [pdfId, 1, 1]];
  if (process.env.EASYREAD_QA_BOOK) {
    const source = resolve(process.env.EASYREAD_QA_BOOK), target = join(library, 'qa-book');
    mkdirSync(target);
    const paper = JSON.parse(readFileSync(join(source, 'paper.json'), 'utf8'));
    assert.equal(paper.meta.format, 'epub', 'optional QA expects an EPUB');
    writeFileSync(join(target, 'paper.json'), JSON.stringify(paper));
    writeFileSync(join(target, 'reader.json'), JSON.stringify({schema: 2, edits: {}, notes: {}, progress: {}}));
    cpSync(join(source, 'images'), join(target, 'images'), {recursive: true});
    const match = paper.blocks.find(block => block.caption_en?.includes('The eight pillars of agentic graph architecture'))
      || paper.blocks.find(block => block.type === 'figure');
    assert(match);
    cases.push(['qa-book', match.chapter, paper.blocks.filter(block => block.type === 'figure' && block.chapter === match.chapter).length]);
  }
  for (const [id, page, expected] of cases) {
    const url = `${base}/read/${id}?page=${page}`;
    await withBrowser(url, async ({evaluate, wait, send}) => {
      if (id === pdfId) {
        // Exercise an actually translated page, not only an untranslated fallback.
        const source = await evaluate(`(async () => {const doc = new DOMParser().parseFromString(await (await fetch(location.href)).text(), 'text/html');
          return doc.querySelector('.blk[data-page-block="true"] > .source').innerHTML;})()`);
        const block = pdfPaper.blocks.find(block => block.source_kind === 'pdf-text');
        const hash = text => {let value = 0x811c9dc5; for (let i = 0; i < text.length; i++) value = Math.imul(value ^ text.charCodeAt(i), 0x01000193) >>> 0; return value.toString(16).padStart(8, '0');};
        block.zh = ['本章介绍示意图。', '图 1. 彩色示意图。'].join('\n\n');
        block.native_translation_layout = {version: 1, source_hash: hash(block.en), translation_hash: hash(block.zh),
          html: source.replace('This book chapter explains a diagram.', '本章介绍示意图。').replace('Figure 1. A colored diagram.', '图 1. 彩色示意图。')};
        writeFileSync(join(pdfRoot, 'paper.json'), JSON.stringify(pdfPaper));
      }
      for (const mode of ['original', 'translation', 'bilingual']) {
        await send('Page.navigate', {url: `${url}&mode=${mode}`});
        await wait(`document.querySelector('.paper')?.dataset.ready === 'true' && document.querySelector('.paper').classList.contains('mode-${mode}')`);
        try {
          await wait(`(() => {const images = [...document.querySelectorAll('.paper img:not(.math-svg)')].filter(image => image.getClientRects().length);
            return images.length === ${expected} && images.every(image => image.complete && image.naturalWidth > 0);})()`);
        } catch (error) {
          console.error(id, mode, await evaluate(`[...document.querySelectorAll('.paper img')].map(image => ({src: image.getAttribute('src')?.slice(0, 100), visible: !!image.getClientRects().length, decoded: image.naturalWidth, parent: image.parentElement.outerHTML.slice(0, 200)}))`));
          throw error;
        }
        const images = await evaluate(`(() => {const images = [...document.querySelectorAll('.paper img:not(.math-svg)')].filter(image => image.getClientRects().length);
          return images.map(image => ({src: image.getAttribute('src'), top: image.getBoundingClientRect().top,
            bottom: image.getBoundingClientRect().bottom, width: image.getBoundingClientRect().width}));})()`);
        assert.equal(images.length, expected, `${id}/${mode}: duplicate or missing figure`);
        assert(images.every(image => image.width > 0));
        assert(images.every(image => image.src.startsWith(`/read/${id}/`) || image.src.startsWith('data:image/png;base64,')));
        assert.equal(await evaluate('document.querySelector("img[alt=文献插图]")'), null);
        if (id === epubId) {
          if (mode !== 'original') assert.equal(await evaluate('document.querySelector(".blk-figure > .translated").textContent.includes("书籍插图")'), true);
          assert.equal(await evaluate(`(() => {const block = document.getElementById('block-${epubFigure.id}');
            const image = block.querySelector(':scope > img');
            const caption = [...block.querySelectorAll('.source, .translated')].find(node => node.getClientRects().length);
            return image.getBoundingClientRect().bottom <= caption.getBoundingClientRect().top;})()`), true, 'figure must precede its caption');
        }
        if (id === pdfId) {
          assert(images[0].src.startsWith('data:image/png;base64,'), 'native PDF crop must survive sanitization');
          assert.equal(await evaluate('document.querySelectorAll(".book-illustration").length'), 0, 'native crop replaces the legacy standalone block');
        }
        if (mode === 'bilingual') {
          for (const order of ['translation', 'source']) {
            await evaluate(`document.querySelector('[data-reading-order="${order}"]').click(); true`);
            assert.equal(await evaluate(`[...document.querySelectorAll('.paper img:not(.math-svg)')].filter(image => image.getClientRects().length).length`), expected,
              `${id}: ${order}-first must show each image once`);
          }
        }
      }
      if (id === 'qa-book') {
        await evaluate(`(() => {const figures = [...document.querySelectorAll('.blk-figure')];
          const figure = figures.find(node => node.textContent.includes('The eight pillars of agentic graph architecture')) || figures[0];
          figure.scrollIntoView({block: 'center', behavior: 'instant'}); return true;})()`);
        const shot = await send('Page.captureScreenshot', {format: 'png'});
        writeFileSync('/tmp/vellio-web-book-figure.png', Buffer.from(shot.result.data, 'base64'));
      }
    });
  }
  assert.equal(readFileSync(join(epubRoot, 'paper.json'), 'utf8'), before, 'rendering must not rewrite the imported book');
  const home = await (await fetch(base)).text();
  assert.match(home, /Vellio · 书库/);
  assert.match(home, /全部书籍/);
  console.log('PASS: EPUB/PDF figures decode above captions in all modes; native crops are safe and not duplicated; encoded image paths work; private assets stay inaccessible; existing book data is unchanged.');
} finally {
  server.kill();
  await delay(250);
  rmSync(directory, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
