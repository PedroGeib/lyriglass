// `api` é exposto globalmente pelo preload (contextBridge); não redeclarar.
const $ = (id) => document.getElementById(id);
const $$ = (sel) => document.querySelectorAll(sel);

const ICONS = {
  play: 'M8 5.14v13.72a1 1 0 0 0 1.52.85l11-6.86a1 1 0 0 0 0-1.7l-11-6.86A1 1 0 0 0 8 5.14z',
  pause: 'M6.5 4h3a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1zm8 0h3a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-3a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z',
  prev: 'M6 6h2v12H6zm3.5 6 8.5 6V6z',
  next: 'M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z',
  shuffle: 'M10.59 9.17 5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.33 9.41-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z',
  repeat: 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
  repeatOne: 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zm-4-2V9h-1l-2 1v1h1.5v4H13z',
  heart: 'M16.5 3c-1.74 0-3.41.81-4.5 2.09C10.91 3.81 9.24 3 7.5 3 4.42 3 2 5.42 2 8.5c0 3.78 3.4 6.86 8.55 11.54L12 21.35l1.45-1.32C18.6 15.36 22 12.28 22 8.5 22 5.42 19.58 3 16.5 3zm-4.4 15.55-.1.1-.1-.1C7.14 14.24 4 11.39 4 8.5 4 6.5 5.5 5 7.5 5c1.54 0 3.04.99 3.57 2.36h1.87C13.46 5.99 14.96 5 16.5 5c2 0 3.5 1.5 3.5 3.5 0 2.89-3.14 5.74-7.9 10.05z',
  heartFilled: 'M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z',
  jam: 'M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z',
  dots: 'M6 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm12 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm-6 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4z',
  minus: 'M5 11h14v2H5z',
  note: 'M12 3v10.55A4 4 0 1 0 14 17V7h4V3h-6z',
  link: 'M3.9 12c0-1.71 1.39-3.1 3.1-3.1h4V7H7a5 5 0 0 0 0 10h4v-1.9H7c-1.71 0-3.1-1.39-3.1-3.1zM8 13h8v-2H8v2zm9-6h-4v1.9h4c1.71 0 3.1 1.39 3.1 3.1s-1.39 3.1-3.1 3.1h-4V17h4a5 5 0 0 0 0-10z',
  offline: 'M23.64 7c-.45-.34-4.93-4-11.64-4-1.5 0-2.89.19-4.15.48L18.18 13.8 23.64 7zm-6.6 8.22L3.27 1.44 2 2.72l2.05 2.06C1.91 5.76.59 6.82.36 7l11.63 14.49.01.01.01-.01 3.9-4.86 3.32 3.32 1.27-1.27-3.46-3.46z',
};
const svg = (name, cls = '') => `<svg class="${cls}" viewBox="0 0 24 24" aria-hidden="true"><path d="${ICONS[name]}"/></svg>`;
const DEFAULT_ACCENT = [30, 215, 96];

const card = $('card');
let cfg = {};
let P = { status: 'loading', track: null, progressMs: 0, at: Date.now() };
let L = { status: 'idle', lines: [] };
let lineEls = [];
let activeIdx = -1;
let freeScrollUntil = 0;
let raf = 0;
let lastFrame = 0;
let seeking = false;
let coverUrl = undefined;
let compact = null;
let jamOpen = false;
let jamStartHandled = false;
let pendingVolume = null;
let volumeTimer = 0;
let toastTimer = 0;
let emptyAction = null;

init();

async function init() {
  for (const el of $$('[data-icon]')) {
    const [a, b] = el.dataset.icon.split('|');
    el.innerHTML = b ? svg(a, 'a') + svg(b, 'b') : svg(a);
  }

  cfg = await api.getConfig();
  const state = await api.getState();
  card.classList.toggle('interactive', !state.clickThrough);
  applyConfig(null);
  setPlayer(state.player);
  setLyrics(state.lyrics);

  api.onConfig((c) => {
    const prev = cfg;
    cfg = c;
    applyConfig(prev);
  });
  api.onPlayer(setPlayer);
  api.onLyrics(setLyrics);
  api.onToast((msg) => toast(msg));
  api.onJamToggle(() => toggleJam());
  api.onClickThrough((on) => {
    card.classList.toggle('interactive', !on);
    toast(on ? 'Click-through ativado · Ctrl+Alt+S para desativar' : 'Click-through desativado');
  });

  bindEvents();
}

