(() => {
  const paper = document.querySelector('.paper');
  if (!paper) return;
  const bar = document.getElementById('floating');
  const dialog = document.getElementById('margin-editor');
  const form = document.getElementById('mark-form');
  let marginEditor = null;
  const reader = document.querySelector('.reader-body');
  const contentsPanel = document.querySelector('.contents-popover');
  contentsPanel?.addEventListener('click', event => {
    // Same-chapter fragment links do not reload the document. Close explicitly
    // without intercepting native link navigation, new tabs or Back/Forward.
    if (event.target.closest('.contents-link') && contentsPanel.matches(':popover-open')) contentsPanel.hidePopover();
  });
  contentsPanel?.addEventListener('toggle', event => {
    if (event.newState !== 'open') return;
    const links = [...contentsPanel.querySelectorAll('.contents-link')];
    const local = links.filter(link => link.dataset.tocPage === reader.dataset.page);
    let current = local[0];
    for (const link of local) {
      const target = link.dataset.tocBlock && document.getElementById(`block-${link.dataset.tocBlock}`);
      if (target && target.getBoundingClientRect().top <= 100) current = link;
    }
    // A short chapter may not scroll far enough to align a clicked heading
    // with the toolbar. Its explicit fragment still identifies the selection.
    let fragment = '';
    try {fragment = decodeURIComponent(location.hash.slice(1));} catch {}
    current = local.find(link => link.dataset.tocBlock && `block-${link.dataset.tocBlock}` === fragment) || current;
    for (const link of links) {
      if (link === current) link.setAttribute('aria-current', 'location');
      else link.removeAttribute('aria-current');
    }
    const scroller = contentsPanel.querySelector('.contents-sections');
    if (current && scroller) {
      const row = current.getBoundingClientRect(), box = scroller.getBoundingClientRect();
      scroller.scrollTop += row.top - box.top - (scroller.clientHeight - row.height) / 2;
    }
  });
  const orderKey = `vellio-web:reading-order:${reader.dataset.bookId}`;
  const orderMenu = document.querySelector('.order-menu');
  // A PDF page is one persisted block but may contain many logical paragraphs.
  // PairLayout supplied the role-checked companions; group its source fragments
  // without changing the book's block IDs or duplicating image/equation rows.
  function foldFootnotes(host, scope, previous) {
    const notes = [...host.children].filter(node => node.matches('[data-vellio-footnote="true"]') ||
      node.querySelector(':scope > .source[data-vellio-footnote="true"]'));
    if (!notes.length) return;
    const details = document.createElement('details');
    details.className = 'pdf-footnotes';details.dataset.footnoteScope = scope;
    details.open = previous?.querySelector(`.pdf-footnotes[data-footnote-scope="${scope}"]`)?.open || false;
    const summary = document.createElement('summary');summary.textContent = '本页脚注';
    details.append(summary, ...notes);host.append(details);
  }
  function prepareBlocks(root, previous = null) {
    // Old saved equation rows also get the same layout, without rewriting TeX,
    // labels, annotations or paragraph IDs. Inline math never enters this path.
    for (const row of root.querySelectorAll('.blk p')) {
      const math = [...row.querySelectorAll('[data-vellio-math]')];
      if (!math.length || math.some(node => node.dataset.display !== 'true')) continue;
      const outside = row.cloneNode(true);
      outside.querySelectorAll('[data-vellio-math]').forEach(node => node.remove());
      if (/^(?:[（(][\w.+*-]+[)）]|\[[\w.+*-]+\])?$/.test(outside.textContent.trim()))
        row.dataset.vellioEquation = 'true';
    }
    for (const block of root.querySelectorAll('.blk')) {
      // Whole-page fallback may contain the same native crop in both languages.
      // Hide only matching images in the secondary language; single-language
      // modes and the user's bilingual order still retain the complete figure.
      const sourceImages = [...block.querySelectorAll(':scope > .source img')];
      const translatedImages = [...block.querySelectorAll(':scope > .translated img')];
      const sourceUrls = new Set(sourceImages.map(image => image.getAttribute('src')).filter(Boolean));
      const translatedUrls = new Set(translatedImages.map(image => image.getAttribute('src')).filter(Boolean));
      for (const image of [...sourceImages, ...translatedImages]) {
        const shared = sourceUrls.has(image.getAttribute('src')) && translatedUrls.has(image.getAttribute('src'));
        image.classList.toggle('shared-illustration', shared);
        const row = image.closest('p');
        if (shared && row && !row.textContent.trim() && [...row.querySelectorAll('img')].every(other =>
          sourceUrls.has(other.getAttribute('src')) && translatedUrls.has(other.getAttribute('src'))))
          row.classList.add('shared-illustration-row');
      }
      if (block.dataset.pageBlock === 'true') for (const lang of block.querySelectorAll(':scope > .source, :scope > .translated')) {
        const selector = 'p,h1,h2,h3,h4,h5,h6,li,td,th,figcaption';
        const units = [...lang.querySelectorAll(selector)].filter(node => !node.querySelector(selector));
        for (const unit of units.length ? units : lang.children) unit.classList.add('paragraph-unit');
      }
      for (const side of ['source', 'translated']) {
        const host = block.querySelector(`:scope > .${side}`);
        if (host) foldFootnotes(host, side, previous);
      }
      const flow = block.querySelector(':scope > .paired-flow');
      if (!flow) continue;
      let primary = [];
      for (const child of [...flow.children]) {
        if (child.matches('blockquote[data-easyread-companion]')) {
          if (!primary.length) continue;
          const pair = document.createElement('div');
          pair.className = 'paragraph-pair';
          flow.insertBefore(pair, primary[0]);
          for (const node of primary) {
            node.classList.add('source');node.lang = 'en';pair.append(node);
          }
          child.classList.add('translated');child.lang = 'zh';pair.append(child);
          primary = [];
        } else if ((child.querySelector('img') && !child.textContent.trim()) ||
                   (child.querySelector('[data-display="true"]') && /^(?:\([\w.+-]+\))?$/.test(child.textContent.trim())) ||
                   (child.matches('p') && /^\d+$/.test(child.textContent.trim()))) {
          child.classList.add('source','paragraph-unit');child.lang = 'en';primary = [];
        } else {
          primary.push(child);
        }
      }
      for (const node of primary) {node.classList.add('source','paragraph-unit');node.lang = 'en';}
      if (previous?.dataset.localLanguages === 'true') block.dataset.localLanguages = 'true';
      const oldPairs = previous?.querySelectorAll('.paragraph-pair') || [];
      for (const [index,pair] of [...flow.querySelectorAll('.paragraph-pair')].entries()) {
        if (oldPairs[index]?.dataset.secondary) pair.dataset.secondary = oldPairs[index].dataset.secondary;
      }
      foldFootnotes(flow, 'paired', previous);
      const active = paper.classList.contains('mode-bilingual') || block.dataset.localLanguages === 'true';
      block.classList.toggle('pair-active', active);
      flow.hidden = !active;
    }
  }
  prepareBlocks(paper);
  function setReadingOrder(value) {
    const sourceFirst = value !== 'translation';
    paper.classList.toggle('original-first', sourceFirst);
    orderMenu.querySelectorAll('[data-reading-order]').forEach(button =>
      button.setAttribute('aria-pressed', String(button.dataset.readingOrder === (sourceFirst ? 'source' : 'translation'))));
    requestAnimationFrame(() => {placeNotes();refreshGeometry();});
  }
  try {setReadingOrder(localStorage.getItem(orderKey) || 'source');} catch {setReadingOrder('source');}
  orderMenu.querySelectorAll('[data-reading-order]').forEach(button => button.addEventListener('click', () => {
    const order = button.dataset.readingOrder;
    try {localStorage.setItem(orderKey, order);} catch {}
    if (!paper.classList.contains('mode-bilingual')) {
      const url = new URL(location.href);url.searchParams.set('mode','bilingual');location.assign(url);
    } else {setReadingOrder(order);} // The native hide action closes the popover.
  }));
  let pending = null;
  let markStyle = 'highlight', markColor = 'yellow';
  let draftRange = null, draftBlock = null;
  const rootFor = node => (node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement)?.closest('.source,.translated');
  let existingCard = null;
  let pointerStart = null, dragged = false;
  paper.addEventListener('mousedown', e => {pointerStart = {x:e.clientX,y:e.clientY};dragged = false;});
  const close = () => { bar.classList.remove('open'); pending = null; existingCard = null; };
  function openAnnotation(marker, rect) {
    const card = [...document.querySelectorAll('.note-card')].find(c => c.dataset.note === marker.dataset.easyreadNote);
    const root = rootFor(marker);
    if (!card || !root) return;
    // One saved quote can span emphasis, plain text and several formulas.
    // Select all its fragments, not just the clicked formula's hidden TeX.
    const markers = [...root.querySelectorAll('[data-easyread-note]')].filter(node =>
      node.dataset.easyreadNote === marker.dataset.easyreadNote &&
      node.parentElement.closest('[data-easyread-note]')?.dataset.easyreadNote !== marker.dataset.easyreadNote);
    if (!markers.length) return;
    const range = document.createRange();
    range.setStartBefore(markers[0]);range.setEndAfter(markers.at(-1));
    const selection = window.getSelection();
    selection.removeAllRanges();selection.addRange(range);
    const source = selected(4000);
    if (!source) {close();return;}
    open(source, rect || marker.getClientRects()[0] || marker.getBoundingClientRect(), card);
    const editor = card.querySelector('.note-edit form');
    markStyle = editor.elements.style.value;markColor = editor.elements.color.value;
    syncMarkToolbar();
  }
  const open = (source, rect, card = null) => {
    pending = source;
    existingCard = card;
    draftRange = window.getSelection()?.rangeCount && !window.getSelection().isCollapsed ? window.getSelection().getRangeAt(0).cloneRange() : null;
    draftBlock = [...paper.querySelectorAll('.blk')].find(b => b.dataset.id === source.anchor);
    syncContextToolbar();
    bar.classList.add('open');
    const width = bar.offsetWidth;
    bar.style.left = `${Math.max(8, Math.min(window.innerWidth - width - 8, rect.left))}px`;
    const height = bar.offsetHeight;
    bar.style.top = `${rect.top < 65 + height + 9 ? rect.bottom + 9 : rect.top - height - 9}px`;
  };
  function selected(maxLength = 1800) {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) return null;
    const range = sel.getRangeAt(0);
    const lang = rootFor(range.startContainer);
    if (!lang || !lang.contains(range.endContainer) || !paper.contains(lang)) return null;
    const block = lang.closest('.blk');
    if (!block) return null;
    const quote = sel.toString();
    if (!quote.trim() || quote.length > maxLength) return null;
    const before = range.cloneRange();
    before.selectNodeContents(lang);
    before.setEnd(range.startContainer, range.startOffset);
    // Disclosure labels are reader UI, not source text or annotation context.
    const contextText = node => {
      const copy = node.cloneNode(true);
      copy.querySelectorAll('.pdf-footnotes > summary').forEach(summary => summary.remove());
      return copy.textContent;
    };
    const text = contextText(lang);
    const offset = contextText(before.cloneContents()).length;
    return {anchor: block.dataset.id, side: lang.classList.contains('source') || block.classList.contains('untranslated') ? 'en' : 'zh',
      quote, prefix: text.slice(Math.max(0, offset - 32), offset),
      suffix: text.slice(offset + quote.length, offset + quote.length + 32)};
  }
  document.addEventListener('mouseup', e => {
    if (e.target instanceof Element && e.target.closest('#floating,#margin-editor')) return;
    dragged = !!pointerStart && Math.hypot(e.clientX-pointerStart.x,e.clientY-pointerStart.y) > 3;
    pointerStart = null;
    // A simple click on a saved mark is handled below, including formulas
    // whose user-select:all creates a native selection before click fires.
    if (!dragged && e.detail < 2 && e.target instanceof Element && e.target.closest('[data-easyread-note]')) return;
    setTimeout(() => {
      const state = selected();
      if (state) {
        const rect = window.getSelection().getRangeAt(0).getBoundingClientRect();
        open(state, rect);
      } else {
        close();
      }
    }, 0);
  });
  function primarySource() {
    return paper.classList.contains('mode-original') ||
      (paper.classList.contains('mode-bilingual') && paper.classList.contains('original-first'));
  }
  function readingContext() {
    if (!draftRange || !draftBlock?.isConnected) return null;
    const lang = rootFor(draftRange.startContainer);
    if (!lang) return null;
    const units = lang.matches('[data-read-paragraph]') ? [lang] : [...lang.querySelectorAll('[data-read-paragraph]')];
    const selected = units.filter(node => draftRange.intersectsNode(node));
    if (selected.length !== 1) return null; // Never guess a paragraph from a cross-paragraph selection.
    const paragraph = selected[0], source = lang.classList.contains('source');
    const flow = draftBlock.querySelector(':scope > .paired-flow');
    const match = [...(flow?.querySelectorAll('[data-read-paragraph]') || [])].find(node =>
      node.dataset.readParagraph === paragraph.dataset.readParagraph && rootFor(node)?.classList.contains(source ? 'source' : 'translated'));
    const pair = paragraph.closest('.paragraph-pair') || match?.closest('.paragraph-pair');
    const target = source ? pair?.querySelector('.translated[data-edit-paragraph],.translated [data-edit-paragraph]') : paragraph;
    const editor = target?.hasAttribute('data-edit-paragraph') && !draftBlock.classList.contains('untranslated') ?
      draftBlock.querySelector(`.translation-editor[data-paragraph="${target.dataset.editParagraph}"]`) : null;
    return {block:draftBlock,paragraph,pair,target,editor,source};
  }
  function secondaryVisible(pair) {
    return pair?.dataset.secondary === 'shown' || (pair?.dataset.secondary !== 'hidden' && paper.classList.contains('mode-bilingual'));
  }
  function syncContextToolbar() {
    const context = readingContext(), language = bar.querySelector('[data-action="original"]');
    const visible = !!context?.pair && secondaryVisible(context.pair);
    language.disabled = !context?.pair;
    language.setAttribute('aria-expanded', String(visible));
    const label = `${visible ? '收起' : '显示'}${primarySource() ? '译文' : '原文'}`;
    language.setAttribute('aria-label', context?.pair ? label : '无法可靠对应该段的另一种语言');
    language.title = language.getAttribute('aria-label');
    bar.querySelector('[data-action="edit"]').disabled = !context?.editor;
  }
  function activatePairs(block) {
    block.dataset.localLanguages = 'true';block.classList.add('pair-active');
    block.querySelector(':scope > .paired-flow').hidden = false;
    renderMath(block);
    requestAnimationFrame(() => {placeNotes();refreshGeometry();});
  }
  function closeTranslationEditor(editor) {
    if (!editor) return;
    editor.reset();editor.hidden = true;
    if (editor.editTarget) editor.editTarget.hidden = false;
    editor.classList.remove('translated');
    const block = editor.closest('.blk');
    block?.append(editor);
    if (editor.readingState && block) {
      const {local,active,pair,secondary} = editor.readingState;
      if (local) block.dataset.localLanguages = local; else delete block.dataset.localLanguages;
      block.classList.toggle('pair-active',active);
      const flow = block.querySelector(':scope > .paired-flow');
      if (flow) flow.hidden = !active;
      if (pair) {if (secondary) pair.dataset.secondary = secondary; else delete pair.dataset.secondary;}
    }
    editor.editTarget = null;editor.readingState = null;
    requestAnimationFrame(() => {placeNotes();refreshGeometry();});
  }
  function beginTranslationEdit() {
    const context = readingContext();
    if (!context?.editor) return;
    paper.querySelectorAll('.translation-editor:not([hidden])').forEach(closeTranslationEditor);
    const {block,pair,target,editor,source} = context;
    editor.readingState = {local:block.dataset.localLanguages,active:block.classList.contains('pair-active'),pair,secondary:pair?.dataset.secondary};
    window.getSelection()?.removeAllRanges();
    if (source) {
      if (primarySource()) pair.dataset.secondary = 'shown';
      activatePairs(block);
    }
    editor.editTarget = target;
    editor.classList.toggle('translated', target.classList.contains('translated'));
    target.before(editor);target.hidden = true;editor.hidden = false;
    close();
    editor.querySelector('textarea').focus({preventScroll:true});
    requestAnimationFrame(() => {placeNotes();refreshGeometry();});
  }
  paper.addEventListener('click', e => {
    if (e.target.closest('[data-translation-cancel]')) {
      closeTranslationEditor(e.target.closest('.translation-editor'));return;
    }
    if (e.target.closest('a,button,input,textarea')) return;
    const marker = e.target.closest('[data-easyread-note]');
    if (marker && !dragged && e.detail < 2) {
      // Anchor to the clicked line, not the first line of a long saved quote.
      const rect = [...marker.getClientRects()].find(r =>
        e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom);
      openAnnotation(marker, rect);
      return;
    }
    if (window.getSelection()?.toString()) return;
    // Unmarked paragraph clicks remain reading/navigation, not action requests.
    close();
  });
  function placeDraft() {marginEditor?.place();}
  window.addEventListener('scroll', placeDraft, {passive:true});
  window.addEventListener('resize', placeDraft);
  function syncMarkToolbar() {
    bar.dataset.markStyle = markStyle;
    bar.querySelectorAll('[data-style]').forEach(button => {
      const active = button.dataset.style === markStyle;
      button.classList.toggle('selected', active);
      button.setAttribute('aria-pressed', String(active));
    });
    const labels = {yellow:'黄色',green:'绿色',blue:'蓝色',pink:'红色'};
    const label = markStyle === 'underline' ? '下划线' : '高亮';
    bar.querySelectorAll('[data-color]').forEach(button => {
      button.classList.toggle('selected', button.dataset.color === markColor);
      button.setAttribute('aria-label', `${labels[button.dataset.color]}${label}`);
      button.title = `添加${labels[button.dataset.color]}${label}`;
    });
  }
  function fillNote(kind) {
    if (!pending) return false;
    for (const key of ['anchor','side','quote','prefix','suffix']) form.elements[key].value = pending[key];
    form.elements.kind.value = kind;
    form.elements.style.value = markStyle;
    form.elements.color.value = markColor;
    form.elements.body.value = '';
    return true;
  }
  function applyMark() {
    if (!pending?.quote) return;
    if (existingCard) {
      const editor = existingCard.querySelector('.note-edit form');
      for (const [key,value] of [['style',markStyle],['color',markColor]]) {
        editor.elements[key].value = value;
        editor.elements[key].dispatchEvent(new Event('change',{bubbles:true}));
      }
      editor.requestSubmit();
      return;
    }
    if (!fillNote('highlight')) return;
    // Plain marks save immediately. Writing uses the separate autosaving
    // margin form, so an in-flight mark cannot overwrite a note draft.
    form.requestSubmit();
  }
  syncMarkToolbar();
  bar.addEventListener('mousedown', e => e.preventDefault()); // Keep selection when clicking the toolbar.
  bar.addEventListener('click', e => {
    const style = e.target.closest('[data-style]');
    if (style) {
      markStyle = style.dataset.style;
      syncMarkToolbar();
      return;
    }
    const color = e.target.closest('[data-color]');
    if (color) {
      markColor = color.dataset.color;
      syncMarkToolbar();
      applyMark();
      return;
    }
    const button = e.target.closest('button[data-action]');
    if (!button || !pending) return;
    const action = button.dataset.action;
    if (action === 'ask' && chatPanel) {
      showPanel('chat');
      const chatForm = chatPanel.querySelector('form');
      chatForm.elements.anchor.value = pending.anchor;
      chatForm.elements.quote.value = pending.quote.slice(0, 1000);
      chatForm.elements.note_id.value = '';
      chatForm.elements.text.value = pending.quote ? `请解释这段内容：“${pending.quote.slice(0, 1000)}”` : '请解释当前段落。';
      chatForm.elements.text.focus({preventScroll:true});
      close();return;
    }
    if (action === 'copy') { navigator.clipboard?.writeText(pending.quote); close(); return; }
    if (action === 'close') { close(); return; }
    if (action === 'original') {
      const context = readingContext();
      if (context?.pair) {
        const visible = secondaryVisible(context.pair);
        window.getSelection()?.removeAllRanges();
        context.pair.dataset.secondary = visible ? 'hidden' : 'shown';
        activatePairs(context.block);
      }
      close();return;
    }
    if (action === 'edit') {beginTranslationEdit();return;}
    if (action === 'note' || action === 'question') {
      marginEditor.open(pending,{kind:action,card:existingCard,color:markColor,range:draftRange});
      close();window.getSelection()?.removeAllRanges();
    }
  });
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
      // Dismiss the top-layer control first. A single Escape must not also
      // close/submit a note draft or a translation editor underneath it.
      if (document.querySelector('.vellio-popover:popover-open')) return;
      close();marginEditor?.close();
      paper.querySelectorAll('.translation-editor:not([hidden])').forEach(closeTranslationEditor);
      window.getSelection()?.removeAllRanges();
    }
  });
  const sourcePanel = document.getElementById('source-panel');
  const notesPanel = document.getElementById('notes-drawer');
  const chatPanel = document.getElementById('chat-panel');
  const chatToggle = document.getElementById('chat-toggle');
  const sourceToggle = document.getElementById('source-toggle');
  const notesToggle = document.getElementById('notes-toggle');
  function showPanel(which) {
    if (which) marginEditor?.close();
    const source = which === 'source';
    const notes = which === 'notes';
    const chat = which === 'chat';
    if (sourcePanel) sourcePanel.hidden = !source;
    notesPanel.hidden = !notes;
    if (chatPanel) chatPanel.hidden = !chat;
    reader.classList.toggle('source-open', source);
    reader.classList.toggle('drawer-open', notes);
    reader.classList.toggle('chat-open', chat);
    sourceToggle?.setAttribute('aria-expanded', String(source));
    notesToggle.setAttribute('aria-expanded', String(notes));
    chatToggle?.setAttribute('aria-expanded', String(chat));
    if (chat) {const messages=chatPanel.querySelector('.chat-messages');messages.scrollTop=messages.scrollHeight;}
    requestAnimationFrame(placeNotes);
  }
  sourceToggle?.addEventListener('click', () => showPanel(sourcePanel.hidden ? 'source' : null));
  notesToggle.addEventListener('click', () => showPanel(notesPanel.hidden ? 'notes' : null));
  chatToggle?.addEventListener('click', () => showPanel(chatPanel.hidden ? 'chat' : null));
  if (chatPanel && location.hash === '#chat-panel') showPanel('chat');
  chatPanel?.querySelector('form')?.addEventListener('submit', event => {
    const button=event.target.querySelector('button[type=submit]');button.disabled=true;button.textContent='正在回答…';
  });
  function openCardEditor(card) {
    if (!card) return;
    if (card.dataset.page && card.dataset.page !== reader.dataset.page) {
      const url=new URL(location.href);url.searchParams.set('page',card.dataset.page);
      url.hash=`edit-note-${encodeURIComponent(card.dataset.note)}`;location.assign(url);return;
    }
    const data=card.querySelector('.note-edit form');
    marginEditor.open({anchor:card.dataset.anchor,side:card.dataset.side || 'zh',
      quote:card.querySelector(':scope > .quote')?.textContent || '',prefix:card.dataset.prefix || '',suffix:card.dataset.suffix || ''},
      {card,kind:data.elements.kind.value==='highlight'?'note':null,color:data.elements.color.value});
    close();window.getSelection()?.removeAllRanges();
  }
  const clipUtf8 = (text,limit) => {
    const encoder=new TextEncoder();let result='',size=0;
    for (const ch of text) {size+=encoder.encode(ch).length;if(size>limit)break;result+=ch;}
    return result;
  };
  reader.addEventListener('click', e => {
    if (e.target.closest('[data-edit-note]')) {openCardEditor(e.target.closest('.note-card,.drawer-note'));return;}
    const target=e.target.closest('[data-ask-note]');
    if (!target || !chatPanel) return;
    const note=target.closest('.note-card,.drawer-note');
    const form=chatPanel.querySelector('form');
    form.elements.anchor.value = note.dataset.anchor;
    form.elements.quote.value = clipUtf8(note.querySelector(':scope > .quote')?.textContent || '',1000);
    const data=note.querySelector('.note-edit form'),question=data.elements.kind.value==='question';
    const followUp=question&&data.elements.answered.value==='yes';
    form.elements.note_id.value = followUp?'':target.dataset.askNote;
    const body=clipUtf8(data.elements.body.value,3500);
    form.elements.text.value = followUp?'':question?body:
      `这是我读这里时写的笔记，请点评：我理解得对不对、有没有漏掉或想错的地方、还可以往哪想。\n\n我的笔记：${body}`;
    showPanel('chat');
    if (followUp) form.elements.text.focus({preventScroll:true});
    else form.requestSubmit();
  });
  reader.querySelectorAll('[data-close-panel]').forEach(button => button.addEventListener('click', () => showPanel(null)));
  const noteFilters = [...notesPanel.querySelectorAll('[data-note-filter]')];
  function applyNoteFilter() {
    const filter = noteFilters.find(button => button.classList.contains('active'))?.dataset.noteFilter || 'all';
    notesPanel.querySelectorAll('.drawer-note').forEach(note => {
      note.hidden = !(filter === 'all' || note.dataset.kind === filter ||
        (filter === 'unanswered' && note.dataset.kind === 'question' && note.dataset.answered === 'false'));
    });
  }
  noteFilters.forEach(button => button.addEventListener('click', () => {
    noteFilters.forEach(item => item.classList.toggle('active', item === button));
    applyNoteFilter();
  }));
  const controls = {
    font: document.getElementById('reader-font'), size: document.getElementById('reader-size'),
    width: document.getElementById('reader-width'), leading: document.getElementById('reader-leading'),
    margin: document.getElementById('reader-margin')
  };
  const key = `vellio-web:appearance:${reader.dataset.bookId}`;
  const defaults = {font: reader.dataset.defaultFont || 'noto', size: Number(reader.dataset.defaultSize) || 21,
    width: Number(reader.dataset.defaultWidth) || 35, leading: Number(reader.dataset.defaultLeading) || 19,
    margin: reader.dataset.defaultMargin !== 'false'};
  let appearance = {...defaults};
  try {appearance = {...defaults, ...JSON.parse(localStorage.getItem(key) || '{}')};} catch {}
  function applyAppearance() {
    for (const field of ['font','size','width','leading']) {
      if (field === 'font') controls.font.value = appearance.font;
      else controls[field].value = appearance[field];
    }
    controls.margin.checked = appearance.margin;
    appearance.font = controls.font.value;
    for (const field of ['size','width','leading']) appearance[field] = Number(controls[field].value);
    const families = {noto: "VellioSerif,'Noto Serif SC','Songti SC',serif", system: "'Songti SC',Georgia,serif", sans: "system-ui,'PingFang SC',sans-serif"};
    document.documentElement.style.setProperty('--reader-family', families[appearance.font]);
    document.documentElement.style.setProperty('--font', `${appearance.size}px`);
    document.documentElement.style.setProperty('--measure', `${appearance.size * appearance.width}px`);
    document.documentElement.style.setProperty('--leading', String(appearance.leading / 10));
    controls.size.nextElementSibling.value = String(appearance.size);
    controls.width.nextElementSibling.value = String(appearance.width);
    controls.leading.nextElementSibling.value = String(appearance.leading / 10);
    reader.classList.toggle('margin-collapsed', !appearance.margin);
    requestAnimationFrame(() => {placeNotes();refreshGeometry();});
  }
  applyAppearance();
  for (const field of ['font','size','width','leading','margin']) {
    controls[field].addEventListener(field === 'margin' || field === 'font' ? 'change' : 'input', () => {
      appearance[field] = field === 'margin' ? controls[field].checked : controls[field].value;
      applyAppearance();
      try {localStorage.setItem(key, JSON.stringify(appearance));} catch {}
    });
  }
  function updateProgress() {
    const bar = document.querySelector('.reading-progress');
    const span = Math.max(1, document.documentElement.scrollHeight - innerHeight);
    const value = Math.min(100, Math.round(100 * ((Number(reader.dataset.page) - 1 + scrollY / span) / Number(reader.dataset.total))));
    bar.firstElementChild.style.width = `${value}%`;
    bar.setAttribute('aria-valuenow', String(value));
  }
  window.addEventListener('scroll', updateProgress, {passive: true});
  window.addEventListener('resize', updateProgress);
  window.addEventListener('pagehide', () => {
    const blocks = [...paper.querySelectorAll('.blk')];
    const block = blocks.filter(b => b.getBoundingClientRect().top < 130).at(-1) || blocks[0];
    if (!block) return;
    const span = Math.max(1, document.documentElement.scrollHeight - innerHeight);
    const ratio = Math.min(1, Math.max(0, (Number(reader.dataset.page) - 1 + scrollY / span) / Number(reader.dataset.total)));
    navigator.sendBeacon(`/read/${encodeURIComponent(reader.dataset.bookId)}/progress`,
      new URLSearchParams({block:block.dataset.id,ratio:String(ratio)}));
  });
  updateProgress();
  const jobStatus = document.getElementById('job-status');
  if (jobStatus?.dataset.active === 'true' || ['running','starting'].includes(jobStatus?.dataset.state)) {
    const timer = setInterval(async () => {
      try {
        const response = await fetch(`/read/${encodeURIComponent(reader.dataset.bookId)}/job`, {cache:'no-store'});
        if (!response.ok) return;
        const job = await response.json();
        if (job.active) {
          jobStatus.textContent = job.state === 'starting' ? '准备翻译…' : `翻译 ${job.done ?? 0}/${job.total ?? '?'}`;
        } else if (['done','partial','cancelled'].includes(job.state)) {
          clearInterval(timer);
          if (job.state === 'done') location.reload();
          else jobStatus.textContent = job.state === 'cancelled' ? '翻译已取消' : '翻译中断，可刷新后重试';
        }
      } catch {jobStatus.textContent = '翻译状态暂时不可用';}
    }, 1400);
    window.addEventListener('pagehide', () => clearInterval(timer));
  }
  function placeNotes() {
    marginEditor?.place();
    const margin = document.querySelector('.margin');
    if (!margin) return;
    margin.querySelectorAll('.note-card').forEach(card => {card.hidden = card.dataset.note === marginEditor?.noteId || card.dataset.note === marginEditor?.sourceId;});
    if (!margin.getClientRects().length) return;
    const top = margin.getBoundingClientRect().top;
    let bottom = 0;
    const blocks = new Map([...paper.querySelectorAll('.blk')].map(b => [b.dataset.id, b]));
    const entries = [...margin.querySelectorAll('.note-card')].filter(card => !card.hidden).map(card => {
      const block = blocks.get(card.dataset.anchor);
      if (!block) return null;
      const mark = [...block.querySelectorAll('[data-easyread-note]')].find(m => m.dataset.easyreadNote === (card.dataset.sourceAnnotation || card.dataset.note) && m.getClientRects().length);
      const rect = mark?.getClientRects()[0] || block.getBoundingClientRect();
      return {card, wanted: rect.top - top};
    }).filter(Boolean);
    if (!dialog.hidden && dialog.parentElement === margin) entries.push({card:dialog,wanted:(marginEditor.anchorRect()?.top || top)-top});
    entries.sort((a,b) => a.wanted - b.wanted || a.card.dataset.note.localeCompare(b.card.dataset.note));
    for (const {card, wanted} of entries) {
      const y = Math.max(wanted, bottom + 18);
      card.style.top = `${Math.round(y)}px`;
      bottom = y + card.offsetHeight;
    }
    margin.style.minHeight = `${Math.max(0, bottom)}px`;
  }
  window.addEventListener('resize', placeNotes);
  new ResizeObserver(() => requestAnimationFrame(() => {placeNotes();refreshGeometry();})).observe(paper);
  const noteResizeObserver = new ResizeObserver(() => requestAnimationFrame(placeNotes));
  document.querySelectorAll('.note-card').forEach(card => noteResizeObserver.observe(card));
  noteResizeObserver.observe(dialog);
  paper.querySelectorAll('img').forEach(image => image.addEventListener('load', placeNotes));
  document.fonts?.ready.then(() => {placeNotes();refreshGeometry();});
  // Keep TeX in the DOM for selection/copy and annotation anchors. The inert
  // image is only the visual layer; selecting an <img> directly leaves a tinted
  // image rectangle with gaps around it and omits the formula from textContent.
  const selectionLayer = document.createElement('div');
  selectionLayer.className = 'selection-layer';
  selectionLayer.setAttribute('aria-hidden', 'true');
  paper.append(selectionLayer);
  const selectionMeasure = document.createElement('canvas').getContext('2d');
  const annotationLayer = document.createElement('div');
  annotationLayer.className = 'annotation-layer';
  annotationLayer.setAttribute('aria-hidden', 'true');
  paper.prepend(annotationLayer);
  function measureRanges(ranges, root) {
    const selected = [...root.querySelectorAll('.math-object')].filter(node =>
      node.getClientRects().length && ranges.some(range => range.intersectsNode(node)));

    const paragraphFor = node => node.closest('p,h1,h2,h3,h4,h5,h6,li,td,th,figcaption,pre,.paragraph-unit,.source,.translated');
    const fragments = [];
    const fonts = new Map();
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let text; (text = walker.nextNode());) {
      const parent = text.parentElement;
      if (parent.closest('.math-object,input,textarea,select') || !text.textContent.trim()) continue;
      const intersecting = ranges.filter(range => range.intersectsNode(text));
      if (!intersecting.length) continue;
      const style = getComputedStyle(parent);
      const fontSize = parseFloat(style.fontSize);
      const lineHeight = parseFloat(style.lineHeight);
      const font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      if (!fonts.has(font)) {
        selectionMeasure.font = font;
        fonts.set(font, selectionMeasure.measureText('Mg').fontBoundingBoxDescent);
      }
      const descent = fonts.get(font) ?? fontSize * .25;
      for (const range of intersecting) {
        const part = document.createRange();
        part.selectNodeContents(text);
        if (range.startContainer === text) part.setStart(text, range.startOffset);
        if (range.endContainer === text) part.setEnd(text, range.endOffset);
        for (const rect of part.getClientRects()) {
          if (rect.width <= 0 || rect.height <= 0) continue;
          const leading = Math.max(0, (lineHeight || rect.height) - rect.height) / 2;
          fragments.push({host: paragraphFor(parent), left: rect.left, right: rect.right,
            top: rect.top - leading, bottom: rect.bottom + leading, inkBottom: rect.bottom,
            baseline: rect.bottom - descent, fontSize, script: !!parent.closest('sup,sub')});
        }
      }
    }
    const mathFragments = selected.map(node => {
      const rect = node.getBoundingClientRect(), style = getComputedStyle(node);
      return {node, host: paragraphFor(node.parentElement), left: rect.left, right: rect.right,
        top: rect.top, bottom: rect.bottom, inkBottom: rect.bottom,
        baseline: rect.bottom + (parseFloat(style.verticalAlign) || 0), fontSize: parseFloat(style.fontSize)};
    });
    return {fragments, mathFragments};
  }
  function paintGeometry(layer, {fragments, mathFragments}, {style = '', color, noteId} = {}) {
    // Keep the local formula band for active selection in notes as well.
    for (const math of style ? [] : mathFragments) {
      const line = [...fragments, ...mathFragments].filter(f => f.host === math.host &&
        Math.abs(f.baseline - math.baseline) < Math.max(2, math.fontSize * .2));
      const top = Math.min(math.top, ...line.map(f => f.top));
      const bottom = Math.max(math.bottom, ...line.map(f => f.bottom));
      math.node.style.setProperty('--math-selection-top', `${top - math.top}px`);
      math.node.style.setProperty('--math-selection-height', `${bottom - top}px`);
    }

    // A formula changes the whole line box, not just its own rectangle. Paint
    // complete selected rows behind the prose, joining adjacent rows at one
    // shared boundary. Never bridge paragraph margins or unselected text.
    const paragraphs = new Map();
    for (const fragment of [...fragments, ...mathFragments]) {
      const host = fragment.host;
      if (!host || !paper.contains(host) || (!style && !host.querySelector('.math-object'))) continue;
      if (!paragraphs.has(host)) paragraphs.set(host, []);
      paragraphs.get(host).push(fragment);
    }
    const origin = paper.getBoundingClientRect();
    for (const [host, pieces] of paragraphs) {
      const rows = [];
      // Establish actual prose/math baselines first, then attach raised
      // footnotes and subscripts to their nearest line rather than inventing
      // an extra row above/below it.
      for (const piece of pieces.sort((a,b) => Number(!!a.script) - Number(!!b.script) || a.baseline - b.baseline)) {
        const nearest = rows.reduce((best, row) => !best || Math.abs(row.baseline-piece.baseline) < Math.abs(best.baseline-piece.baseline) ? row : best, null);
        let row = nearest && Math.abs(nearest.baseline - piece.baseline) < Math.max(2, piece.fontSize * (piece.script ? 1.1 : .4)) ? nearest : null;
        if (!row) {row = {...piece}; rows.push(row);}
        else {
          row.left = Math.min(row.left, piece.left); row.right = Math.max(row.right, piece.right);
          row.top = Math.min(row.top, piece.top); row.bottom = Math.max(row.bottom, piece.bottom);
          row.inkBottom = Math.max(row.inkBottom, piece.inkBottom);
        }
      }
      rows.sort((a,b) => a.baseline - b.baseline);
      for (let i = 1; i < rows.length; i++) {
        const boundary = (rows[i-1].bottom + rows[i].top) / 2;
        rows[i-1].bottom = boundary;
        rows[i].top = boundary;
      }
      const bounds = host.getBoundingClientRect(), hostStyle = getComputedStyle(host);
      const left = bounds.left + (parseFloat(hostStyle.borderLeftWidth) || 0) + (parseFloat(hostStyle.paddingLeft) || 0);
      const right = bounds.right - (parseFloat(hostStyle.borderRightWidth) || 0) - (parseFloat(hostStyle.paddingRight) || 0);
      const group = document.createElement('div');
      group.className = style ? 'annotation-paragraph' : 'selection-paragraph';
      group.dataset.anchor = host.closest('.blk')?.dataset.id || '';
      if (style) {group.dataset.style = style; group.dataset.note = noteId;}
      rows.forEach((row, index) => {
        const band = document.createElement('span');
        band.className = style ? 'annotation-band' : 'selection-band';
        // Wrapped lines select through the column edge; keep the first and
        // last endpoints exact for partial and backwards selections.
        const x = index > 0 ? left : row.left;
        const end = index < rows.length - 1 ? right : row.right;
        // Snap the shared boundaries identically; independently rounded CSS
        // heights can otherwise leave a fractional-pixel seam between rows.
        const top = index === 0 ? Math.floor(row.top - origin.top) : Math.round(row.top - origin.top);
        const bottom = index === rows.length - 1 ? Math.ceil(row.bottom - origin.top) : Math.round(row.bottom - origin.top);
        band.style.left = `${x - origin.left}px`;
        band.style.top = `${style === 'underline' ? Math.ceil(row.inkBottom - origin.top) + 1 : top}px`;
        band.style.width = `${Math.max(0, end - x)}px`;
        band.style.height = `${style === 'underline' ? 2 : Math.max(0, bottom - top)}px`;
        if (color) band.style.backgroundColor = color;
        group.append(band);
      });
      layer.append(group);
    }
  }
  function updateMathSelection() {
    const selection = window.getSelection();
    const range = selection?.rangeCount && !selection.isCollapsed ? selection.getRangeAt(0) : null;
    selectionLayer.replaceChildren();
    const geometry = range ? measureRanges([range], reader) : {fragments:[],mathFragments:[]};
    const active = new Set(geometry.mathFragments.map(f => f.node));
    reader.querySelectorAll('.math-object').forEach(node => node.classList.toggle('math-selected', active.has(node)));
    paintGeometry(selectionLayer, geometry);
  }
  function updateAnnotations() {
    annotationLayer.replaceChildren();
    const groups = new Map();
    const lineColors = {yellow:'#dca722',green:'#58994c',blue:'#477fc1',pink:'#cc413a'};
    const fills = new Set(['#f2df8a','#d4e3c9','#d2e0ed','#f0cec7']);
    for (const decoration of paper.querySelectorAll('[data-easyread-note] mark, [data-easyread-note] u')) {
      const marker = decoration.closest('[data-easyread-note]');
      const root = decoration.closest('.blk');
      const style = decoration.tagName === 'U' ? 'underline' : 'highlight';
      const color = style === 'underline' ? lineColors[decoration.dataset.underline] || '#dca722' :
        (fills.has(decoration.getAttribute('color')) ? decoration.getAttribute('color') : '#f2df8a');
      decoration.classList.add('annotation-rendered');
      const key = `${marker.dataset.easyreadNote}:${style}:${color}`;
      if (!groups.has(key)) groups.set(key, {root, ranges:[], style, color, noteId:marker.dataset.easyreadNote});
      const range = document.createRange();range.selectNodeContents(decoration);
      groups.get(key).ranges.push(range);
    }
    for (const group of groups.values()) paintGeometry(annotationLayer, measureRanges(group.ranges, group.root), group);
  }
  function refreshGeometry() {updateAnnotations();updateMathSelection();}
  paper.addEventListener('toggle', event => {
    if (!event.target.matches('.pdf-footnotes')) return;
    if (event.target.open) renderMath(event.target);
    requestAnimationFrame(() => {refreshGeometry();placeNotes();});
  }, true);
  document.addEventListener('selectionchange', updateMathSelection);
  function installMath(node, image, metrics) {
    const text = document.createElement('span');
    text.className = 'math-tex';
    text.append(...node.childNodes);
    const marker = text.querySelector('[data-easyread-note]');
    if (marker) node.dataset.easyreadNote = marker.dataset.easyreadNote;
    node.replaceChildren(text, image);node.classList.add('math-object');
    for (const [name,value] of Object.entries(metrics)) node.style[name] = value;
  }
  function reuseMath(previous, next) {
    const cached = new Map();
    const key = node => JSON.stringify([node.dataset.vellioMath,node.dataset.display]);
    for (const node of previous.querySelectorAll('.math-object')) {
      const image = node.querySelector('.math-svg');
      if (!image) continue;
      if (!cached.has(key(node))) cached.set(key(node), []);
      cached.get(key(node)).push({image,metrics:{width:node.style.width,aspectRatio:node.style.aspectRatio,verticalAlign:node.style.verticalAlign}});
    }
    for (const node of next.querySelectorAll('[data-vellio-math]')) {
      const match = cached.get(key(node))?.shift();
      // Move the already-decoded image: cloning its revoked Blob URL would
      // force a failing reload and briefly collapse the formula's line box.
      if (match) installMath(node,match.image,match.metrics);
    }
  }
  // Only application-generated SVG is loaded, never source-document SVG/HTML.
  function renderMath(root) {
    root.querySelectorAll('[data-vellio-math]:not(.math-object)').forEach(async node => {
      // Paired PDF pages keep a hidden single-language fallback. Do not render
      // every formula three times; switching reading modes reloads the page.
      if (node.closest('.blk.paired') && !node.getClientRects().length) return;
      const url = new URL('/math.svg', location.origin);
      url.searchParams.set('tex', node.dataset.vellioMath);
      url.searchParams.set('display', node.dataset.display || 'false');
      let objectUrl;
      try {
        const response = await fetch(url);
        if (!response.ok) return; // Preserve readable TeX on compile failure.
        const svg = await response.text();
        // Read numeric metrics only; never inject parsed markup into the DOM.
        const metrics = new DOMParser().parseFromString(svg, 'image/svg+xml').documentElement;
        const width = Number(metrics.getAttribute('width'));
        const height = Number(metrics.getAttribute('height'));
        const baseline = Number(metrics.getAttribute('data-baseline'));
        const fontSize = Number(metrics.getAttribute('data-font-size'));
        if (![width, height, baseline, fontSize].every(Number.isFinite) || width <= 0 || height <= 0 || fontSize <= 0 || baseline < 0 || baseline > height) return;
        const image = new Image();
        image.className = 'math-svg';
        image.alt = '';
        image.setAttribute('aria-hidden', 'true');
        image.draggable = false;
        objectUrl = URL.createObjectURL(new Blob([svg], {type: 'image/svg+xml'}));
        image.src = objectUrl;
        await image.decode();
        if (!node.isConnected) return;
        // Keep new annotation metadata, but reuse exactly the same metrics
        // when a later partial update moves this decoded image into fresh HTML.
        installMath(node,image,{width:`${width / fontSize}em`,aspectRatio:`${width} / ${height}`,verticalAlign:`${(baseline - height) / fontSize}em`});
        refreshGeometry();
        placeNotes();
      } catch {
        // A network/parse/decode failure must leave the original TeX intact.
      } finally {
        if (objectUrl) URL.revokeObjectURL(objectUrl);
      }
    });
  }
  const updates = window.VellioReaderUpdates({reader,paper,bar,noteForm:form,prepareBlocks,reuseMath,renderMath,
    refreshGeometry,placeNotes,applyNoteFilter,
    observeCard:card => noteResizeObserver.observe(card),unobserveCard:card => noteResizeObserver.unobserve(card),
    captureAction:() => pending,
    onSaved:(submitted,action) => {
      updateProgress();
      if (submitted.getAttribute('action')?.endsWith('/notes/delete')) marginEditor?.forget(submitted.elements.note_id.value);
      if (existingCard && submitted === existingCard.querySelector('.note-edit form') && pending === action) {
        close();window.getSelection()?.removeAllRanges();
      }
      if (submitted === form && pending === action) {close();window.getSelection()?.removeAllRanges();}
    }
  });
  marginEditor = window.VellioMarginEditor({reader,paper,view:dialog,updates,
    onLayout:()=>requestAnimationFrame(placeNotes),onOpen:()=>showPanel(null)});
  if (location.hash.startsWith('#edit-note-')) {
    const id=decodeURIComponent(location.hash.slice('#edit-note-'.length));
    const card=[...reader.querySelectorAll('.margin .note-card')].find(card=>card.dataset.note===id);
    if(card)requestAnimationFrame(()=>{
      const url=new URL(location.href);url.hash=`block-${card.dataset.anchor}`;history.replaceState(history.state,'',url);
      document.getElementById(`block-${card.dataset.anchor}`)?.scrollIntoView({block:'center'});openCardEditor(card);
    });
  }
  renderMath(reader);
  placeNotes();
  paper.dataset.ready = 'true';
})();
