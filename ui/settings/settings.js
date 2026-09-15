// `api` é criado por ../api.js (ponte com o backend Tauri); não redeclarar.
const $ = (id) => document.getElementById(id);
const $$ = (sel) => document.querySelectorAll(sel);

let cfg = {};
const timers = {};
let snackTimer = 0;

init();

async function init() {
  cfg = await api.getConfig();
  fill();
  // Without a Client ID there is no way to log in, so the guide starts open.
  $('clientIdGuide').open = !cfg.spotifyClientId;

  const initialTab = window.__LYRIGLASS_TAB__ || 'account';
  selectTab(initialTab);

  renderAuth(await api.getAuth());
  renderKeyStatus(await api.hasGeminiKey());
  renderInfo(await api.appInfo());
  refreshJamPreview();

  api.onConfig((c) => {
    const jamChanged = c.jamLink !== cfg.jamLink;
    cfg = c;
    fill();
    if (jamChanged) refreshJamPreview();
  });
  api.onAuth(renderAuth);
  api.onPlayer(() => { if (!cfg.jamLink) refreshJamPreview(); });
  api.onSettingsTab(selectTab);

  bindEvents();
}

// ----------------------------------------------------------------- binding
function fill() {
  for (const el of $$('[data-key]')) {
    const value = cfg[el.dataset.key];
    if (el.type === 'checkbox') el.checked = Boolean(value);
    else if (el.type === 'radio') el.checked = String(value) === el.value;
    else if (el !== document.activeElement) el.value = value ?? '';
    formatOutput(el);
  }
  refreshDependencies();
}

function readValue(el) {
  if (el.type === 'checkbox') return el.checked;
  if (el.type === 'range' || el.type === 'number') return Number(el.value);
  return el.value.trim();
}

function formatOutput(el) {
  const out = document.querySelector(`output[data-for="${el.dataset.key}"]`);
  if (!out) return;
  const v = Number(el.value);
  const formats = {
    pct: () => `${Math.round(v * 100)}%`,
    px: () => `${v} px`,
    min: () => `${v} min`,
    ms: () => `${v > 0 ? '+' : ''}${v} ms`,
  };
  out.textContent = (formats[out.dataset.format] || (() => v))();
}

function refreshDependencies() {
  const test = (expr) => {
    const m = expr.match(/^(\w+)(!=|=)(.*)$/);
    if (!m) return Boolean(cfg[expr]);
    return m[2] === '=' ? String(cfg[m[1]]) === m[3] : String(cfg[m[1]]) !== m[3];
  };
  for (const el of $$('[data-show-if]')) el.hidden = !test(el.dataset.showIf);
  for (const el of $$('[data-enabled-if]')) {
    const on = test(el.dataset.enabledIf);
    el.classList.toggle('disabled', !on);
    el.inert = !on;
  }
}

function setKey(key, value, delay) {
  cfg[key] = value;
  refreshDependencies();
  clearTimeout(timers[key]);
  timers[key] = setTimeout(() => api.setConfig({ [key]: value }), delay);
}

// ------------------------------------------------------------------- views
function selectTab(tab) {
  if (!document.querySelector(`[data-panel="${tab}"]`)) tab = 'account';
  for (const el of $$('.tab')) el.classList.toggle('active', el.dataset.tab === tab);
  for (const el of $$('.panel')) el.classList.toggle('active', el.dataset.panel === tab);
  document.querySelector('.content').scrollTop = 0;
}

function renderAuth(auth) {
  const name = auth.profile?.name;
  const avatar = $('avatar');
  avatar.textContent = auth.loggedIn && name ? name.trim().charAt(0).toUpperCase() : '?';
  avatar.classList.toggle('on', auth.loggedIn);
  if (auth.loggedIn && auth.profile?.image) {
    const img = document.createElement('img');
    img.alt = '';
    img.referrerPolicy = 'no-referrer';
    img.onerror = () => img.remove();
    img.src = auth.profile.image;
    avatar.append(img);
  }
  $('accName').textContent = auth.loggedIn ? name || 'Conta conectada' : 'Não conectado';
  $('accSub').textContent = auth.demo
    ? 'Rodando com dados de exemplo'
    : auth.loggedIn
      ? auth.profile?.product === 'premium' ? 'Spotify Premium · controles liberados' : 'Conectado ao Spotify'
      : 'Faça login para começar';
  $('btnLogin').hidden = auth.loggedIn;
  $('btnLogout').hidden = !auth.loggedIn || auth.demo;
  if (auth.redirectUri) $('redirectUri').textContent = auth.redirectUri;
}

function renderKeyStatus(has) {
  $('keyStatus').textContent = has ? '✓ Chave salva com segurança neste computador.' : 'Nenhuma chave salva.';
  $('btnClearKey').hidden = !has;
}