// ------------------------------------------------------------------ config
function applyConfig(prev) {
  const root = document.documentElement;
  root.style.zoom = cfg.scale;
  root.style.setProperty('--glass', cfg.opacity);
  root.style.setProperty('--lyrics-size', `${cfg.lyricsFontSize}px`);

  for (const layout of ['horizontal', 'vertical', 'mini']) card.classList.toggle(`layout-${layout}`, cfg.layout === layout);
  card.classList.toggle('karaoke', cfg.karaoke);
  card.classList.toggle('locked', cfg.lockPosition);
  card.classList.toggle('no-jam', !cfg.jamEnabled);

  if (!prev) return;
  if (prev.accentFromCover !== cfg.accentFromCover) {
    const url = coverUrl;
    coverUrl = undefined;
    setCover(url);
  }
  if (prev.showTranslation !== cfg.showTranslation || prev.showLyrics !== cfg.showLyrics) renderLyrics();
  if (prev.lyricsOffsetMs !== cfg.lyricsOffsetMs) activeIdx = -2;
  if (!cfg.jamEnabled && jamOpen) closeJam();
  if (jamOpen && prev.jamLink !== cfg.jamLink) refreshJam();
  if (prev.layout !== cfg.layout || prev.lyricsFontSize !== cfg.lyricsFontSize) {
    requestAnimationFrame(() => {
      updateMarquee();
      scrollToActive('instant');
    });
  }
  updateFooter();
  updateCompact();
  draw();
}

// ------------------------------------------------------------------ player
function setPlayer(s) {
  const prev = P;
  P = s;
  const t = s.track;
  card.dataset.status = s.status;

  if (t?.id !== prev.track?.id || t?.name !== prev.track?.name) {
    $('titleText').textContent = t?.name || '';
    $('artist').textContent = t ? t.artists.join(', ') : '';
    const meta = $('miniMeta');
    meta.textContent = '';
    if (t) {
      const b = document.createElement('b');
      b.textContent = t.name;
      meta.append(b, ` · ${t.artists.join(', ')}`);
    }
    $('tDur').textContent = fmt(t?.durationMs || 0);
    updateMarquee();
    if (jamOpen && !cfg.jamLink) refreshJam();
  }
  setCover(t ? t.cover : null);

  const playing = s.status === 'playing';
  for (const el of $$('[data-action="toggle"]')) {
    el.classList.toggle('alt', playing);
    el.title = playing ? 'Pausar' : 'Tocar';
  }
  for (const el of $$('.like')) {
    el.classList.toggle('alt', Boolean(s.liked));
    el.title = s.liked ? 'Remover das Músicas Curtidas' : 'Curtir';
  }
  card.classList.toggle('no-like', !t || t.type !== 'track' || t.isLocal);
  for (const el of $$('.shuffle')) el.classList.toggle('on', s.shuffle);
  for (const el of $$('.repeat')) {
    el.classList.toggle('on', s.repeat !== 'off');
    el.classList.toggle('alt', s.repeat === 'track');
  }

  updateEmpty();
  updateFooter();
  updateCompact();

  if (playing) startLoop();
  else {
    stopLoop();
    draw();
  }

  if (!jamStartHandled && t) {
    jamStartHandled = true;
    if (cfg.jamEnabled && cfg.jamOpenOnStart) openJam();
  }
}

function progressNow() {
  const t = P.track;
  if (!t) return 0;
  const p = P.status === 'playing' ? P.progressMs + (Date.now() - P.at) : P.progressMs;
  return Math.max(0, Math.min(p, t.durationMs));
}

function setCover(url) {
  if (url === coverUrl) return;
  coverUrl = url;
  const art = $('art');
  const bg = $('cardBg');
  if (!url) {
    art.style.backgroundImage = '';
    art.classList.add('placeholder');
    bg.style.backgroundImage = '';
    setAccent(DEFAULT_ACCENT);
    return;
  }
  const cssUrl = `url("${url.replace(/["\\]/g, '')}")`;
  const img = new Image();
  img.crossOrigin = 'anonymous';
  img.onload = () => {
    if (coverUrl !== url) return;
    art.style.backgroundImage = cssUrl;
    art.classList.remove('placeholder');
    bg.style.backgroundImage = cssUrl;
    setAccent(cfg.accentFromCover ? extractAccent(img) : DEFAULT_ACCENT);
  };
  img.onerror = () => {
    if (coverUrl !== url) return;
    art.style.backgroundImage = cssUrl;
    art.classList.remove('placeholder');
    bg.style.backgroundImage = cssUrl;
    setAccent(DEFAULT_ACCENT);
  };
  img.src = url;
}

