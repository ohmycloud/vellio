// Fixed same-origin endpoints. Release notes are text, never executable HTML.
(() => {
  if (!document.querySelector('#about-updates')) return;
  const button = document.querySelector('#check-updates');
  const automatic = document.querySelector('#automatic-updates');
  const error = document.querySelector('#update-error');
  let timer;
  function render(state) {
    button.disabled = state.checking;
    automatic.checked = state.enabled;
    document.querySelector('#update-status').textContent = state.status;
    error.textContent = state.error;
    document.querySelector('#update-release').hidden = !state.latest;
    document.querySelector('#update-latest').textContent = `What's new in ${state.latest}`;
    document.querySelector('#update-notes').textContent = state.notes;
    const download = document.querySelector('#download-update');
    download.hidden = !state.download;
    if (state.download) download.href = state.download;
    else download.removeAttribute('href');
    download.textContent = `Download for ${state.platform}`;
    document.querySelector('#update-no-package').hidden = !!state.download;
    clearTimeout(timer);
    if (state.checking) timer = setTimeout(() => request('status'), 800);
  }
  async function request(action, body) {
    try {
      const response = await fetch(`/settings/updates/${action}`, {
        method: action === 'status' ? 'GET' : 'POST',
        ...(body ? {body: new URLSearchParams(body)} : {}),
        credentials: 'same-origin',
      });
      if (!response.ok) throw new Error(`Update request failed (${response.status}). Please retry.`);
      render(await response.json());
    } catch (e) {
      button.disabled = false;
      error.textContent = e.message;
    }
  }
  button.addEventListener('click', () => {
    button.disabled = true;
    document.querySelector('#update-status').textContent = 'Checking…';
    request('check');
  });
  automatic.addEventListener('change', async () => {
    automatic.disabled = true;
    try { await request('automatic', {enabled: String(automatic.checked)}); }
    finally { automatic.disabled = false; }
  });
  request('status');
})();