function renderInfo(info) {
  $('version').textContent = `v${info.version}${info.demo ? ' · demo' : ''}`;
  $('aboutVersion').textContent = info.version;
  $('dataPath').textContent = info.dataPath;

  const box = $('hotkeys');
  box.textContent = '';
  for (const hk of info.hotkeys) {
    const row = document.createElement('div');
    row.className = 'kbd-row';
    const label = document.createElement('div');
    label.className = 'grow';
    label.textContent = hk.label;
    row.append(label);
    if (!hk.ok) {
      const fail = document.createElement('span');
      fail.className = 'badge-fail';
      fail.textContent = 'em uso por outro app';
      row.append(fail);
    }
    const keys = document.createElement('span');
    keys.className = 'keys';
    hk.accel.replace('CommandOrControl', 'Ctrl').split('+').forEach((k, i) => {
      if (i) keys.append('+');
      const kbd = document.createElement('kbd');
      kbd.textContent = k;
      keys.append(kbd);
    });
    row.append(keys);
    box.append(row);
  }
}

async function refreshJamPreview() {
  const link = (cfg.jamLink || '').trim();
  const state = await api.getState();
  const track = state.player.track;
  const target = link || track?.url || 'https://open.spotify.com';
  $('jamPreview').src = await api.jamQr(target);
  $('jamPreviewText').textContent = link
    ? `Leva para: ${link}`
    : track ? `Sem link de Jam — abre “${track.name}”.` : 'Sem link de Jam — abre o Spotify.';
}

function showClientIdGuide() {
  const guide = $('clientIdGuide');
  guide.open = true;
  guide.scrollIntoView({ behavior: 'smooth', block: 'start' });
  $('clientId').focus({ preventScroll: true });
}

function snack(message) {
  const el = $('snackbar');
  el.textContent = message;
  el.classList.add('show');
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => el.classList.remove('show'), 2200);
}

// ------------------------------------------------------------------ events
function bindEvents() {
  document.addEventListener('input', (e) => {
    const el = e.target.closest('[data-key]');
    if (!el) return;
    const isText = ['text', 'url', 'password'].includes(el.type);
    setKey(el.dataset.key, readValue(el), isText ? 500 : el.type === 'range' ? 60 : 0);
    formatOutput(el);
  });

  for (const tab of $$('.tab')) tab.addEventListener('click', () => selectTab(tab.dataset.tab));

  document.addEventListener('click', (e) => {
    const ext = e.target.closest('[data-external]');
    if (ext) {
      e.preventDefault();
      api.openExternal(ext.href);
      return;
    }
    const copy = e.target.closest('[data-copy]');
    if (copy) {
      api.copy($(copy.dataset.copy).textContent).then(() => snack('Copiado!'));
      return;
    }
    const preset = e.target.closest('[data-preset]');
    if (preset) api.movePreset(preset.dataset.preset);
  });

  $('btnLogin').addEventListener('click', async () => {
    const btn = $('btnLogin');
    btn.disabled = true;
    btn.textContent = 'Aguardando o navegador…';
    $('btnCancelLogin').hidden = false;
    $('loginHint').hidden = false;
    $('authMessage').hidden = true;
    const result = await api.login();
    btn.disabled = false;
    btn.textContent = 'Conectar com Spotify';
    $('btnCancelLogin').hidden = true;
    $('loginHint').hidden = true;
    if (result.ok) snack(`Conectado como ${result.profile.name}`);
    else if (result.error !== 'Login cancelado.') {
      $('authMessage').textContent = result.error;
      $('authMessage').hidden = false;
      if (/client id|client_id/i.test(result.error)) showClientIdGuide();
    }
    renderAuth(await api.getAuth());
  });

  $('btnCancelLogin').addEventListener('click', () => api.cancelLogin());

  $('btnLogout').addEventListener('click', async () => {
    renderAuth(await api.logout());
    snack('Conta desconectada');
  });

  $('btnSaveKey').addEventListener('click', async () => {
    const key = $('geminiKey').value.trim();
    if (!key) return;
    renderKeyStatus(await api.setGeminiKey(key));
    $('geminiKey').value = '';
    snack('Chave do Gemini salva');
  });

  $('btnClearKey').addEventListener('click', async () => {
    renderKeyStatus(await api.setGeminiKey(''));
    snack('Chave removida');
  });

  $('btnResetOffset').addEventListener('click', () => {
    api.setConfig({ lyricsOffsetMs: 0 });
  });

  $('btnClearCache').addEventListener('click', async () => {
    const count = await api.clearLyricsCache();
    snack(`${count} ${count === 1 ? 'letra removida' : 'letras removidas'} do cache`);
  });

  $('btnOpenData').addEventListener('click', () => api.openDataFolder());
}
