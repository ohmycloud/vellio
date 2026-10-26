// Source-faithful nested EPUB3 nav / EPUB2 NCX / PDF bookmarks, not pagination.
// cargo build --locked --manifest-path web-topcoat/Cargo.toml && node web-topcoat/contents-regression.mjs
// Optional EASYREAD_QA_BOOK=/path/to/imported/epub: read-only copy + compare with original EPUB nav.
import {spawn, spawnSync} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdtempSync, mkdirSync, readFileSync, writeFileSync, cpSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

const delay = ms => new Promise(done => setTimeout(done, ms));
const temp = mkdtempSync(join(tmpdir(), 'vellio-contents-'));
const library = join(temp, 'library'); mkdirSync(library);
const generated = spawnSync('python3', ['-c', `
import sys,pathlib,zipfile,html
root=pathlib.Path(sys.argv[1])
labels=['Chapter 1 — Original title','1.1 Source subsection','1.1.1 Details & limits','Chapter 2','2.1 Source subsection','2.1.1 Source paragraph']
targets=['ch1.xhtml#chapter','ch1.xhtml#sub','ch1.xhtml#deep','ch2.xhtml#chapter','ch2.xhtml#sub','ch2.xhtml#paragraph']
children={0:[1],1:[2],3:[4],4:[5]}
def nav(i):
 return '<li><a href="'+targets[i]+'">'+html.escape(labels[i])+'</a>'+('<ol>'+''.join(nav(c) for c in children.get(i,[]))+'</ol>' if i in children else '')+'</li>'
def ncx(i):
 return '<navPoint id="n'+str(i)+'"><navLabel><text>'+html.escape(labels[i])+'</text></navLabel><content src="'+targets[i]+'"/>'+''.join(ncx(c) for c in children.get(i,[]))+'</navPoint>'
for kind in ['nav','ncx']:
 with zipfile.ZipFile(root/(kind+'.epub'),'w') as z:
  z.writestr('mimetype','application/epub+zip',compress_type=zipfile.ZIP_STORED)
  z.writestr('META-INF/container.xml','<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
  navigation='<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>' if kind=='nav' else '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
  z.writestr('OEBPS/package.opf','<package xmlns="http://www.idpf.org/2007/opf" version="'+('3.0' if kind=='nav' else '2.0')+'" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">test-'+kind+'</dc:identifier><dc:title>Nested book</dc:title><dc:language>en</dc:language></metadata><manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>'+navigation+'</manifest><spine'+(' toc="ncx"' if kind=='ncx' else '')+'><itemref idref="ch1"/><itemref idref="ch2"/></spine></package>')
  z.writestr('OEBPS/ch1.xhtml','<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Body title, not navigation label</title></head><body><h1 id="chapter">Body chapter one</h1><p>Introduction.</p><h2 id="sub">Body subsection one</h2><p>Subsection prose.</p><h3 id="deep">Nested body heading</h3><p>Details.</p><h6>Example 7-4, not in the original table of contents</h6></body></html>')
  z.writestr('OEBPS/ch2.xhtml','<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter two</title></head><body><h1 id="chapter">Body chapter two</h1><h2 id="sub">Body subsection two</h2><p id="paragraph">Paragraph target, not a heading.</p></body></html>')
  z.writestr('OEBPS/nav.xhtml','<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol>'+nav(0)+nav(3)+'</ol></nav></body></html>')
  z.writestr('OEBPS/toc.ncx','<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>'+ncx(0)+ncx(3)+'</navMap></ncx>')
`, temp], {encoding: 'utf8'});
assert.equal(generated.status, 0, generated.stderr);
function bookmarkedPdf() {
  const stream = text => `<< /Length ${Buffer.byteLength(text)} >>\nstream\n${text}\nendstream`;
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R /Outlines 8 0 R >>',
    '<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>',
    stream('BT /F1 12 Tf 72 700 Td (The first PDF chapter.) Tj ET'),
    stream('BT /F1 12 Tf 72 700 Td (The second PDF chapter.) Tj ET'),
    '<< /Type /Outlines /First 9 0 R /Last 12 0 R /Count 4 >>',
    '<< /Title (PDF chapter one) /Parent 8 0 R /Next 12 0 R /First 10 0 R /Last 10 0 R /Count 2 /Dest [3 0 R /Fit] >>',
    '<< /Title (1.1 Source PDF subsection) /Parent 9 0 R /First 11 0 R /Last 11 0 R /Count 1 /Dest [4 0 R /Fit] >>',
    '<< /Title (1.1.1 Nested PDF subsection) /Parent 10 0 R /Dest [4 0 R /Fit] >>',
    '<< /Title (PDF chapter two) /Parent 8 0 R /Prev 9 0 R /Dest [4 0 R /Fit] >>',
  ];
  let pdf = '%PDF-1.4\n'; const offsets = [0];
  objects.forEach((object, index) => {offsets.push(Buffer.byteLength(pdf)); pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;});
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  pdf += offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('');
  return Buffer.from(pdf + `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
}
const listener = createServer(); await new Promise(done => listener.listen(0, '127.0.0.1', done));
const port = listener.address().port; await new Promise(done => listener.close(done));
const server = spawn(resolve('web-topcoat/target/debug/vellio-web-topcoat'), [], {
  env: {...process.env, VELLIO_WEB_LIBRARY: library, VELLIO_DESKTOP_LIBRARY: library,
    HOST: '127.0.0.1', PORT: String(port), EASYREAD_MODEL: ''}, stdio: 'ignore',
});
try {
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) {try {if ((await fetch(base)).ok) break;} catch {} await delay(100);}
  async function upload(bytes, name) {
    const data = new FormData(); data.append('book', new Blob([bytes]), name);
    const response = await fetch(`${base}/import`, {method: 'POST', body: data, redirect: 'manual'});
    assert.equal(response.status, 303);
    return response.headers.get('location').split('/').at(-1);
  }
  const cases = [];
  const epubExpected = [
    {title: 'Chapter 1 — Original title', depth: 0}, {title: '1.1 Source subsection', depth: 1},
    {title: '1.1.1 Details & limits', depth: 2}, {title: 'Chapter 2', depth: 0},
    {title: '2.1 Source subsection', depth: 1}, {title: '2.1.1 Source paragraph', depth: 2},
  ];
  for (const kind of ['nav', 'ncx']) {
    const id = await upload(readFileSync(join(temp, `${kind}.epub`)), `${kind}.epub`);
    const paper = JSON.parse(readFileSync(join(library, id, 'paper.json'), 'utf8'));
    const first = paper.outline[0].block;
    writeFileSync(join(library, id, 'reader.json'), JSON.stringify({schema: 2, notes: {}, progress: {}, edits: {[first]: {zh: 'Edited heading must not rename the table of contents'}}}));
    assert.deepEqual(paper.outline.map(({title, depth}) => ({title, depth})), epubExpected);
    cases.push({id, page: 1, expected: epubExpected, paper});
  }
  const pdfId = await upload(bookmarkedPdf(), 'bookmarks.pdf');
  cases.push({id: pdfId, page: 1, expected: [
    {title: 'PDF chapter one', depth: 0}, {title: '1.1 Source PDF subsection', depth: 1},
    {title: '1.1.1 Nested PDF subsection', depth: 2}, {title: 'PDF chapter two', depth: 0},
  ]});
  const empty = join(library, 'no-toc'); mkdirSync(empty);
  writeFileSync(join(empty, 'paper.json'), JSON.stringify({schema: 2, meta: {format: 'epub', chapter_count: 1}, blocks: [{id: 'prose', type: 'para', chapter: 1, en: 'A book without navigation metadata.'}]}));
  writeFileSync(join(empty, 'reader.json'), JSON.stringify({schema: 2, notes: {}, edits: {}, progress: {}}));
  if (process.env.EASYREAD_QA_BOOK) {
    const source = resolve(process.env.EASYREAD_QA_BOOK), target = join(library, 'qa-book'); mkdirSync(target);
    const paper = JSON.parse(readFileSync(join(source, 'paper.json'), 'utf8'));
    writeFileSync(join(target, 'paper.json'), JSON.stringify(paper));
    writeFileSync(join(target, 'reader.json'), JSON.stringify({schema: 2, notes: {}, edits: {}, progress: {}}));
    cpSync(join(source, 'images'), join(target, 'images'), {recursive: true});
    // Read the publisher's actual EPUB3 navigation independently of paper.json.
    const original = spawnSync('python3', ['-c', `
import sys,zipfile,posixpath,json,xml.etree.ElementTree as E
with zipfile.ZipFile(sys.argv[1]) as z:
 container=E.fromstring(z.read('META-INF/container.xml')); opf=next(n.get('full-path') for n in container.iter() if n.tag.endswith('rootfile'))
 package=E.fromstring(z.read(opf)); item=next(n for n in package.iter() if n.tag.endswith('item') and 'nav' in n.get('properties','').split())
 path=posixpath.normpath(posixpath.join(posixpath.dirname(opf),item.get('href'))); doc=E.fromstring(z.read(path))
 nav=next(n for n in doc.iter() if n.tag.endswith('}nav') and n.get('{http://www.idpf.org/2007/ops}type')=='toc'); result=[]
 def walk(ol,depth):
  for li in ol:
   a=next((n for n in li if n.tag.endswith('}a')),None)
   if a is not None: result.append({'title':' '.join(''.join(a.itertext()).split()),'depth':depth})
   for child in li:
    if child.tag.endswith('}ol'): walk(child,depth+1)
 walk(next(n for n in nav if n.tag.endswith('}ol')),0)
 print(json.dumps(result))
`, join(source, 'source.epub')], {encoding: 'utf8'});
    assert.equal(original.status, 0, original.stderr);
    cases.push({id: 'qa-book', page: 8, expected: JSON.parse(original.stdout), paper});
  }
  for (const {id, page, expected, paper} of cases) {
    const url = `${base}/read/${id}?page=${page}&mode=bilingual`;
    await withBrowser(url, async ({evaluate, send, wait}) => {
      await evaluate('document.fonts.ready');
      const open = () => evaluate('document.querySelector(".contents-menu > button").click(); true');
      await open(); await wait('document.querySelector(".contents-popover").matches(":popover-open")');
      const actual = await evaluate(`[...document.querySelectorAll('.contents-link')].map(link => {
        let depth = -1; for (let node = link.parentElement; node; node = node.parentElement) if (node.tagName === 'OL') depth++;
        return {title: link.textContent, depth};})`);
      assert.deepEqual(actual, expected, `${id}: titles/order/depth must match source navigation`);
      assert.equal(await evaluate('document.querySelectorAll(".contents-pages,.contents-popover form,.contents-popover input").length'), 0);
      assert.equal(await evaluate('document.querySelector(".contents-popover").textContent.includes("Example 7-4")'), false);
      const scroll = await evaluate('scrollY');
      for (const [width, height] of [[1400, 900], [390, 500]]) {
        await send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: false});
        const rect = await evaluate(`(() => {const r = document.querySelector('.contents-popover').getBoundingClientRect();
          return {left: r.left, right: r.right, top: r.top, bottom: r.bottom};})()`);
        assert(rect.left >= -1 && rect.top >= -1 && rect.right <= width + 1 && rect.bottom <= height + 1, JSON.stringify(rect));
      }
      await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
      assert.equal(await evaluate('scrollY'), scroll, 'opening/scanning the directory must not move the reading viewport');
      if (id === 'qa-book') {
        const shot = await send('Page.captureScreenshot', {format: 'png'});
        writeFileSync('/tmp/vellio-web-book-contents.png', Buffer.from(shot.result.data, 'base64'));
      }
      if (paper) {
        const local = paper.outline.find(entry => entry.depth >= 1 && paper.blocks.find(block => block.id === entry.block)?.chapter === page);
        const block = local.block;
        await evaluate(`document.querySelector('[data-toc-block="${block}"]').click(); true`);
        await wait(`location.hash === '#block-${block}' && !document.querySelector('.contents-popover').matches(':popover-open')`);
        await open();
        await wait(`document.querySelector('.contents-link[aria-current="location"]')?.dataset.tocBlock === '${block}'`);
        if (id !== 'qa-book') {
          const next = paper.outline.find(entry => paper.blocks.find(block => block.id === entry.block)?.chapter === 2 && entry.depth === 2);
          await evaluate(`document.querySelector('[data-toc-block="${next.block}"]').click(); true`);
          await wait(`document.querySelector('.reader-body')?.dataset.page === '2' && location.hash === '#block-${next.block}'`);
          await evaluate('history.back(); true');
          await wait(`document.querySelector('.reader-body')?.dataset.page === '1' && location.hash === '#block-${block}'`);
        }
      } else {
        await evaluate('document.querySelector(".contents-link[data-toc-page=\\"2\\"]").click(); true');
        await wait('document.querySelector(".reader-body")?.dataset.page === "2"');
      }
      // Directory links remain functional without any reader JavaScript.
      await send('Emulation.setScriptExecutionDisabled', {value: true});
      await send('Page.navigate', {url});
      await wait('document.readyState === "complete" && document.querySelector(".contents-link") && !document.querySelector(".paper").dataset.ready');
      assert.equal(await evaluate('document.querySelectorAll(".contents-link").length'), expected.length);
      await open();
      const next = await evaluate(`(() => {const link = document.querySelector('.contents-link[data-toc-page="2"]') || document.querySelector('.contents-link');
        const href = link.href; link.click(); return href;})()`);
      assert.equal(new URL(next).searchParams.get('mode'), 'bilingual');
      await wait(`location.href === ${JSON.stringify(next)} && document.readyState === 'complete'`);
      await send('Emulation.setScriptExecutionDisabled', {value: false});
    });
  }
  await withBrowser(`${base}/read/no-toc`, async ({evaluate}) => {
    assert.equal(await evaluate('document.querySelector(".contents-empty").textContent'), '原书未提供可用目录。');
    assert.equal(await evaluate('document.querySelectorAll(".contents-link,.contents-pages,.contents-popover form").length'), 0);
  });
  console.log('PASS: EPUB3/NCX/PDF source directories preserve full titles, order and nested subsections; no page grid/jump form; same/cross-chapter links, Back, active location, narrow layout and no-JS navigation work.');
} finally {
  server.kill(); await delay(250);
  rmSync(temp, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
