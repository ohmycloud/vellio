// Progressive enhancement: existing POST/303 routes still work without JS.
// Fetch the server's sanitized reader HTML, but never replace the document,
// reader shell, unrelated paragraphs, or a live note editor.
window.VellioReaderUpdates = function ({reader, paper, bar, noteForm, prepareBlocks,
  reuseMath, renderMath, refreshGeometry, placeNotes, applyNoteFilter, observeCard, unobserveCard,
  captureAction, onSaved}) {
  const editors = new Map(), busy = new Set();
  const actionUrl = form => new URL(form.getAttribute('action') || location.href,location.href);
  let queue = Promise.resolve(), messageVersion = 0, failureForm = null;
  const feedback = document.createElement('div');
  feedback.className = 'reader-feedback';feedback.hidden = true;
  feedback.setAttribute('role','status');feedback.setAttribute('aria-live','polite');
  document.body.append(feedback);
  function message(text, error=false, retry) {
    const version=++messageVersion;
    feedback.replaceChildren(document.createTextNode(text));
    feedback.hidden=false;feedback.classList.toggle('error',error);
    if(retry) {
      const button=document.createElement('button');button.type='button';button.textContent='重试更新显示';
      button.onclick=()=>{button.disabled=true;queue=queue.then(retry).catch(()=>{});};feedback.append(button);
    } else if(!error) setTimeout(()=>{if(version===messageVersion)feedback.hidden=true},2500);
  }
  const isMargin = form => form.matches('.margin-note-form');
  function status(form,name,error) {
    const labels = isMargin(form) ? {dirty:'Unsaved changes',saving:'Autosaving…',saved:'Autosaved',error:'Save failed',partial:'Saved · refresh failed'} :
      {dirty:'尚未保存',saving:'正在保存…',saved:'已自动保存',error:'保存失败，请重试',partial:'已保存，显示更新失败'};
    form.dataset.saveState=name;
    const label=form.querySelector('.autosave-status');
    if(label)label.textContent=(isMargin(form)&&name==='dirty'&&form.dataset.persisted!=='true'&&!form.elements.body.value.trim()?'Autosave':labels[name])+(error?` (${error.message})`:'');
  }
  function bindEditor(form) {
    if(editors.has(form))return;
    const state={revision:0,dirty:false,bodyDirty:false,timer:null,resave:false};editors.set(form,state);
    const changed=event=>{
      if(event.type==='change'&&!event.target.matches('select'))return;
      state.dirty=true;state.revision++;clearTimeout(state.timer);
      if(isMargin(form))form.elements.revision.value=String(state.revision);
      status(form,'dirty');
      if(event.target.matches('textarea')||isMargin(form)) {
        state.bodyDirty=true;
        if(!isMargin(form)||form.dataset.persisted==='true'||form.elements.body.value.trim())
          state.timer=setTimeout(()=>submit(form,{auto:true}),700);
      }
    };
    form.addEventListener('input',changed);form.addEventListener('change',changed);
  }
  function bindEditors(root) {root.querySelectorAll('.note-edit form').forEach(bindEditor);}
  function cancelEditor(form) {
    const state=editors.get(form);if(!state)return;
    clearTimeout(state.timer);state.revision++;state.dirty=false;editors.delete(form);
  }
  function syncCard(card, incoming, changedId) {
    if(card.dataset.note!==changedId)return;
    card.className=incoming.className;
    for(const [key,value] of Object.entries(incoming.dataset))card.dataset[key]=value;
    for(const selector of [':scope > .tag',':scope > .quote',':scope > .body']) {
      const old=card.querySelector(selector),next=incoming.querySelector(selector);
      if(old&&next)old.replaceChildren(...next.childNodes);
    }
    // Action labels, availability and kind-specific AI controls change with
    // the note. Keep the card and live editor, but refresh these small slots.
    for(const selector of [':scope > .note-ai-slot',':scope > .card-actions']) {
      const old=card.querySelector(selector),next=incoming.querySelector(selector);
      if(old&&next&&old.innerHTML!==next.innerHTML)old.replaceChildren(...next.childNodes);
    }
    const form=card.querySelector('.note-edit form'),next=incoming.querySelector('.note-edit form');
    // A second keystroke while a save was in flight is still an unsaved draft.
    // Never overwrite it, close its details, replace its DOM, or move its caret.
    if(form&&next&&!editors.get(form)?.dirty)for(const input of next.elements) {
      const current=form.elements.namedItem(input.name);
      if(input.name&&current&&current.value!==input.value)current.value=input.value;
    }
  }
  function syncCards(parent, incoming, selector, changedId) {
    const old=new Map([...parent.querySelectorAll(selector)].map(card=>[card.dataset.note,card]));
    const next=[...incoming.querySelectorAll(selector)],ids=new Set(next.map(card=>card.dataset.note));
    for(const [id,card] of old)if(!ids.has(id)) {
      card.querySelectorAll('.note-edit form').forEach(cancelEditor);unobserveCard(card);card.remove();
    }
    let cursor=parent.querySelector(selector);
    for(const incomingCard of next) {
      let card=old.get(incomingCard.dataset.note);
      if(card)syncCard(card,incomingCard,changedId);
      else {card=document.importNode(incomingCard,true);observeCard(card);}
      if(card!==cursor)parent.insertBefore(card,cursor);
      cursor=card.nextElementSibling;
    }
  }
  function viewport() {
    const block=[...paper.querySelectorAll('.blk')].find(node=>node.getBoundingClientRect().bottom>80);
    return {x:scrollX,y:scrollY,id:block?.id,top:block?.getBoundingClientRect().top};
  }
  function patch(doc, anchor, changedId) {
    const nextReader=doc.querySelector('.reader-body'),nextMargin=doc.querySelector('.margin'),nextNotes=doc.querySelector('#notes-drawer');
    if(!nextReader||!nextMargin||!nextNotes||nextReader.dataset.bookId!==reader.dataset.bookId||nextReader.dataset.page!==reader.dataset.page)
      throw new Error('返回的阅读页不匹配');
    const position=viewport(); // Capture now, not at POST time: the reader may have scrolled during the request.
    const panelScroll=[...reader.querySelectorAll('.tool-panel')].map(panel=>[panel,panel.scrollTop]);
    const old=anchor&&[...paper.querySelectorAll('.blk')].find(node=>node.dataset.id===anchor);
    const incoming=anchor&&[...doc.querySelectorAll('.paper .blk')].find(node=>node.dataset.id===anchor);
    if(old&&incoming) {
      const container=document.createElement('div'),replacement=document.importNode(incoming,true);
      container.append(replacement);prepareBlocks(container,old);reuseMath(old,replacement);old.replaceWith(replacement);
    }
    syncCards(reader.querySelector('.margin'),nextMargin,'.note-card',changedId);
    syncCards(reader.querySelector('#notes-drawer'),nextNotes,'.drawer-note',changedId);
    bindEditors(reader);applyNoteFilter();renderMath(reader);refreshGeometry();placeNotes();
    const stable=position.id&&document.getElementById(position.id);
    const delta=stable?stable.getBoundingClientRect().top-position.top:0;
    window.scrollTo({left:position.x,top:position.y+delta,behavior:'instant'});
    for(const [panel,top] of panelScroll)panel.scrollTop=top;
    for(const [form] of editors)if(!form.isConnected)cancelEditor(form);
  }
  async function refresh(anchor, changedId) {
    const url=new URL(location.href);url.hash='';
    url.searchParams.set('page',reader.dataset.page);
    const response=await fetch(url,{cache:'no-store'});
    if(!response.ok)throw new Error(`HTTP ${response.status}`);
    const doc=new DOMParser().parseFromString(await response.text(),'text/html');
    // Only app-generated sanitized block/card markup is imported. No scripts,
    // styles, navigation, or server error pages are installed or executed.
    patch(doc,anchor,changedId);
  }
  function submit(form,{auto=false,submitter}={}) {
    const state=editors.get(form);
    if(busy.has(form)){if(state)state.resave=true;return;}
    if(auto&&(!state?.dirty||!form.isConnected))return;
    if(isMargin(form)&&form.dataset.persisted!=='true'&&!form.elements.body.value.trim())return;
    const revision=state?.revision,action=captureAction();
    if(state)clearTimeout(state.timer);
    const payload=new URLSearchParams(new FormData(form));
    if(submitter?.name)payload.set(submitter.name,submitter.value);
    const changedId=payload.get('note_id');
    const anchor=payload.get('anchor')||form.closest('.blk,.note-card,.drawer-note')?.dataset.id||form.closest('.note-card,.drawer-note')?.dataset.anchor;
    const deleting=actionUrl(form).pathname.endsWith('/notes/delete');
    if(deleting)for(const [editor,state] of editors)if(editor.elements.note_id.value===changedId) {
      clearTimeout(state.timer);state.revision++;state.dirty=false;state.bodyDirty=false;
    }
    const disabled=new Map();
    const controls=form===noteForm?[...form.elements,...bar.querySelectorAll('button')]:[...form.querySelectorAll('button[type=submit],button:not([type])')];
    for(const control of controls){disabled.set(control,control.disabled);control.disabled=true;}
    busy.add(form);reader.dataset.updating='true';
    status(form,'saving');
    queue=queue.then(async()=>{
      // Coalesce obsolete queued autosaves; explicit saves retain their snapshot.
      if(auto&&(!form.isConnected||!editors.has(form)||state.revision!==revision))return;
      let saved=false;
      try {
        const response=await fetch(actionUrl(form),{method:'POST',body:payload,redirect:'manual'});
        if(response.status!==303&&response.type!=='opaqueredirect')throw new Error(`HTTP ${response.status}`);
        saved=true;
        if(isMargin(form))form.dataset.persisted='true';
        if(state&&state.revision===revision){state.dirty=false;state.bodyDirty=false;}
        await refresh(anchor,changedId);
        onSaved(form,action);if(!state?.dirty)status(form,'saved');
        if(!auto||failureForm===form) {failureForm=null;message(deleting?'已删除':auto?'已自动保存':'已保存');}
      } catch(error) {
        failureForm=form;
        if(saved) {
          onSaved(form,action);
          const retry=async()=>{
            reader.dataset.updating='true';
            try {
              await refresh(anchor,changedId);if(failureForm===form)failureForm=null;message('已更新显示');
              if(!state?.dirty)status(form,'saved');
            } catch(error){message(`已保存，但显示更新失败（${error.message}）`,true,retry);}
            finally {reader.dataset.updating=String(busy.size>0);}
          };
          message(`已保存，但显示更新失败（${error.message}）；重试不会重复写入。`,true,retry);
          status(form,'partial');
        } else {
          if(state)state.dirty=true;
          message(`保存未完成（${error.message}），请保留草稿并重试。`,true);
          status(form,'error',error);
        }
      }
    }).catch(error=>message(`更新失败（${error.message}）`,true)).finally(()=>{
      busy.delete(form);reader.dataset.updating=String(busy.size>0);
      for(const [control,wasDisabled] of disabled)control.disabled=wasDisabled;
      const resave=state?.resave;if(state)state.resave=false;
      if(resave&&state.dirty&&form.isConnected&&editors.has(form))submit(form,{auto:true});
    });
  }
  document.addEventListener('submit',event=>{
    const form=event.target;
    if(!(form instanceof HTMLFormElement))return;
    const url=actionUrl(form);
    const prefix=`/read/${encodeURIComponent(reader.dataset.bookId)}/`;
    if(url.origin!==location.origin||!url.pathname.startsWith(prefix)||
      !['notes','notes/margin','notes/edit','notes/action','notes/delete','paper-note','translation'].includes(url.pathname.slice(prefix.length)))return;
    event.preventDefault();submit(form,{submitter:event.submitter});
  });
  window.addEventListener('pagehide',()=>{
    for(const [form,state] of editors) {
      clearTimeout(state.timer);
      if(state.bodyDirty&&form.isConnected)navigator.sendBeacon(actionUrl(form),new URLSearchParams(new FormData(form)));
    }
  });
  bindEditors(reader);
  return {bind:bindEditor,dispose:cancelEditor,flush:form=>{if(editors.get(form)?.dirty)submit(form,{auto:true});},
    isDirty:form=>!!editors.get(form)?.dirty||busy.has(form)};
};
