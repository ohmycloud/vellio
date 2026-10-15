(() => {
  const list = document.getElementById('home-list');
  if (!list) return;
  const buttons = [...document.querySelectorAll('[data-filter]')];
  const search = document.getElementById('library-search');
  const sort = document.getElementById('library-sort');
  const count = document.getElementById('home-count');
  const heading = document.querySelector('.home-top h1');
  const batch = document.getElementById('batch-form');
  const master = document.getElementById('batch-select-all');
  const selected = new Set();
  const requested = new URLSearchParams(location.search).get('filter');
  let filter = buttons.find(button => button.dataset.filter === requested)?.dataset.filter || 'all';
  buttons.forEach(button => button.classList.toggle('active', button.dataset.filter === filter));
  if (filter !== 'all') heading.textContent = buttons.find(button => button.dataset.filter === filter).textContent.trim();
  function updateBatch() {
    const visible = [...list.querySelectorAll('.home-row:not([hidden]) .row-select')];
    master.checked = visible.length > 0 && visible.every(input => selected.has(input.value));
    master.indeterminate = visible.some(input => selected.has(input.value)) && !master.checked;
    batch.elements.ids.value = [...selected].join(',');
    document.getElementById('batch-count').textContent = `${selected.size} 篇已选`;
    for (const button of batch.querySelectorAll('button')) button.disabled = !selected.size;
    batch.querySelector('.batch-restore').hidden = filter !== 'trash';
    batch.querySelector('.batch-confirm').hidden = filter === 'trash';
    batch.querySelector('[data-batch-cite]').hidden = filter === 'trash';
    batch.querySelectorAll('select').forEach(control => control.hidden = filter === 'trash');
    batch.querySelectorAll('button[value=status],button[value=assign],button[value=remove-folder]').forEach(button => button.hidden = filter === 'trash');
  }
  function update() {
    const query = search.value.toLocaleLowerCase().trim();
    const rows = [...list.querySelectorAll('.home-row')];
    rows.sort((a,b) => {
      if (sort.value === 'title') return a.dataset.title.localeCompare(b.dataset.title);
      if (sort.value === 'year') return Number(b.dataset.year) - Number(a.dataset.year) || a.dataset.title.localeCompare(b.dataset.title);
      const field = sort.value === 'added' ? 'added' : 'opened';
      return b.dataset[field].localeCompare(a.dataset[field]) || a.dataset.title.localeCompare(b.dataset.title);
    });
    list.append(...rows);
    let shown = 0;
    for (const row of rows) {
      const removed = row.dataset.removed === 'true';
      const matches = filter === 'trash' ? removed : !removed &&
        (filter === 'all' || row.dataset.status === filter ||
          (filter === 'starred' && row.dataset.starred === 'true') ||
          (filter === 'questions' && Number(row.dataset.questions) > 0) ||
          row.dataset.folders.split(' ').includes(filter));
      row.hidden = !matches || !`${row.dataset.title} ${row.dataset.details} ${row.dataset.folderNames}`.includes(query);
      if (!row.hidden) shown++;
    }
    count.textContent = `${shown} 篇`;
    document.getElementById('home-empty').hidden = shown !== 0;
    updateBatch();
  }
  buttons.forEach(button => button.addEventListener('click', () => {
    filter = button.dataset.filter;
    buttons.forEach(item => item.classList.toggle('active', item === button));
    heading.textContent = button.textContent.trim().replace(/^[^\p{L}\p{N}]+/u, '').trim();
    update();
  }));
  search.addEventListener('input', update);
  sort.addEventListener('change', update);
  master.addEventListener('change', () => {
    for (const checkbox of list.querySelectorAll('.home-row:not([hidden]) .row-select')) {
      checkbox.checked = master.checked;
      if (master.checked) selected.add(checkbox.value); else selected.delete(checkbox.value);
    }
    updateBatch();
  });
  list.addEventListener('change', event => {
    if (!event.target.matches('.row-select')) return;
    if (event.target.checked) selected.add(event.target.value); else selected.delete(event.target.value);
    updateBatch();
  });
  batch.addEventListener('submit', event => {
    const action = event.submitter?.value;
    if (!selected.size || (['assign','remove-folder'].includes(action) && !batch.elements.folder.value)) {
      event.preventDefault();
      window.alert(selected.size ? '请先选择文件夹' : '请先选择文献');
    }
  });
  batch.querySelector('[data-batch-cite]').addEventListener('click', async () => {
    const citations = [...selected].map(id => [...list.querySelectorAll('.home-row')].find(row => row.dataset.book === id)?.dataset.citation).filter(Boolean).join('\n\n');
    try {await navigator.clipboard.writeText(citations);document.getElementById('batch-count').textContent = '引文已复制';}
    catch {window.alert('无法写入剪贴板，请检查浏览器权限');}
  });
  list.addEventListener('dragstart', event => {
    const row = event.target.closest('.home-row');
    if (!row || event.target.closest('input,button,select,textarea')) {event.preventDefault();return;}
    event.dataTransfer.setData('text/plain', row.dataset.book);
    event.dataTransfer.effectAllowed = 'copy';
  });
  for (const target of document.querySelectorAll('[data-folder-drop]')) {
    target.addEventListener('dragover', event => {
      if (!event.dataTransfer.types.includes('text/plain')) return;
      event.preventDefault();
      target.classList.add('drop-target');
      event.dataTransfer.dropEffect = 'copy';
    });
    target.addEventListener('dragleave', () => target.classList.remove('drop-target'));
    target.addEventListener('drop', async event => {
      event.preventDefault();
      target.classList.remove('drop-target');
      const book = event.dataTransfer.getData('text/plain');
      if (![...list.querySelectorAll('.home-row')].some(row => row.dataset.book === book)) return;
      try {
        const response = await fetch('/library/folders', {method:'POST',redirect:'manual',
          body:new URLSearchParams({action:'assign',book,folder:target.dataset.folderDrop})});
        if (response.status !== 303 && response.type !== 'opaqueredirect') throw new Error(`HTTP ${response.status}`);
        location.assign(`/?filter=${encodeURIComponent(target.dataset.folderDrop)}`);
      } catch (error) {window.alert(`移动失败：${error.message}`);}
    });
  }
  update();
})();
