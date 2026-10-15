// Desktop-style page-margin writing: one stable note ID, no Save/Cancel modal.
window.VellioMarginEditor = function ({reader,paper,view,updates,onLayout,onOpen}) {
  const template = document.getElementById('margin-note-template');
  const drafts = document.getElementById('margin-drafts');
  const sessions = new Map();
  const fills = {yellow:'#f2df8a',green:'#d4e3c9',blue:'#d2e0ed',pink:'#f0cec7'};
  let active = null;
  const cards = () => [...reader.querySelectorAll('.margin .note-card')];
  const value = (form,key) => form.elements.namedItem(key).value;
  function fromCard(card) {
    const form = card.querySelector('.note-edit form');
    return {note_id:card.dataset.note,anchor:card.dataset.anchor,side:card.dataset.side,
      quote:card.querySelector(':scope > .quote')?.textContent || '',prefix:card.dataset.prefix || '',suffix:card.dataset.suffix || '',
      source_annotation:card.dataset.sourceAnnotation || '',kind:value(form,'kind'),body:value(form,'body'),
      color:value(form,'color'),answered:value(form,'answered'),created:card.dataset.created};
  }
  function dateLabel(created) {
    const date = new Date(created), pad = n => String(n).padStart(2,'0');
    return Number.isNaN(date.getTime()) ? '' : `${pad(date.getMonth()+1)}/${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
  }
  function render(session) {
    const form = session.form, question = value(form,'kind') === 'question', answered = value(form,'answered') === 'yes';
    form.querySelector('.margin-note-title').textContent = question ? 'My question' : 'My note';
    const badge = form.querySelector('.margin-question-badge');
    badge.hidden = !question;badge.textContent = answered ? 'Answered' : 'Unanswered';badge.classList.toggle('answered',answered);
    const time = form.querySelector('time');time.dateTime = session.created;time.textContent = dateLabel(session.created);
    form.querySelector('.margin-note-quote').textContent = `“${value(form,'quote')}”`;
    form.style.setProperty('--quote-color',fills[value(form,'color')] || fills.yellow);
    form.querySelectorAll('[data-note-kind]').forEach(button => button.setAttribute('aria-pressed',String(button.dataset.noteKind === value(form,'kind'))));
    form.querySelectorAll('[data-note-color]').forEach(button => button.setAttribute('aria-pressed',String(button.dataset.noteColor === value(form,'color'))));
    const toggle = form.querySelector('[data-note-answered]');toggle.hidden = !question;
    toggle.textContent = answered ? 'Mark as unanswered' : 'Mark as answered';
    toggle.setAttribute('aria-pressed',String(answered));
  }
  function change(form,key,next) {
    if (value(form,key) === next) return;
    form.elements.namedItem(key).value = next;
    form.elements.namedItem(key).dispatchEvent(new Event('input',{bubbles:true}));
  }
  function create(record,persisted) {
    const form = template.content.querySelector('form').cloneNode(true);
    for (const [key,val] of Object.entries(record)) if (form.elements.namedItem(key)) form.elements.namedItem(key).value = val;
    form.elements.session_id.value = crypto.randomUUID();
    form.dataset.persisted = String(persisted);
    const session = {form,created:record.created || new Date().toISOString(),range:null};
    form.addEventListener('input',() => {render(session);onLayout();});
    form.addEventListener('click',event => {
      const button = event.target.closest('button');if (!button) return;
      if (button.hasAttribute('data-margin-close')) {close();return;}
      if (button.dataset.noteKind) change(form,'kind',button.dataset.noteKind);
      if (button.dataset.noteColor) change(form,'color',button.dataset.noteColor);
      if (button.hasAttribute('data-note-answered')) change(form,'answered',value(form,'answered') === 'yes' ? 'no' : 'yes');
      if (button.hasAttribute('data-note-retry')) updates.flush(form);
    });
    drafts.append(form);updates.bind(form);render(session);
    sessions.set(record.note_id,session);
    return session;
  }
  function open(source,{kind='note',card=null,color='yellow',range=null}={}) {
    let sourceAnnotation = '';
    if (card && value(card.querySelector('.note-edit form'),'kind') === 'highlight') {
      sourceAnnotation = card.dataset.note;
      card = cards().find(item => item.dataset.sourceAnnotation === sourceAnnotation) || null;
    }
    if (!card) card = cards().find(item => {
      const record = fromCard(item);
      return ['note','question'].includes(record.kind) && record.anchor === source.anchor && record.side === source.side &&
        record.quote === source.quote && record.prefix === source.prefix && (!sourceAnnotation || record.source_annotation === sourceAnnotation);
    });
    const record = card ? fromCard(card) : {...source,source_annotation:sourceAnnotation,kind,body:'',color,answered:'no'};
    let session = card && sessions.get(record.note_id);
    if (!session && !card) session = [...sessions.values()].find(({form}) => value(form,'anchor') === source.anchor &&
      value(form,'side') === source.side && value(form,'quote') === source.quote && value(form,'prefix') === source.prefix &&
      value(form,'source_annotation') === sourceAnnotation);
    if (active && active !== session) close();
    if (!session) session = create({...record,note_id:record.note_id || crypto.randomUUID()},!!card);
    else if (card && !updates.isDirty(session.form)) {
      for (const [key,val] of Object.entries(record)) if (session.form.elements.namedItem(key)) session.form.elements.namedItem(key).value = val;
      session.form.dataset.persisted = 'true';session.created = record.created || session.created;
    }
    active = session;session.range = range?.cloneRange() || session.range;
    view.replaceChildren(session.form);view.dataset.note = value(session.form,'note_id');view.hidden = false;
    onOpen();place();render(session);
    if (kind) change(session.form,'kind',kind);
    onLayout();
    session.form.elements.body.focus({preventScroll:true});
  }
  function close() {
    if (!active) return;
    updates.flush(active.form);drafts.append(active.form);
    active = null;view.hidden = true;delete view.dataset.note;
    reader.classList.remove('margin-editing');onLayout();
  }
  function anchorRect() {
    if (!active) return null;
    const form = active.form, block = [...paper.querySelectorAll('.blk')].find(b => b.dataset.id === value(form,'anchor'));
    if (!block) return null;
    const id = value(form,'source_annotation') || value(form,'note_id');
    const marker = [...block.querySelectorAll('[data-easyread-note]')].find(node => node.dataset.easyreadNote === id &&
      !node.closest('.math-tex') && node.getClientRects().length);
    if (marker) return marker.getClientRects()[0];
    if (active.range?.startContainer.isConnected && !active.range.collapsed) {
      const rect = active.range.getBoundingClientRect();if (rect.height) return rect;
    }
    // A saved block can be replaced in place. Recover the quote from its text,
    // including hidden TeX, without holding a disconnected Range forever.
    const side = value(form,'side') === 'en' ? '.source' : '.translated';
    const quote = value(form,'quote');
    for (const root of block.querySelectorAll(side)) {
      if (!root.getClientRects().length || !quote) continue;
      const text = root.textContent;let offset = text.indexOf(quote);
      while (offset >= 0) {
        const prefix = value(form,'prefix');
        if (!prefix || text.slice(Math.max(0,offset-prefix.length),offset) === prefix || offset === 0) break;
        offset = text.indexOf(quote,offset+1);
      }
      if (offset < 0) continue;
      const walker = document.createTreeWalker(root,NodeFilter.SHOW_TEXT);
      let start = 0,first = null,last = null;
      for (let node;(node=walker.nextNode());) {
        const end = start+node.length;
        if (!first && offset < end) first = [node,offset-start];
        if (first && offset+quote.length <= end) {last=[node,offset+quote.length-start];break;}
        start = end;
      }
      if (first && last) {
        const range = document.createRange();range.setStart(...first);range.setEnd(...last);active.range = range;
        return range.getClientRects()[0] || root.getBoundingClientRect();
      }
    }
    return block.getBoundingClientRect();
  }
  function place() {
    if (!active) return;
    const wide = innerWidth >= 1200, host = wide ? reader.querySelector('.margin') : document.body;
    reader.classList.toggle('margin-editing',wide);
    view.classList.toggle('docked',!wide);
    if (view.parentElement !== host) host.append(view);
    if (!wide) {
      const rect = anchorRect();
      view.style.top = `${Math.max(65,Math.min(innerHeight-view.offsetHeight-12,rect?.top || 85))}px`;
    }
  }
  function forget(id) {
    const session = sessions.get(id);if (!session) return;
    updates.dispose(session.form);session.form.remove();sessions.delete(id);
    if (active === session) {active = null;view.hidden = true;delete view.dataset.note;reader.classList.remove('margin-editing');}
    onLayout();
  }
  return {open,close,place,anchorRect,forget,
    get noteId(){return active ? value(active.form,'note_id') : null;},
    get sourceId(){return active ? value(active.form,'source_annotation') : null;}};
};