function setAccent([r, g, b]) {
  document.documentElement.style.setProperty('--accent', `${r} ${g} ${b}`);
}

// Escolhe a cor mais "vibrante" da capa agrupando pixels por matiz.
function extractAccent(img) {
  try {
    const size = 32;
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = size;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    ctx.drawImage(img, 0, 0, size, size);
    const data = ctx.getImageData(0, 0, size, size).data;
    const buckets = Array.from({ length: 12 }, () => ({ w: 0, r: 0, g: 0, b: 0 }));
    for (let i = 0; i < data.length; i += 4) {
      const [h, s, l] = rgbToHsl(data[i], data[i + 1], data[i + 2]);
      if (l < 0.12 || l > 0.92) continue;
      const w = s * s * (1 - Math.abs(l - 0.5));
      const bucket = buckets[Math.floor(h * 12) % 12];
      bucket.w += w;
      bucket.r += data[i] * w;
      bucket.g += data[i + 1] * w;
      bucket.b += data[i + 2] * w;
    }
    const top = buckets.reduce((a, b) => (b.w > a.w ? b : a));
    if (top.w < 3) return [214, 218, 228];
    let [h, s, l] = rgbToHsl(top.r / top.w, top.g / top.w, top.b / top.w);
    s = Math.max(s, 0.5);
    l = Math.min(Math.max(l, 0.58), 0.7);
    return hslToRgb(h, s, l);
  } catch {
    return DEFAULT_ACCENT;
  }
}

// ------------------------------------------------------------------ lyrics
function setLyrics(l) {
  L = l;
  renderLyrics();
  updateFooter();
  updateCompact();
}

function renderLyrics() {
  const inner = $('lyricsInner');
  const status = $('lyricsStatus');
  inner.textContent = '';
  status.textContent = '';
  status.className = 'lyrics-status';
  lineEls = [];
  activeIdx = -1;
  const hasLines = L.status === 'ready' && L.lines.length > 0;
  $('lyrics').classList.toggle('unsynced', hasLines && !L.synced);

  if (!cfg.showLyrics || !P.track) return;
  if (L.status === 'loading') {
    status.classList.add('loading');
    status.innerHTML = '<i></i><i></i><i></i>';
    return;
  }
  if (L.status !== 'ready') return;
  if (!hasLines) {
    status.textContent = L.episode ? 'Episódio de podcast'
      : L.instrumental ? '♪ Faixa instrumental'
      : L.error ? 'Não foi possível buscar a letra'
      : cfg.layout === 'mini' ? (P.track.album || 'Letra não encontrada')
      : 'Letra não encontrada';
    return;
  }
  if (!L.synced && cfg.layout === 'mini') {
    status.textContent = 'Letra disponível só sem sincronia';
    return;
  }

  const tr = cfg.showTranslation && L.translation && !L.translation.same ? L.translation.lines : null;
  const frag = document.createDocumentFragment();
  L.lines.forEach((line, i) => {
    const el = document.createElement('div');
    el.className = 'line';
    el.dataset.i = i;
    if (!line.text || /^[♪♫\s]+$/.test(line.text)) {
      el.classList.add('gap');
      el.innerHTML = '<span class="dots"><i></i><i></i><i></i></span>';
    } else {
      const txt = document.createElement('span');
      txt.className = 'txt';
      txt.textContent = line.text;
      el.append(txt);
      const translated = tr?.[i];
      if (translated && translated.toLowerCase() !== line.text.toLowerCase()) {
        const t = document.createElement('div');
        t.className = 'tr';
        t.textContent = translated;
        el.append(t);
      }
    }
    frag.append(el);
    lineEls.push(el);
  });
  inner.append(frag);

  if (L.synced) draw(true);
  else $('lyricsScroll').scrollTop = 0;
}

function findLine(t) {
  const lines = L.lines;
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].t <= t) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

