// Topcoat 0.11: native popovers, keyed element IDs, scoped Fluent, live drafts.
// cargo build --locked --manifest-path web-topcoat/Cargo.toml && node web-topcoat/controls-regression.mjs
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {readFileSync, writeFileSync, mkdtempSync, mkdirSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

const delay = ms => new Promise(done => setTimeout(done, ms));
const listener = createServer();
await new Promise(done => listener.listen(0, '127.0.0.1', done));
const port = listener.address().port;
await new Promise(done => listener.close(done));
const library = mkdtempSync(join(tmpdir(), 'vellio-controls-'));
const id = 'controls_fixture', book = join(library, id);
mkdirSync(book);
const fixture = JSON.parse(readFileSync('fixtures/state.json', 'utf8'));
fixture.paper.meta.page_count = 2;
fixture.paper.blocks.push({id: 'page-two', type: 'heading', page: 2, en: 'Second chapter', zh: '第二章'});
for (const part of ['paper', 'reader']) writeFileSync(join(book, `${part}.json`), JSON.stringify(fixture[part]));
const settings = {models: ['first', 'second'].map(id => ({id, name: id, provider: 'api', model: 'test',
  base_url: 'https://example.invalid/v1', key_env: 'VELLIO_CONTROLS_UNUSED_KEY'})), translation_model: 'first'};
const settingsPath = join(library, '.settings.json');
writeFileSync(settingsPath, JSON.stringify(settings));
const server = spawn(resolve('web-topcoat/target/debug/vellio-web-topcoat'), [], {
  env: {...process.env, VELLIO_WEB_LIBRARY: library, VELLIO_DESKTOP_LIBRARY: library,
    HOST: '127.0.0.1', PORT: String(port), EASYREAD_MODEL: ''}, stdio: 'ignore',
});
try {
  const base = `http://127.0.0.1:${port}`, url = `${base}/read/${id}`;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(base)).ok) break; } catch {}
    await delay(100);
  }
  for (const [language, expected, heading] of [
    ['en-US,en;q=0.9', 'en', 'Reading appearance'],
    ['zh-CN,en;q=0.5', 'zh-CN', '阅读外观'],
    ['en;q=0,zh-CN;q=0.5', 'zh-CN', '阅读外观'],
    ['fr-FR', 'zh-CN', '阅读外观'],
    ['', 'zh-CN', '阅读外观'],
  ]) {
    const response = await fetch(url, {headers: {'Accept-Language': language}});
    assert.equal(response.status, 200);
    assert.match(response.headers.get('vary'), /Accept-Language/i);
    const html = await response.text();
    assert(html.includes(`class="appearance-menu" lang="${expected}" dir="ltr"`), language);
    assert(html.includes(heading));
    assert.match(html, /<html lang="zh-CN"/); // Do not mislabel the untranslated reader.
  }
  const note = await fetch(`${url}/notes`, {method: 'POST', redirect: 'manual', body: new URLSearchParams({
    anchor: 's1-p2', side: 'en', quote: 'Native Markdown', prefix: '', suffix: '', kind: 'note',
    body: 'Saved note', color: 'yellow', style: 'highlight', page: '1', mode: 'bilingual',
  })});
  assert.equal(note.status, 303);
  await withBrowser(url, async ({evaluate, send, wait}) => {
    const click = async selector => {
      const point = await evaluate(`(() => { const el = document.querySelector(${JSON.stringify(selector)});
        const r = el.getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
      await send('Input.dispatchMouseEvent', {type: 'mousePressed', ...point, button: 'left', clickCount: 1});
      await send('Input.dispatchMouseEvent', {type: 'mouseReleased', ...point, button: 'left', clickCount: 1});
    };
    const escape = async () => {
      await send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27});
      await send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27});
    };
    const ids = await evaluate('[...document.querySelectorAll(".vellio-popover")].map(el => el.id)');
    assert.equal(new Set(ids).size, 3);
    assert(ids.every(Boolean));
    assert.equal(await evaluate(`[...document.querySelectorAll('[popovertarget]')].every(button =>
      document.getElementById(button.getAttribute('popovertarget'))?.hasAttribute('popover'))`), true);
    for (const [width, height] of [[1400, 900], [530, 500], [390, 360]]) {
      await send('Emulation.setDeviceMetricsOverride', {width, height, deviceScaleFactor: 1, mobile: false});
      for (const [trigger, panel] of [['.contents-menu > button', '.contents-popover'],
        ['.order-menu > button', '.order-options'], ['.appearance-menu > button', '.appearance-popover']]) {
        await click(trigger);
        await wait(`document.querySelector('${panel}').matches(':popover-open')`);
        const geometry = await evaluate(`(() => {const r = document.querySelector('${panel}').getBoundingClientRect();
          return {left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height};})()`);
        assert(geometry.width > 0 && geometry.height > 0);
        assert(geometry.left >= -1 && geometry.top >= -1 && geometry.right <= width + 1 && geometry.bottom <= height + 1,
          `${panel} outside ${width}x${height}: ${JSON.stringify(geometry)}`);
        await escape();
        await wait(`!document.querySelector('${panel}').matches(':popover-open')`);
        assert.equal(await evaluate(`document.activeElement === document.querySelector('${trigger}')`), true,
          'Escape restores focus to the invoker');
      }
    }
    await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
    await click('.contents-menu > button');
    await click('.appearance-menu > button');
    assert.equal(await evaluate('document.querySelectorAll(":popover-open").length'), 1, 'only one sibling menu stays open');
    await send('Input.dispatchMouseEvent', {type: 'mousePressed', x: 5, y: 100, button: 'left', clickCount: 1});
    await send('Input.dispatchMouseEvent', {type: 'mouseReleased', x: 5, y: 100, button: 'left', clickCount: 1});
    await wait('!document.querySelector(":popover-open")');
    // Parent overflow must not clip the browser top layer.
    await evaluate('document.querySelector(".top").style.overflow="hidden"');
    await click('.appearance-menu > button');
    assert.equal(await evaluate(`(() => {const r = document.querySelector('.appearance-popover').getBoundingClientRect();
      return document.elementFromPoint(r.left + 20, r.top + 20)?.closest('.appearance-popover') !== null;})()`), true);
    await escape();
    // The reader's global Escape handler must not close the editor beneath a menu.
    await evaluate('document.querySelector(".note-card [data-edit-note]").click()');
    await wait('!document.querySelector("#margin-editor").hidden');
    await evaluate(`window.draftForm = document.querySelector('#margin-editor form');
      window.draftForm.elements.body.value = 'Unsaved draft <&>';`);
    await click('.appearance-menu > button');
    await escape();
    assert.equal(await evaluate(`!document.querySelector('#margin-editor').hidden && window.draftForm.isConnected &&
      window.draftForm.elements.body.value === 'Unsaved draft <&>'`), true);
    await evaluate('document.querySelector("#margin-editor [data-margin-close]").click()');
    // Controls and IDs are stable on another server render; user Aa overrides survive it.
    await click('.appearance-menu > button');
    await evaluate(`const size = document.getElementById('reader-size'); size.value = '25'; size.dispatchEvent(new Event('input'));`);
    await send('Page.navigate', {url});
    await wait('document.querySelector(".paper")?.dataset.ready === "true" && document.getElementById("reader-size").value === "25"');
    assert.deepEqual(await evaluate('[...document.querySelectorAll(".vellio-popover")].map(el => el.id)'), ids);
    // Model menus use identity keys, not list indices. Reordering must not cross-wire targets.
    const modelIds = () => evaluate(`Object.fromEntries([...document.querySelectorAll('.model-card')].map(card =>
      [card.dataset.id, card.querySelector('[popover]').id]))`);
    await send('Page.navigate', {url: `${base}/settings`});
    await wait('document.querySelectorAll(".model-card").length === 2');
    const before = await modelIds();
    assert.notEqual(before.first, before.second);
    settings.models.reverse();
    writeFileSync(settingsPath, JSON.stringify(settings));
    await send('Page.navigate', {url: `${base}/settings?tab=model`});
    await wait('document.querySelector(".model-card")?.dataset.id === "second"');
    assert.deepEqual(await modelIds(), before);
    await click('.model-card[data-id="first"] .model-more > button');
    await click('.model-card[data-id="first"] [data-edit-model]');
    await wait('document.querySelector("#model-dialog").open');
    assert.equal(await evaluate('document.activeElement === document.querySelector("#model-editor [name=name]")'), true);
    assert.equal(await evaluate('document.querySelector(":popover-open")'), null);
    // Re-load with all page scripts disabled: directory popovers and links remain native.
    await send('Emulation.setScriptExecutionDisabled', {value: true});
    await send('Page.navigate', {url});
    await wait('document.readyState === "complete" && document.querySelector(".contents-menu > button") && document.querySelector(".paper") && !document.querySelector(".paper").dataset.ready');
    await click('.contents-menu > button');
    await wait('document.querySelector(".contents-popover")?.matches(":popover-open")');
    await escape();
    await wait('!document.querySelector(":popover-open")');
    await click('.contents-menu > button');
    await click('.contents-link[href*="page=2"]');
    await wait('document.querySelector(".reader-body")?.dataset.page === "2"');
    await send('Emulation.setScriptExecutionDisabled', {value: false});
  });
  console.log('PASS: Topcoat 0.11 popover geometry/dismissal/focus/no-JS, stable keyed IDs, Fluent negotiation and draft preservation.');
} finally {
  server.kill();
  await delay(250);
  rmSync(library, {recursive: true, force: true, maxRetries: 8, retryDelay: 100});
}
