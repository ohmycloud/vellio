(() => {
  const dialog = document.getElementById('model-dialog');
  const form = document.getElementById('model-editor');
  if (!dialog || !form) return;
  const showProvider = () => {
    const api = form.elements.provider.value === 'api';
    form.querySelector('[data-api-fields]').hidden = !api;
    form.querySelector('[data-cli-fields]').hidden = api;
    form.elements.base_url.required = api;
    form.elements.key_env.required = api;
  };
  const open = card => {
    form.reset();
    form.elements.id.value = card?.dataset.id || '';
    form.elements.name.value = card?.dataset.name || '';
    form.elements.model.value = card?.dataset.model || '';
    form.elements.provider.value = card?.dataset.provider || 'api';
    form.elements.base_url.value = card?.dataset.base || 'https://api.openai.com/v1';
    form.elements.key_env.value = card?.dataset.keyEnv || 'EASYREAD_API_KEY';
    form.elements.executable.value = card?.dataset.executable || '';
    form.elements.reasoning.value = card?.dataset.reasoning || '';
    showProvider();
    dialog.showModal();
    form.elements.name.focus();
  };
  document.getElementById('add-model')?.addEventListener('click', () => open(null));
  document.querySelectorAll('[data-edit-model]').forEach(button => button.addEventListener('click', () => {
    // Close before opening the modal so native focus restoration does not
    // send keyboard focus back to the menu behind the dialog.
    button.closest('[popover]').hidePopover();
    open(button.closest('.model-card'));
  }));
  dialog.querySelectorAll('[data-close-model]').forEach(button => button.addEventListener('click', () => dialog.close()));
  dialog.addEventListener('click', e => {if (e.target === dialog) dialog.close()});
  form.elements.provider.addEventListener('change', showProvider);
  document.querySelectorAll('[data-confirm-remove]').forEach(button => button.addEventListener('click', e => {
    if (!window.confirm(`Remove ${button.dataset.confirmRemove}?`)) e.preventDefault();
  }));
})();
