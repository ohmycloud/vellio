// Single home import button -> native chooser -> automatic multipart upload.
// cargo build --locked --manifest-path web-topcoat/Cargo.toml && node web-topcoat/import-regression.mjs
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdtempSync, readdirSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

const delay = ms => new Promise(done => setTimeout(done, ms));
const listener = createServer();
await new Promise(done => listener.listen(0, '127.0.0.1', done));
const port = listener.address().port;
await new Promise(done => listener.close(done));
const library = mkdtempSync(join(tmpdir(), 'vellio-import-'));
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
  await withBrowser(base, async ({evaluate, send, wait, on}) => {
    await send('Page.enable');
    await send('Network.enable');
    await send('Page.setInterceptFileChooserDialog', {enabled: true});
    const choosers = [];
    let uploads = 0;
    on('Page.fileChooserOpened', event => choosers.push(event));
    on('Network.requestWillBeSent', ({request}) => {
      if (request.method === 'POST' && request.url === `${base}/import`) uploads++;
    });
    const chooser = async count => {
      for (let i = 0; i < 50; i++) {
        if (choosers.length >= count) return choosers[count - 1];
        await delay(80);
      }
      assert.fail('Clicking Import did not open the native file chooser');
    };
    const clickImport = async () => {
      const point = await evaluate(`(() => {const r = document.querySelector('[data-import-book]').getBoundingClientRect();
        return {x: r.left + r.width / 2, y: r.top + r.height / 2};})()`);
      await send('Input.dispatchMouseEvent', {type: 'mousePressed', ...point, button: 'left', clickCount: 1});
      await send('Input.dispatchMouseEvent', {type: 'mouseReleased', ...point, button: 'left', clickCount: 1});
    };
    assert.deepEqual(await evaluate(`(() => {const form = document.querySelector('.upload-compact');
      return {buttons: [...form.querySelectorAll('button')].map(button => button.textContent),
        labels: form.querySelectorAll('label').length, hidden: form.elements.book.hidden,
        accept: form.elements.book.accept};})()`), {
      buttons: ['导入'], labels: 0, hidden: true, accept: '.pdf,.epub,application/pdf,application/epub+zip',
    });
    await clickImport();
    const first = await chooser(1);
    assert.equal(first.mode, 'selectSingle');
    assert.equal(uploads, 0, 'opening the chooser must not submit an empty file');
    // Emulate cancelling: an empty file list must not trigger a POST/navigation.
    await send('DOM.setFileInputFiles', {files: [], backendNodeId: first.backendNodeId});
    await delay(200);
    assert.equal(uploads, 0);
    assert.equal(await evaluate('location.pathname'), '/');
    assert.equal(await evaluate('document.querySelector("[data-import-book]").disabled'), false);
    // The same native button must also work via keyboard activation.
    await evaluate('document.querySelector("[data-import-book]").focus()');
    await send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r'});
    await send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13});
    const second = await chooser(2);
    await send('DOM.setFileInputFiles', {
      files: [resolve('fixtures/pdf-fragmented-caption.pdf')], backendNodeId: second.backendNodeId,
    });
    await wait('location.pathname.startsWith("/read/") && document.querySelector(".paper")?.dataset.ready === "true"');
    assert.equal(uploads, 1, 'selecting a file automatically submits exactly once');
    assert.equal(await evaluate('document.querySelector(".reader-body").dataset.page'), '1');
    const bookId = await evaluate('document.querySelector(".reader-body").dataset.bookId');
    assert(readdirSync(library).includes(bookId), 'import must persist the book');
    // Returning to the cached home must restore the button and clear the old file.
    await evaluate('history.back(); true');
    await wait('location.pathname === "/" && document.querySelector("[data-import-book]")?.disabled === false');
    assert.equal(await evaluate('document.querySelector("[data-import-book]").textContent'), '导入');
    assert.equal(await evaluate('document.querySelector(".upload-compact").elements.book.files.length'), 0);
    await clickImport();
    const third = await chooser(3);
    await send('DOM.setFileInputFiles', {files: [], backendNodeId: third.backendNodeId});
    assert.equal(uploads, 1);
  }, {ready: 'document.querySelector("#home-count")?.textContent === "0 项"'});
  console.log('PASS: home has one Import button; mouse/keyboard open native chooser; cancel does not submit; selection imports once; Back restores the button.');
} finally {
  server.kill();
  await delay(250);
  rmSync(library, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