function draw(forceScroll = false) {
  const pos = progressNow();
  const dur = P.track?.durationMs || 0;
  if (!seeking) {
    const pct = dur ? `${(pos / dur) * 100}%` : '0%';
    for (const f of $$('.bar .fill, .edge .fill')) f.style.width = pct;
    $('tCur').textContent = fmt(pos);
  }

  if (!L.synced || !lineEls.length) return;
  const t = pos + cfg.lyricsOffsetMs;
  const idx = findLine(t);

  if (idx !== activeIdx) {
    lineEls.forEach((el, i) => {
      el.classList.toggle('active', i === idx);
      el.classList.toggle('past', i < idx);
    });
    const jumped = activeIdx === -2 || Math.abs(idx - activeIdx) > 2;
    activeIdx = idx;
    scrollToActive(forceScroll || jumped ? 'instant' : 'smooth');
  } else if (forceScroll) {
    scrollToActive('instant');
  }

  if (cfg.karaoke && idx >= 0) {
    const line = L.lines[idx];
    const nextT = L.lines[idx + 1]?.t ?? line.t + 5000;
    const span = Math.max(600, Math.min(nextT - line.t, Math.max(1500, line.text.length * 95)));
    lineEls[idx].style.setProperty('--p', Math.min(1, Math.max(0, (t - line.t) / span)).toFixed(3));
  }

  if (freeScrollUntil && Date.now() > freeScrollUntil) {
    freeScrollUntil = 0;
    scrollToActive('smooth');
  }
}

function scrollToActive(behavior) {
  if (Date.now() < freeScrollUntil || !L.synced) return;
  const scroller = $('lyricsScroll');
  const el = lineEls[activeIdx];
  const top = el ? el.offsetTop - scroller.clientHeight / 2 + el.offsetHeight / 2 : 0;
  scroller.scrollTo({ top, behavior });
}

function startLoop() {
  if (raf) return;
  const tick = (now) => {
    raf = requestAnimationFrame(tick);
    if (now - lastFrame < 33) return;
    lastFrame = now;
    draw();
  };
  raf = requestAnimationFrame(tick);
}

function stopLoop() {
  cancelAnimationFrame(raf);
  raf = 0;
}

// ------------------------------------------------------------- ui states
function updateEmpty() {
  const s = P.status;
  let view = null;
  if (s === 'auth') {
    view = { icon: 'link', title: 'Conecte o Spotify', sub: 'Faça login para ver o que está tocando.', action: ['Conectar', () => api.openSettings('account')] };
  } else if (!P.track) {
    if (s === 'idle') view = { icon: 'note', title: 'Nada tocando', sub: 'Dê play no Spotify em qualquer dispositivo.' };
    else if (s === 'error') view = { icon: 'offline', title: 'Sem conexão', sub: P.message || 'Tentando novamente…' };
    else view = { icon: 'note', title: 'Carregando…', sub: '' };
  }

  $('empty').hidden = !view;
  if (!view) return;
  $('emptyIcon').innerHTML = svg(view.icon);
  $('emptyTitle').textContent = view.title;
  $('emptySub').textContent = view.sub;
  const btn = $('emptyAction');
  btn.hidden = !view.action;
  if (view.action) {
    btn.textContent = view.action[0];
    emptyAction = view.action[1];
  }
}

function updateCompact() {
  let value;
  if (cfg.layout === 'mini') value = false;
  else if (!P.track) value = true;
  else if (!cfg.showLyrics) value = true;
  else if (L.status !== 'ready' || L.trackId !== P.track.id) value = compact ?? false;
  else value = cfg.collapseWhenNoLyrics && L.lines.length === 0;

  card.classList.toggle('compact', value);
  if (value !== compact) {
    compact = value;
    api.setCompact(value);
    requestAnimationFrame(updateMarquee);
  }
}

function updateFooter() {
  const next = $('nextUp');
  next.textContent = '';
  if (cfg.showNextUp && P.nextUp && P.track) {
    const b = document.createElement('b');
    b.textContent = P.nextUp.name;
    next.append('A seguir: ', b, ` · ${P.nextUp.artists.join(', ')}`);
  }
  const tags = [];
  if (L.status === 'ready' && L.lines.length && !L.synced) tags.push('sem sincronia');
  if (L.synced && cfg.lyricsOffsetMs) tags.push(`${cfg.lyricsOffsetMs > 0 ? '+' : ''}${cfg.lyricsOffsetMs} ms`);
  $('lyricsTag').textContent = tags.join(' · ');

  const hasFoot = Boolean(next.textContent || tags.length);
  if ($('lyrics').classList.contains('has-foot') !== hasFoot) {
    $('lyrics').classList.toggle('has-foot', hasFoot);
    requestAnimationFrame(() => scrollToActive('instant'));
  }
}

