// Isolated, deterministic PDF footnotes + numbered equation geometry regression.
// cargo build --manifest-path web-topcoat/Cargo.toml && node web-topcoat/pdf-layout-regression.mjs
// Optional: EASYREAD_QA_BOOK=/path/to/imported/attention/book (read-only copy).
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {readFileSync,writeFileSync,mkdtempSync,rmSync,cpSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import assert from 'node:assert/strict';
import {withBrowser} from './browser-smoke.mjs';

// Two pages: a body fragment followed by a small, separator-backed footnote,
// then the body's continuation. No external PDF fixture or model is required.
function pdfFixture() {
  const stream = text => `<< /Length ${Buffer.byteLength(text)} >>\nstream\n${text}\nendstream`;
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>',
    stream('BT /F1 12 Tf 72 500 Td (A paragraph about attention begins here and continues) Tj 0 -16 Td (on the next page without a footnote interrupting it.) Tj ET\n0.4 w 72 113 m 242 113 l S\nBT /F1 10 Tf 72 100 Td (4A separate explanatory footnote with smaller text) Tj 0 -12 Td (and a wrapped continuation that is not body prose.) Tj ET\nBT /F1 12 Tf 303 36 Td (1) Tj ET'),
    stream('BT /F1 12 Tf 72 700 Td (This is the continuation on the second source page.) Tj ET\nBT /F1 12 Tf 303 36 Td (2) Tj ET'),
  ];
  let pdf = '%PDF-1.4\n';const offsets = [0];
  objects.forEach((object,i) => {offsets.push(Buffer.byteLength(pdf));pdf += `${i+1} 0 obj\n${object}\nendobj\n`;});
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  pdf += offsets.slice(1).map(offset => `${String(offset).padStart(10,'0')} 00000 n \n`).join('');
  return Buffer.from(pdf + `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
}
function textHash(text) {
  let hash = 0x811c9dc5;
  for (let i=0;i<text.length;i++) hash = Math.imul(hash ^ text.charCodeAt(i),0x01000193) >>> 0;
  return hash.toString(16).padStart(8,'0');
}
const portServer = createServer();await new Promise(done => portServer.listen(0,'127.0.0.1',done));
const port = portServer.address().port;await new Promise(done => portServer.close(done));
const library = mkdtempSync(join(tmpdir(),'vellio-pdf-layout-'));
const web = spawn(resolve('web-topcoat/target/debug/vellio-web-topcoat'),[], {
  env:{...process.env,VELLIO_WEB_LIBRARY:library,VELLIO_DESKTOP_LIBRARY:library,HOST:'127.0.0.1',PORT:String(port),EASYREAD_MODEL:''},stdio:'ignore',
});
const delay = ms => new Promise(done => setTimeout(done,ms));
try {
  const base = `http://127.0.0.1:${port}`;
  for (let i=0;i<100;i++) {try {if ((await fetch(base)).ok) break;} catch {} await delay(100);}
  const file = new FormData();file.append('book',new Blob([pdfFixture()],{type:'application/pdf'}),'footnotes.pdf');
  const imported = await fetch(`${base}/import`,{method:'POST',body:file,redirect:'manual'});
  assert.equal(imported.status,303);
  const url = `${base}${imported.headers.get('location')}`, id = imported.headers.get('location').split('/').at(-1);
  const paperPath = join(library,id,'paper.json');
  await withBrowser(`${url}?mode=original`,async ({evaluate,send,wait}) => {
    const source = await evaluate(`(()=>{const block=document.querySelector('.blk[data-page-block="true"]');
      const host=block.querySelector(':scope > .source').cloneNode(true);
      const details=host.querySelector('.pdf-footnotes');
      if(details){details.querySelector('summary').remove();details.replaceWith(...details.childNodes);}
      host.querySelectorAll('[data-read-paragraph]').forEach(node=>node.removeAttribute('data-read-paragraph'));
      host.querySelectorAll('[data-edit-paragraph]').forEach(node=>node.removeAttribute('data-edit-paragraph'));
      return {id:block.dataset.id,html:host.innerHTML};})()`);
    assert.match(source.html,/data-vellio-footnote="true"/);
    const paper = JSON.parse(readFileSync(paperPath,'utf8'));
    const block = paper.blocks.find(block => block.id === source.id);
    // A legacy saved translation has the correct skeleton but no note metadata.
    block.zh = '正文译文。\n\n4脚注译文。';
    block.native_translation_layout = {version:1,source_hash:textHash(block.en),translation_hash:textHash(block.zh),
      html:source.html.replace(/ data-vellio-footnote="true"/g,'').replace('A paragraph about attention','正文译文 A paragraph about attention').replace('4A separate explanatory','4脚注译文 A separate explanatory')};
    const tex = String.raw`\operatorname{Attention}(Q,K,V)=\operatorname{softmax}\left(\frac{QK^T}{\sqrt{d_k}}\right)V`;
    // Exercise an old, unmarked native formula row through the source HTML
    // adapter; the PDF recovery emits the same span + literal-label structure.
    paper.blocks.push({id:'numbered-equation',type:'para',source_kind:'epub-xhtml',page:1,en:`$$${tex}$$ (1)`,zh:'',
      html:`<p><span data-vellio-math="${tex}" data-display="true">$$${tex}$$</span> &#8195;(1)</p>`});
    // A page-level list uses the same per-paragraph hover rail as recovered
    // PDF bullets. Include both bare li and p-in-li/nested translated items.
    const listZh = '列表译文';
    paper.blocks.push({id:'bullet-list',type:'list',source_kind:'pdf-text',page:1,en:'Synthetic bullet list',zh:listZh,
      items:[{en:'The encoder contains self-attention layers. '.repeat(5),zh:'编码器含有自注意力层。'},
        {en:'Each position can attend to all positions in the previous layer.',zh:'每个位置都能关注前一层的所有位置。'}],
      native_translation_layout:{version:1,source_hash:textHash('Synthetic bullet list'),translation_hash:textHash(listZh),
        html:'<ul><li><p>编码器含有自注意力层。</p><ul><li>嵌套列表项。</li></ul></li><li>每个位置都能关注前一层的所有位置。</li><li>额外说明。</li></ul>'}});
    writeFileSync(paperPath,JSON.stringify(paper));
    const persisted = readFileSync(paperPath,'utf8');
    for (const mode of ['bilingual','original','translation']) {
      await send('Page.navigate',{url:`${url}?page=1&mode=${mode}`});
      await wait(`document.querySelector('.paper.mode-${mode}')?.dataset.ready==='true'`);
      await wait(`document.querySelector('#block-numbered-equation .math-object') !== null`);
      const geometry = await evaluate(`(()=>{
        const row=[...document.querySelectorAll('#block-numbered-equation p')].find(node=>node.getClientRects().length);
        const math=row.querySelector('.math-object'),r=math.getBoundingClientRect();
        const walker=document.createTreeWalker(row,NodeFilter.SHOW_TEXT);let label;
        while(walker.nextNode())if(!walker.currentNode.parentElement.closest('[data-vellio-math]') && walker.currentNode.textContent.includes('(1)'))label=walker.currentNode;
        const range=document.createRange();range.selectNodeContents(label);const n=range.getBoundingClientRect();
        return {row:row.dataset.vellioEquation,math:r.toJSON(),number:n.toJSON(),font:parseFloat(getComputedStyle(row).fontSize),display:getComputedStyle(row).display};})()`);
      assert.equal(geometry.row,'true');assert.equal(geometry.display,'grid');
      assert(Math.abs((geometry.math.top+geometry.math.bottom)/2-(geometry.number.top+geometry.number.bottom)/2) < geometry.font*.35,JSON.stringify(geometry));
      assert(geometry.number.left >= geometry.math.right,JSON.stringify(geometry));
      const scope = mode === 'bilingual' ? '> .paired-flow' : mode === 'original' ? '> .source' : '> .translated';
      const host = `#block-${source.id} ${scope}`;
      assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)})?.open`),false,
        await evaluate(`document.querySelector('#block-${source.id}').outerHTML`));
      assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)})?.textContent.includes('4')`),true);
      if (mode === 'bilingual') {
        assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)})?.textContent.includes('脚注译文')`),true);
        assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} > .paragraph-pair .source`)})?.textContent.includes('explanatory footnote')`),false);
      }
      await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)}).open=true;true`);
      await wait(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes [data-read-paragraph]`)}).getBoundingClientRect().height>0`);
      if (mode === 'translation') {
        // A save patches this block in place. Keep the disclosure open, retain
        // its paragraph/edit IDs, and never persist the UI label as context.
        await evaluate(`(()=>{const node=document.querySelector(${JSON.stringify(`${host} .pdf-footnotes [data-read-paragraph]`)});
          node.scrollIntoView({block:'center'});const range=document.createRange();range.selectNodeContents(node);
          const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
          document.dispatchEvent(new MouseEvent('mouseup',{bubbles:true}));return true;})()`);
        await wait('document.querySelector("#floating").classList.contains("open")');
        await evaluate('document.querySelector("#floating [data-color=yellow]").click();true');
        await wait('document.querySelectorAll(".note-card").length === 1 && document.querySelector(".reader-body").dataset.updating === "false"');
        assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)})?.open`),true);
        assert.equal(await evaluate(`!!document.querySelector(${JSON.stringify(`${host} .pdf-footnotes [data-edit-paragraph="1"] [data-easyread-note]`)})`),true);
        const reader = JSON.parse(readFileSync(join(library,id,'reader.json'),'utf8'));
        const note = Object.values(reader.notes).find(note => !note.deleted);
        assert.equal(note.anchor,source.id);assert.equal(note.side,'zh');
        assert.doesNotMatch(note.prefix + note.suffix,/本页脚注/);
      }
      // Opening/reading must never rewrite the source, translation or stable IDs.
      assert.equal(readFileSync(paperPath,'utf8'),persisted);
      if (mode === 'bilingual') {
        await evaluate(`document.querySelector('[data-reading-order="translation"]').click();true`);
        assert.equal(await evaluate(`document.querySelector(${JSON.stringify(`${host} .pdf-footnotes`)})?.open`),true);
      }
      for (const width of [530,1400]) {
        await send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});
        await delay(100);
        assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'),true,`${mode} equation overflows at ${width}px`);
      }
    }
    // Hover actual list paragraphs at every supported size, and reserve a full
    // em for the native marker plus an extra 8px gap to the indicator's edge.
    // The old fixed -17px rail fails this bound (and intersects the bullet).
    for (const mode of ['original','translation','bilingual']) {
      await send('Page.navigate',{url:`${url}?page=1&mode=${mode}`});
      await wait(`document.querySelector('.paper.mode-${mode}')?.dataset.ready==='true'`);
      for (const width of [530,1400]) {
        await send('Emulation.setDeviceMetricsOverride',{width,height:900,deviceScaleFactor:1,mobile:false});
        await delay(100);
        for (const font of [14,21,32]) {
          await evaluate(`document.querySelector('.paper').style.setProperty('--font','${font}px');
            new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(()=>done(true))))`);
          const units = await evaluate(`(()=>{const units=[...document.querySelectorAll('#block-bullet-list ul .paragraph-unit')]
            .filter(node=>node.getBoundingClientRect().height>0);
            return units.map(node=>{node.dataset.bulletQa=String(units.indexOf(node));return node.dataset.bulletQa;});})()`);
          assert(units.length>0,`Missing visible list in ${mode}`);
          for (const unit of units) {
            const selector = `#block-bullet-list [data-bullet-qa="${unit}"]`;
            const point = await evaluate(`(()=>{const node=document.querySelector(${JSON.stringify(selector)});
              node.scrollIntoView({block:'center',behavior:'instant'});const r=node.getBoundingClientRect();return {x:r.left+5,y:r.top+5};})()`);
            await send('Input.dispatchMouseEvent',{type:'mouseMoved',...point});
            const geometry = await evaluate(`(()=>{const node=document.querySelector(${JSON.stringify(selector)}),style=getComputedStyle(node,'::before');
              return {content:style.content,left:parseFloat(style.left),width:parseFloat(style.width),font:parseFloat(getComputedStyle(node).fontSize)};})()`);
            assert.equal(geometry.content,'\"\"',JSON.stringify({mode,width,font,unit,geometry}));
            assert(geometry.left+geometry.width <= -geometry.font-8,JSON.stringify({mode,width,font,unit,geometry}));
          }
          assert.equal(await evaluate('document.documentElement.scrollWidth <= innerWidth'),true,
            `${mode} list overflows at ${width}px/${font}px: ` + JSON.stringify(await evaluate(`({width:innerWidth,scroll:document.documentElement.scrollWidth,
              overflow:[...document.querySelectorAll('body *')].filter(node=>node.getBoundingClientRect().right>innerWidth+1).map(node=>({tag:node.tagName,cls:node.className,right:node.getBoundingClientRect().right})).slice(0,10)})`)));
        }
      }
    }
    writeFileSync('/tmp/vellio-topcoat-list-hover.png',Buffer.from((await send('Page.captureScreenshot',{format:'png'})).result.data,'base64'));
    await send('Page.navigate',{url:`${url}?page=2&mode=original`});
    await wait('document.querySelector(".reader-body")?.dataset.page==="2" && document.querySelector(".paper")?.dataset.ready==="true"');
    assert.match(await evaluate('document.querySelector(".paper").textContent'),/continuation on the second source page/);
    assert.equal(await evaluate('document.querySelectorAll(".pdf-footnotes").length'),0);
  });
  if (process.env.EASYREAD_QA_BOOK) {
    cpSync(resolve(process.env.EASYREAD_QA_BOOK),join(library,'attention_qa'),{recursive:true});
    await withBrowser(`${base}/read/attention_qa?page=4&mode=bilingual`,async ({evaluate,send,wait}) => {
      await wait('document.querySelector(".paired-flow .math-object[data-display=true]") !== null');
      assert.equal(await evaluate('document.querySelector(".paired-flow .pdf-footnotes")?.open'),false);
      assert.match(await evaluate('document.querySelector(".paired-flow .pdf-footnotes").textContent'),/To illustrate why the dot products/);
      const row = await evaluate(`(()=>{const row=document.querySelector('.paired-flow p[data-vellio-equation=true]');
        row.scrollIntoView({block:'center',behavior:'instant'});const math=row.querySelector('.math-object').getBoundingClientRect();
        const range=document.createRange();range.selectNodeContents(row.lastChild);const number=range.getBoundingClientRect();
        return {difference:Math.abs((math.top+math.bottom-number.top-number.bottom)/2),font:parseFloat(getComputedStyle(row).fontSize)};})()`);
      assert(row.difference<row.font*.35,JSON.stringify(row));
      await evaluate('new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(()=>done(true))))');
      writeFileSync('/tmp/vellio-topcoat-numbered-equation.png',Buffer.from((await send('Page.captureScreenshot',{format:'png'})).result.data,'base64'));
      await evaluate('document.querySelector(".paired-flow .pdf-footnotes").open=true;true');
      await wait('[...document.querySelectorAll(".paired-flow .pdf-footnotes [data-vellio-math]")].every(node=>node.classList.contains("math-object"))');
      await evaluate('document.querySelector(".paired-flow .pdf-footnotes").scrollIntoView({block:"center",behavior:"instant"});new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(()=>done(true))))');
      assert.match(await evaluate('document.querySelector(".paired-flow .pdf-footnotes .translated").textContent'),/为了说明/);
      writeFileSync('/tmp/vellio-topcoat-pdf-footnote.png',Buffer.from((await send('Page.captureScreenshot',{format:'png'})).result.data,'base64'));
      await send('Page.navigate',{url:`${base}/read/attention_qa?page=5&mode=original`});
      await wait('document.querySelector(".paper.mode-original")?.dataset.ready==="true" && document.querySelector(".blk > .source ul .paragraph-unit") !== null');
      const point = await evaluate(`(()=>{const node=document.querySelector('.blk > .source ul .paragraph-unit');
        node.scrollIntoView({block:'center',behavior:'instant'});const r=node.getBoundingClientRect();return {x:r.left+5,y:r.top+5};})()`);
      await send('Input.dispatchMouseEvent',{type:'mouseMoved',...point});
      const gap = await evaluate(`(()=>{const node=document.querySelector('.blk > .source ul .paragraph-unit'),style=getComputedStyle(node,'::before');
        return -parseFloat(style.left)-parseFloat(style.width)-parseFloat(getComputedStyle(node).fontSize);})()`);
      assert(gap>=8,`Attention bullet gutter: ${gap}px`);
      writeFileSync('/tmp/vellio-topcoat-attention-list-hover.png',Buffer.from((await send('Page.captureScreenshot',{format:'png'})).result.data,'base64'));
    });
  }
  console.log('PDF layout: numbered equation centered beside label; legacy/source/bilingual footnotes folded; list hover rails clear bullets at 14/21/32px; page continuation and narrow layouts OK');
} finally {
  web.kill();await delay(250);rmSync(library,{recursive:true,force:true,maxRetries:8,retryDelay:100});
}
