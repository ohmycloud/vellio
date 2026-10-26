// cargo build --locked --manifest-path web-topcoat/Cargo.toml && node web-topcoat/note-code-regression.mjs
// Isolated browser regression: highlighted note code keeps source lines, hides
// scrollbars, and scrolls horizontally in both margin and drawer cards.
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {readFileSync, writeFileSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

const listener = createServer();
await new Promise(done => listener.listen(0, '127.0.0.1', done));
const port = listener.address().port;
await new Promise(done => listener.close(done));
const library = mkdtempSync(join(tmpdir(), 'vellio-note-code-'));
const server = spawn(resolve('web-topcoat/target/debug/vellio-web-topcoat'), [], {
  env: {...process.env, VELLIO_WEB_LIBRARY: library, VELLIO_DESKTOP_LIBRARY: library,
    HOST: '127.0.0.1', PORT: String(port), EASYREAD_MODEL: ''}, stdio: 'ignore',
});
const delay = ms => new Promise(done => setTimeout(done, ms));
try {
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(base)).ok) break; } catch {}
    await delay(100);
  }
  const upload = new FormData();
  upload.append('book', new Blob([readFileSync('fixtures/pdf-fragmented-caption.pdf')],
    {type: 'application/pdf'}), 'note-code.pdf');
  const imported = await fetch(`${base}/import`, {method: 'POST', body: upload, redirect: 'manual'});
  assert.equal(imported.status, 303);
  const location = imported.headers.get('location');
  const id = location.split('/').at(-1);
  const paper = JSON.parse(readFileSync(join(library, id, 'paper.json'), 'utf8'));
  const readerPath = join(library, id, 'reader.json');
  const reader = JSON.parse(readFileSync(readerPath, 'utf8'));
  const code = `print("${'long 中文 <&> code '.repeat(80)}")\n`;
  reader.notes['code-note'] = {id: 'code-note', anchor: paper.blocks[0].id,
    key: paper.blocks[0].id, kind: 'note', color: 'yellow', quote: '',
    body: `代码示例：\n\n\`\`\`python\n${code}\`\`\`\n\n正文正常换行。`,
    created: '2020-01-01T00:00:00Z', updated: '2020-01-01T00:00:00Z'};
  writeFileSync(readerPath, JSON.stringify(reader));
  const persisted = readFileSync(readerPath, 'utf8');
  await withBrowser(`${base}${location}?mode=original`, async ({evaluate, wait, send}) => {
    for (const drawer of [false, true]) {
      if (drawer) await evaluate('document.getElementById("notes-toggle").click()');
      const selector = `${drawer ? '.drawer-note' : '.margin .note-card'}[data-note="code-note"] .body pre`;
      await wait(`document.querySelector(${JSON.stringify(selector)})?.getClientRects().length > 0`);
      const geometry = await evaluate(`(() => {
        const pre = document.querySelector(${JSON.stringify(selector)}), style = getComputedStyle(pre);
        const before = pre.getBoundingClientRect();
        pre.scrollLeft = 120;
        const colors = new Set([...pre.querySelectorAll('span')].map(node => getComputedStyle(node).color));
        const after = pre.getBoundingClientRect();
        return {text: pre.textContent, whiteSpace: style.whiteSpace, wrap: style.overflowWrap,
          scrollbar: style.scrollbarWidth, webkitScrollbar: getComputedStyle(pre, '::-webkit-scrollbar').display,
          width: pre.clientWidth, contentWidth: pre.scrollWidth, left: pre.scrollLeft,
          height: before.height, lineHeight: getComputedStyle(pre.querySelector('code')).lineHeight,
          stable: before.width === after.width && before.height === after.height, colors: colors.size};
      })()`);
      assert.equal(geometry.text, code);
      assert.equal(geometry.whiteSpace, 'pre');
      assert.equal(geometry.wrap, 'normal');
      assert.equal(geometry.scrollbar, 'none');
      assert.equal(geometry.webkitScrollbar, 'none');
      assert(geometry.contentWidth > geometry.width * 2);
      assert.equal(geometry.left, 120);
      assert(geometry.stable);
      assert(geometry.colors >= 2);
      // The one source line (plus its trailing newline) must not grow into a
      // paragraph, regardless of viewport width or the highlighted spans.
      assert(geometry.height < parseFloat(geometry.lineHeight) * 3);
      await send('Input.dispatchMouseEvent', {type: 'mouseMoved', x: 5, y: 5});
    }
  });
  assert.equal(readFileSync(readerPath, 'utf8'), persisted, 'scrolling must not rewrite the note');
  console.log('PASS: margin and drawer note code retain syntax colors, do not wrap, and scroll with invisible scrollbars.');
} finally {
  server.kill();
  await delay(250);
  rmSync(library, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