function updateMarquee() {
  const box = $('title');
  const span = $('titleText');
  box.classList.remove('marquee');
  const overflow = span.offsetWidth - box.clientWidth;
  if (overflow > 2) {
    box.style.setProperty('--shift', `-${overflow + 6}px`);
    box.style.setProperty('--dur', `${Math.max(5, overflow / 14)}s`);
    box.classList.add('marquee');
  }
}

function toast(message, ms = 2400) {
  const el = $('toast');
  el.textContent = message;
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), ms);
}

// --------------------------------------------------------------------- jam
function jamUrl() {
  return (cfg.jamLink || '').trim() || P.track?.url || 'https://open.spotify.com';
}

async function refreshJam() {
  const hasLink = Boolean((cfg.jamLink || '').trim());
  $('jamTitle').textContent = hasLink ? 'Entre na minha Jam' : 'Ouça comigo';
  $('jamSub').textContent = hasLink
    ? 'Aponte a câmera do celular para o QR Code.'
    : P.track ? `${P.track.name} — ${P.track.artists.join(', ')}` : 'Abra o Spotify pelo QR Code.';
  $('jamQr').src = await api.jamQr(jamUrl());
}

function openJam() {
  if (!cfg.jamEnabled) return;
  jamOpen = true;
  refreshJam();
  $('jam').hidden = false;
}

function closeJam() {
  jamOpen = false;
  $('jam').hidden = true;
}

function toggleJam() {
  if (jamOpen) closeJam();
  else openJam();
}

// ------------------------------------------------------------------ events
function bindEvents() {
  document.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-action]');
    if (btn) {
      handleAction(btn.dataset.action);
      return;
    }
    const line = e.target.closest('.line');
    if (line && L.synced) {
      const target = L.lines[Number(line.dataset.i)];
      freeScrollUntil = 0;
      api.command('seek', Math.max(0, target.t - cfg.lyricsOffsetMs + 30));
    }
  });

  $('emptyAction').addEventListener('click', () => emptyAction?.());

  document.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    api.showMenu();
  });

  const bar = $('bar');
  const ratio = (e) => {
    const r = bar.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  };
  const preview = (e) => {
    const k = ratio(e);
    for (const f of $$('.bar .fill, .edge .fill')) f.style.width = `${k * 100}%`;
    $('tCur').textContent = fmt(k * P.track.durationMs);
  };
  bar.addEventListener('pointerdown', (e) => {
    if (!P.track) return;
    seeking = true;
    bar.classList.add('seeking');
    bar.setPointerCapture(e.pointerId);
    preview(e);
  });
  bar.addEventListener('pointermove', (e) => { if (seeking) preview(e); });
  bar.addEventListener('pointerup', (e) => {
    if (!seeking) return;
    seeking = false;
    bar.classList.remove('seeking');
    freeScrollUntil = 0;
    api.command('seek', ratio(e) * P.track.durationMs);
  });

  // Roda do mouse sobre a capa ajusta o volume.
  $('player').addEventListener('wheel', (e) => {
    e.preventDefault();
    if (P.volume == null) return;
    const v = Math.min(100, Math.max(0, (pendingVolume ?? P.volume) + (e.deltaY < 0 ? 5 : -5)));
    pendingVolume = v;
    toast(`Volume ${v}%`, 900);
    clearTimeout(volumeTimer);
    volumeTimer = setTimeout(() => {
      api.command('volume', v);
      pendingVolume = null;
    }, 250);
  }, { passive: false });

  // Rolagem manual pausa o acompanhamento automático por alguns segundos.
  $('lyricsScroll').addEventListener('wheel', () => {
    if (L.synced) freeScrollUntil = Date.now() + 4000;
  }, { passive: true });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && jamOpen) closeJam();
  });
}

function handleAction(action) {
  switch (action) {
    case 'toggle':
    case 'next':
    case 'prev':
    case 'shuffle':
    case 'repeat':
    case 'like':
      api.command(action);
      break;
    case 'menu':
      api.showMenu();
      break;
    case 'hide':
      api.hideOverlay();
      break;
    case 'jam':
      toggleJam();
      break;
    case 'jamCopy':
      api.copy(jamUrl()).then(() => toast('Link copiado!'));
      break;
  }
}

// ----------------------------------------------------------------- helpers
function fmt(ms) {
  const total = Math.floor(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}

function rgbToHsl(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h / 6, s, l];
}

function hslToRgb(h, s, l) {
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue = (t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return [hue(h + 1 / 3), hue(h), hue(h - 1 / 3)].map((v) => Math.round(v * 255));
}
