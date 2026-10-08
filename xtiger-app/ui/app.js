'use strict';

/* The xTiger app front end. One page; each screen is a render function that returns HTML and a
 * bind function that wires its events. Everything the Rust side does goes through `call`. */

let tauri = window.__TAURI__;
const call = (cmd, args) => tauri.core.invoke(cmd, args);

const SEVERITIES = ['fatal', 'error', 'warning', 'untidy', 'tips'];
const SEVERITY_LABELS = { fatal: 'Fatal', error: 'Errors', warning: 'Warnings', untidy: 'Untidy', tips: 'Tips' };
const TINTS = ['#2B5BB8', '#7A4A1E', '#2E6B52', '#6A3A8C', '#8C3A3A', '#3A5E8C', '#5E6B2E', '#2E5E6B'];
const PAGE_SIZE = 300;
const TIPS = [
  'Double-click a report to jump to its line.',
  'Turn on “Only new” to see what changed since the last run.',
  'Reports with weak confidence can be false alarms. Check them, but do not panic.',
  'Use ↑ and ↓ to move through the reports, and Enter to open one.',
  'Group the reports by key to fix one kind of mistake everywhere at once.',
];

const state = {
  screen: null,
  setup: null,
  mods: [],
  modsLoaded: false,
  source: 'all',
  search: '',
  selectedMod: null,
  pictures: new Map(),
  run: null,
  validating: null,
  view: { severities: new Set(SEVERITIES), onlyNew: false, groupBy: 'file', text: '', selected: 0, collapsed: new Set(), limit: PAGE_SIZE },
  notice: null,
};

/* ---------- Small helpers ---------- */

const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

function esc(value) {
  return String(value ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

function plural(count, one, many = `${one}s`) {
  return `${count.toLocaleString()} ${count === 1 ? one : many}`;
}

function initials(name) {
  const words = name.replace(/[^\p{L}\p{N} ]/gu, ' ').split(/\s+/).filter(Boolean);
  if (words.length === 0) return '?';
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

function tint(name) {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.codePointAt(0)) >>> 0;
  return TINTS[hash % TINTS.length];
}

function formatDuration(ms) {
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)} s`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds} s`;
  return `${Math.floor(seconds / 60)} min ${String(seconds % 60).padStart(2, '0')} s`;
}

function clock(ms) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

function ago(ms) {
  const minutes = Math.round((Date.now() - ms) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${plural(minutes, 'minute')} ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${plural(hours, 'hour')} ago`;
  return `${plural(Math.round(hours / 24), 'day')} ago`;
}

function shortPath(path) {
  return String(path ?? '').replaceAll('/', '\\');
}

function fileName(path) {
  return String(path ?? '').split(/[\\/]/).pop();
}

function toast(text) {
  const element = $('#toast');
  element.textContent = text;
  element.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => element.classList.remove('show'), 1800);
}

async function copy(text, what) {
  try {
    await navigator.clipboard.writeText(text);
    toast(`${what} copied`);
  } catch {
    toast('Could not copy');
  }
}

async function pickFolder(title) {
  const picked = await tauri.dialog.open({ directory: true, multiple: false, title });
  return Array.isArray(picked) ? picked[0] : picked;
}

const icon = {
  check: '<svg class="status-icon" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="var(--good)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>',
  warn: '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--sev-warning)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l9.5 17h-19z"/><path d="M12 10v4"/><path d="M12 17.5v.5"/></svg>',
  search: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5L21 21"/></svg>',
  magnifier: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.5 15.5L21 21"/></svg>',
  again: '<svg class="icon-spin" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/></svg>',
  open: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/></svg>',
  plus: '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14"/><path d="M5 12h14"/></svg>',
  close: '<svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.6"/></svg>',
  chevron: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>',
  spark: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5L18 18M18 6l-2.5 2.5M8.5 15.5L6 18"/></svg>',
  folder: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>',
};

/* ---------- Animation helpers ---------- */

/* Give each child of every .stagger its place in the queue, so CSS can delay it. */
function stagger(root) {
  for (const list of $$('.stagger', root)) {
    [...list.children].forEach((child, i) => child.style.setProperty('--i', Math.min(i, 16)));
  }
}

/* Numbers marked with data-count roll up from zero. */
function countUp(root) {
  for (const element of $$('[data-count]', root)) {
    const target = Number(element.dataset.count);
    const prefix = element.dataset.prefix ?? '';
    if (reducedMotion() || !Number.isFinite(target) || target === 0) {
      element.textContent = prefix + target.toLocaleString();
      continue;
    }
    const start = performance.now();
    const duration = Math.min(1200, 400 + Math.abs(target) * 8);
    const step = (now) => {
      // A frame's timestamp is when the frame began, which can be a little before `start`.
      const t = Math.min(1, Math.max(0, (now - start) / duration));
      const eased = 1 - (1 - t) ** 3;
      element.textContent = prefix + Math.round(target * eased).toLocaleString();
      if (t < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }
}

/* Type the text of .typed elements one letter at a time. */
function typeOut(root) {
  for (const element of $$('.typed', root)) {
    const text = element.dataset.text ?? '';
    if (reducedMotion()) {
      element.textContent = text;
      element.classList.add('done');
      continue;
    }
    let shown = 0;
    element.textContent = '';
    setTimeout(function tick() {
      if (!element.isConnected) return;
      shown += 1;
      element.textContent = text.slice(0, shown);
      if (shown < text.length) setTimeout(tick, text[shown - 1] === '.' ? 220 : 22);
      else element.classList.add('done');
    }, 650);
  }
}

/* Pixel confetti around an element. */
function sparkle(target, count = 26) {
  if (reducedMotion() || !target) return;
  const colors = ['var(--good)', 'var(--accent)', 'var(--sev-warning)', 'var(--accent-text)', '#FFFFFF'];
  for (let i = 0; i < count; i += 1) {
    const spark = document.createElement('span');
    const angle = (i / count) * Math.PI * 2 + Math.random() * 0.4;
    const distance = 110 + Math.random() * 90;
    spark.className = 'spark';
    spark.style.setProperty('--dx', `${Math.cos(angle) * distance}px`);
    spark.style.setProperty('--dy', `${Math.sin(angle) * distance}px`);
    spark.style.setProperty('--r', `${Math.round(Math.random() * 180)}deg`);
    spark.style.setProperty('--c', colors[i % colors.length]);
    spark.style.animationDelay = `${Math.random() * 120}ms`;
    const size = 6 + Math.round(Math.random() * 3) * 2;
    spark.style.width = spark.style.height = `${size}px`;
    target.append(spark);
    spark.addEventListener('animationend', () => spark.remove());
  }
}

function placeTabPill(root) {
  const pill = $('.tab-pill', root);
  const tab = $('.tab[aria-selected="true"]', root);
  if (!pill || !tab) return;
  pill.style.left = `${tab.offsetLeft}px`;
  pill.style.width = `${tab.offsetWidth}px`;
}

/* ---------- Navigation ---------- */

const SCREENS = {};
const RAIL_SCREENS = new Set(['mods', 'results', 'allclear', 'activity', 'settings']);

// The screen the latest go() asked for. Swaps can run late when quick clicks cut transitions short,
// so each swap renders the latest target rather than its own.
let target = null;

function go(screen, { animate = true } = {}) {
  const changed = (target ?? state.screen) !== screen;
  target = screen;
  const swap = () => {
    state.screen = target;
    render();
  };
  if (animate && changed) transition(swap);
  else swap();
}

/* Run a DOM change as a view transition when the browser can and the user wants motion. */
function transition(change) {
  if (!document.startViewTransition || reducedMotion() || document.hidden) {
    change();
    return;
  }
  const running = document.startViewTransition(change);
  // A transition that is cut short by the next one still applies its change; only the animation
  // is skipped.
  running.ready.catch(() => {});
  running.finished.catch(() => {});
  running.updateCallbackDone.catch(() => {});
}

function render() {
  const { html, bind } = SCREENS[state.screen];
  const root = $('#screen');
  root.innerHTML = html();
  const rail = $('#rail');
  rail.hidden = !RAIL_SCREENS.has(state.screen);
  const current = state.screen === 'allclear' ? 'results' : state.screen;
  for (const button of $$('.rail-btn', rail)) {
    if (button.dataset.nav === current) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  }
  const resultsButton = $('[data-nav="results"]', rail);
  $('.badge-dot', resultsButton)?.remove();
  if (state.run?.newCount > 0 && current !== 'results') resultsButton.insertAdjacentHTML('beforeend', '<span class="badge-dot"></span>');
  stagger(root);
  bind?.(root);
  countUp(root);
}

/* ---------- Theme ---------- */

const lightQuery = window.matchMedia('(prefers-color-scheme: light)');

function applyTheme() {
  const choice = state.setup?.theme ?? 'system';
  const theme = choice === 'system' ? (lightQuery.matches ? 'light' : 'dark') : choice;
  document.documentElement.dataset.theme = theme;
}
lightQuery.addEventListener('change', applyTheme);

/* ---------- Mods ---------- */

async function loadMods() {
  state.mods = await call('list_mods');
  state.modsLoaded = true;
  const known = state.mods.some((mod) => mod.modFile === state.selectedMod);
  if (!known) {
    const last = state.mods.find((mod) => mod.modFile === state.setup?.lastMod);
    state.selectedMod = (last ?? state.mods[0])?.modFile ?? null;
  }
}

function selectedMod() {
  return state.mods.find((mod) => mod.modFile === state.selectedMod) ?? null;
}

function modStatus(mod) {
  if (mod.lastCount == null) return { text: 'Not checked yet', color: 'var(--muted)' };
  if (mod.lastCount === 0) return { text: 'No reports', color: 'var(--good)' };
  return { text: plural(mod.lastCount, 'report'), color: 'var(--sev-warning)' };
}

function loadPictures(root) {
  for (const thumb of $$('.thumb[data-picture]', root)) {
    const path = thumb.dataset.picture;
    const show = (url) => {
      if (!url || !thumb.isConnected) return;
      const img = document.createElement('img');
      img.alt = '';
      img.src = url;
      thumb.append(img);
      $('.initials', thumb)?.remove();
    };
    if (state.pictures.has(path)) {
      show(state.pictures.get(path));
    } else {
      call('mod_picture', { path }).then((url) => {
        state.pictures.set(path, url);
        show(url);
      });
    }
  }
}

/* Show the cards that match the source tab and the search, with a short entrance for the ones that
 * come back. */
function filterCards(root) {
  const needle = state.search.trim().toLowerCase();
  let shown = 0;
  for (const card of $$('.mod-card', root)) {
    const source = card.dataset.source;
    const match = (state.source === 'all' || (state.source === 'workshop') === (source === 'workshop'))
      && (!needle || card.dataset.name.includes(needle));
    if (match && card.hidden) {
      card.style.animation = 'none';
      void card.offsetWidth;
      card.style.animation = `rise 0.35s ${Math.min(shown, 12) * 30}ms var(--ease-out) both`;
    }
    card.hidden = !match;
    if (match) shown += 1;
  }
  const empty = $('.empty', root);
  if (empty) {
    empty.hidden = shown > 0 || state.mods.length === 0;
    $('span', empty).textContent = needle ? `No mod matches “${state.search}”.` : 'No mods here.';
  }
}

/* ---------- Validation ---------- */

async function validate(mod) {
  if (!mod) return;
  state.selectedMod = mod.modFile;
  const estimate = await call('last_duration', { modFile: mod.modFile });
  state.validating = { mod, started: Date.now(), estimate, log: [], error: null, tip: 0 };
  go('validating');
  try {
    const result = await call('validate', { modFile: mod.modFile });
    state.run = { ...result, mod, finished: Date.now() };
    state.view = { ...state.view, selected: 0, collapsed: new Set(), limit: PAGE_SIZE, onlyNew: false };
    state.setup.lastMod = mod.modFile;
    mod.lastCount = result.reports.length;
    mod.lastRun = Date.now();
    go(result.reports.length > 0 ? 'results' : 'allclear');
  } catch (error) {
    if (state.validating?.cancelled) return;
    state.validating.error = String(error);
    if (state.screen === 'validating') render();
  }
}

/* ---------- Reports ---------- */

function severityOf(report) {
  return SEVERITIES.includes(report.severity) ? report.severity : 'tips';
}

function where(report) {
  return report.locations?.[0] ?? null;
}

function reportText(report) {
  const lines = [`${report.severity}(${report.key}): ${report.message}`];
  for (const loc of report.locations ?? []) {
    lines.push(`  --> [${loc.from}] ${loc.path}:${loc.linenr ?? ''}:${loc.column ?? ''}${loc.tag ? ` (${loc.tag})` : ''}`);
    if (loc.line) lines.push(`      ${loc.line}`);
  }
  if (report.info) lines.push(`  = Info: ${report.info}`);
  if (report.wiki) lines.push(`  = Wiki: ${report.wiki}`);
  return lines.join('\n');
}

function visibleReports() {
  const { severities, onlyNew, text } = state.view;
  const needle = text.trim().toLowerCase();
  return state.run.reports.filter((report) => {
    if (!severities.has(severityOf(report))) return false;
    if (onlyNew && !report.isNew) return false;
    if (!needle) return true;
    const loc = where(report);
    return [report.message, report.key, report.info, loc?.path].some((field) => field?.toLowerCase().includes(needle));
  });
}

function groupReports(reports) {
  const by = state.view.groupBy;
  const groups = new Map();
  for (const report of reports) {
    let label;
    if (by === 'key') label = report.key;
    else if (by === 'severity') label = SEVERITY_LABELS[severityOf(report)];
    else label = where(report)?.path ?? '(no file)';
    if (!groups.has(label)) groups.set(label, []);
    groups.get(label).push(report);
  }
  const rank = (report) => SEVERITIES.indexOf(severityOf(report));
  const entries = [...groups.entries()];
  for (const [, items] of entries) {
    items.sort((a, b) => rank(a) - rank(b) || (where(a)?.linenr ?? 0) - (where(b)?.linenr ?? 0));
  }
  if (by === 'severity') entries.sort((a, b) => rank(a[1][0]) - rank(b[1][0]));
  else entries.sort((a, b) => a[0].localeCompare(b[0]));
  return entries;
}

/* The reports in the order the list shows them, with groups that are folded left out. */
function listedReports() {
  return groupReports(visibleReports())
    .filter(([label]) => !state.view.collapsed.has(label))
    .flatMap(([, items]) => items);
}

function codeBlock(loc, severity, primary) {
  const position = `${shortPath(loc.path)}${loc.linenr ? `:${loc.linenr}` : ''}${loc.column ? `:${loc.column}` : ''}`;
  let code = '';
  if (loc.line != null) {
    const line = loc.line;
    const column = Math.max(1, loc.column ?? 1);
    // Keep tabs in the caret's indent so it lines up under the same characters.
    const indent = [...line.slice(0, column - 1)].map((char) => (char === '\t' ? '\t' : ' ')).join('');
    const caret = '^'.repeat(Math.max(1, loc.length ?? 1));
    code = `<pre><span class="ln">${esc(loc.linenr ?? '')}</span>${esc(line)}\n<span class="ln"></span>${esc(indent)}<span class="caret" style="--sev: var(--sev-${severity})">${caret}</span></pre>`;
  }
  return `
    <div class="code">
      <div class="code-head">
        ${loc.tag ? `<span class="code-tag">${esc(loc.tag)}</span>` : ''}
        ${!primary && !loc.tag ? '<span class="code-tag">also</span>' : ''}
        <span class="loc" title="${esc(loc.fullpath)}"><bdi>${esc(position)}</bdi></span>
        <button class="btn btn-xs" data-act="copy-loc" data-loc="${esc(position)}">Copy</button>
        ${loc.fullpath ? `<button class="btn btn-xs" data-act="open-loc" data-path="${esc(loc.fullpath)}" data-line="${loc.linenr ?? ''}" data-col="${loc.column ?? ''}">Open</button>` : ''}
      </div>
      ${code}
    </div>`;
}

function detailHtml(report) {
  if (!report) {
    return '<div class="detail-inner"><p class="muted">Pick a report on the left to see it here.</p></div>';
  }
  const severity = severityOf(report);
  const [main, ...others] = report.locations ?? [];
  const editor = state.setup.hasVscode && state.setup.openInEditor ? 'Open in VS Code' : 'Open file';
  return `
    <div class="detail-inner" style="--sev: var(--sev-${severity})">
      <div class="detail-tags">
        <span class="badge">${esc(severity)}</span>
        <span class="muted">${esc(report.confidence)} confidence</span>
        <span class="key">${esc(report.key)}</span>
        ${report.isNew ? '<span class="new-tag">NEW</span>' : ''}
      </div>
      <h2>${esc(report.message)}</h2>
      ${report.info ? `<p>${esc(report.info)}</p>` : ''}
      ${main ? codeBlock(main, severity, true) : ''}
      ${others.map((loc) => codeBlock(loc, severity, false)).join('')}
      <div class="actions">
        ${main?.fullpath ? `<button class="btn btn-primary" data-act="open-main">${icon.open}${editor}</button>` : ''}
        <button class="btn" data-act="copy-report">Copy report</button>
        ${main?.fullpath ? `<button class="btn" data-act="reveal">${icon.folder}Show in folder</button>` : ''}
        ${report.wiki ? `<button class="btn" data-act="copy-wiki">Copy wiki link</button>` : ''}
        <button class="btn" data-act="ask-ai">${icon.spark}Ask AI to fix</button>
      </div>
      <div class="hint">Double-click a report to jump to its line. Use ↑ ↓ to move and Enter to open.</div>
    </div>`;
}

function openReport(report) {
  const loc = where(report);
  if (!loc?.fullpath) return;
  call('open_location', { path: loc.fullpath, line: loc.linenr ?? null, column: loc.column ?? null }).catch((e) => toast(String(e)));
}

async function exportReports() {
  const run = state.run;
  const base = run.mod.name.replace(/[^\w.-]+/g, '-').replace(/^-|-$/g, '') || 'reports';
  const path = await tauri.dialog.save({
    title: 'Export the reports',
    defaultPath: `${base}-reports.json`,
    filters: [
      { name: 'JSON', extensions: ['json'] },
      { name: 'Text', extensions: ['txt'] },
    ],
  });
  if (!path) return;
  const reports = visibleReports().map(({ isNew, ...report }) => report);
  const contents = /\.txt$/i.test(path) ? reports.map(reportText).join('\n\n') + '\n' : JSON.stringify(reports, null, 2);
  try {
    await call('write_text', { path, contents });
    toast(`Exported ${plural(reports.length, 'report')}`);
  } catch (error) {
    toast(String(error));
  }
}

/* ---------- Requests for the AI assistants ---------- */

/* The reports as the validator wrote them, without what the app added. */
function plainReports(list) {
  return list.map(({ isNew, ...report }) => report);
}

const ASK_HOW = 'Your assistant gets it the next time it checks xTiger, or tell it: “Pick up my xTiger requests.” You can follow it on the AI activity screen.';

async function sendRequest(request, button) {
  button.disabled = true;
  try {
    await call('ask_ai', { request });
    closeModal();
    activity.requestsStamp = null;
    toast('Request left for your AI assistant');
    return true;
  } catch (error) {
    button.disabled = false;
    toast(String(error));
    return false;
  }
}

function askToFix(report) {
  const run = state.run;
  const sameKey = run.reports.filter((item) => item.key === report.key);
  const shown = visibleReports();
  const scopes = [
    ['one', 'Only this report', [report]],
    ['key', `All ${plural(sameKey.length, 'report')} of <span class="key">${esc(report.key)}</span>`, sameKey],
    ['shown', `All ${plural(shown.length, 'report')} shown now`, shown],
  ].filter(([id, , list], i, all) => id === 'one' || (list.length > 1 && list.length !== all[i - 1][2].length));
  const modal = openModal(`
    <div class="modal-head">
      <div class="q q-idle" style="width:56px;height:56px" role="img" aria-label="Qubis"></div>
      <div>
        <p class="eyebrow">Ask AI to fix</p>
        <h2 class="title-lg">${esc(run.mod.name)}</h2>
        <div class="desc">${ASK_HOW}</div>
      </div>
    </div>
    <fieldset class="ask-scope">
      <legend class="eyebrow">What to fix</legend>
      ${scopes.map(([id, label], i) => `<label class="check"><input type="radio" name="scope" value="${id}" ${i === 0 ? 'checked' : ''}> ${label}</label>`).join('')}
    </fieldset>
    <label class="ask-note">
      <span class="eyebrow">Anything it should know (optional)</span>
      <textarea class="field" rows="3" maxlength="1000" placeholder="For example: keep the event texts as they are" spellcheck="true"></textarea>
    </label>
    <div class="modal-actions">
      <span class="grow"></span>
      <button class="btn" data-act="cancel">Cancel</button>
      <button class="btn btn-primary" data-act="ask">${icon.spark}Ask AI</button>
    </div>`);
  $('[data-act="cancel"]', modal).addEventListener('click', closeModal);
  $('[data-act="ask"]', modal).addEventListener('click', (event) => {
    const scope = $('input[name="scope"]:checked', modal).value;
    const reports = scopes.find(([id]) => id === scope)[2];
    sendRequest({
      kind: 'fix',
      modFile: run.mod.modFile,
      modName: run.mod.name,
      gameVersion: state.setup.game?.version ?? null,
      note: $('textarea', modal).value.trim() || null,
      reports: plainReports(reports),
    }, event.currentTarget);
  });
}

async function openUpdateBrief() {
  const run = state.run;
  const mod = run.mod;
  const gameVersion = state.setup.game?.version ?? null;
  let brief;
  try {
    brief = await call('update_brief', {
      info: { name: mod.name, version: mod.version ?? null, supportedVersion: mod.supportedVersion ?? null, dir: mod.dir ?? null },
      gameVersion,
      reports: plainReports(run.reports),
    });
  } catch (error) {
    toast(String(error));
    return;
  }
  const modal = openModal(`
    <div class="modal-head">
      <div class="q q-idle" style="width:56px;height:56px" role="img" aria-label="Qubis"></div>
      <div>
        <p class="eyebrow">Update report</p>
        <h2 class="title-lg">${esc(mod.name)}</h2>
        <div class="desc">What the validator found, worst first, and how to go about it. Hand it to any AI assistant, or leave it for the ones connected to xTiger.</div>
      </div>
    </div>
    <div class="changelog brief selectable">${markdown(brief)}</div>
    <div class="modal-actions">
      <button class="btn btn-sm" data-act="copy">Copy</button>
      <button class="btn btn-sm" data-act="save">Save…</button>
      <span class="grow"></span>
      <button class="btn" data-act="close">Close</button>
      <button class="btn btn-primary" data-act="ask">${icon.spark}Ask AI to update</button>
    </div>`);
  $('[data-act="close"]', modal).addEventListener('click', closeModal);
  $('[data-act="copy"]', modal).addEventListener('click', () => copy(brief, 'Update report'));
  $('[data-act="save"]', modal).addEventListener('click', async () => {
    const base = mod.name.replace(/[^\w.-]+/g, '-').replace(/^-|-$/g, '') || 'mod';
    const path = await tauri.dialog.save({
      title: 'Save the update report',
      defaultPath: `${base}-update.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (!path) return;
    try {
      await call('write_text', { path, contents: brief });
      toast('Update report saved');
    } catch (error) {
      toast(String(error));
    }
  });
  $('[data-act="ask"]', modal).addEventListener('click', (event) => {
    sendRequest({
      kind: 'update',
      modFile: mod.modFile,
      modName: mod.name,
      gameVersion,
      note: null,
      reports: plainReports(run.reports),
      brief,
    }, event.currentTarget);
  });
}

/* ---------- Screens ---------- */

SCREENS.boot = {
  html: () => '<div class="center-stage"><div class="q q-idle" style="width:96px;height:96px"></div></div>',
};

SCREENS.welcome = {
  html() {
    const { game, paradox } = state.setup;
    const local = state.mods.filter((mod) => mod.source !== 'workshop').length;
    const workshop = state.mods.length - local;
    const modsLine = state.mods.length === 0
      ? 'No mods yet. You can add a mod folder next.'
      : `${plural(local, 'local mod')}, ${workshop} from the Workshop`;
    const say = state.mods.length === 0
      ? 'Hi, I’m Qubis. I found your game. Show me a mod and I’ll check it.'
      : 'Hi, I’m Qubis. I found your game and your mods. Let’s check them.';
    return `
      <div class="welcome">
        <section class="hero" aria-label="Qubis riding a tiger">
          <img src="assets/qubis-tiger.webp" alt="">
          <div class="hero-caption">
            <div class="hero-title">xTiger</div>
            <div class="hero-sub">The validator for Crusader Kings III mods.</div>
          </div>
        </section>
        <section class="welcome-side stagger">
          <div class="speech">
            <div class="q q-idle" role="img" aria-label="Qubis" style="width:96px;height:96px"></div>
            <div class="bubble"><span class="typed" data-text="${esc(say)}"></span></div>
          </div>
          <h1 class="title-lg">Let’s set things up</h1>
          <div class="stagger" style="display:flex;flex-direction:column;gap:12px">
            <div class="card setup-row">
              ${icon.check}
              <div class="grow">
                <div class="name">Crusader Kings III ${esc(game.version ?? '')}</div>
                <div class="path" title="${esc(game.path)}"><bdi>${esc(shortPath(game.path))}</bdi></div>
              </div>
              <button class="btn btn-sm" data-act="change-game">Change</button>
            </div>
            <div class="card setup-row">
              ${paradox ? icon.check : icon.warn}
              <div class="grow">
                <div class="name">${esc(modsLine)}</div>
                <div class="path" title="${esc(paradox ?? '')}"><bdi>${esc(paradox ? shortPath(paradox) + '\\mod' : 'Paradox documents folder not found')}</bdi></div>
              </div>
              <button class="btn btn-sm" data-act="change-paradox">Change</button>
            </div>
            ${state.setup.hasVscode ? `
            <label class="check" style="padding:4px 2px">
              <input type="checkbox" data-act="vscode" ${state.setup.openInEditor ? 'checked' : ''}>
              Open reports in VS Code
            </label>` : ''}
          </div>
          <div style="display:flex;align-items:center;gap:16px;flex-wrap:wrap">
            <button class="btn btn-primary btn-lg" data-act="start">Show my mods</button>
            <span class="muted" style="font-size:13px">You can change all of this later in Settings.</span>
          </div>
        </section>
      </div>`;
  },
  bind(root) {
    typeOut(root);
    $('[data-act="start"]', root).addEventListener('click', () => go('mods'));
    $('[data-act="change-game"]', root).addEventListener('click', () => changeGame());
    $('[data-act="change-paradox"]', root).addEventListener('click', () => changeParadox());
    $('[data-act="vscode"]', root)?.addEventListener('change', (event) => setPreference({ openInEditor: event.target.checked }));
  },
};

SCREENS.notfound = {
  html() {
    const notice = state.notice
      ? `<div class="notice">${icon.warn}<span>${esc(state.notice)}</span></div>`
      : '';
    return `
      <div class="center-stage">
        <section class="stack stagger" style="width:620px;gap:24px">
          <div class="q q-confused" role="img" aria-label="Qubis looking confused" style="width:160px;height:160px"></div>
          <h1 class="title-xl" style="font-size:28px">I couldn’t find Crusader Kings III</h1>
          <p class="lead">I looked in your Steam libraries and found nothing. Show me the game folder: the one that holds <span class="mono">game</span> and <span class="mono">launcher</span>.</p>
          <form class="folder-form" data-act="form">
            <label for="gamedir">Game folder</label>
            <div class="row">
              <input id="gamedir" class="field" type="text" placeholder="…\\Crusader Kings III" spellcheck="false" autocomplete="off">
              <button class="btn" type="submit">Use this</button>
              <button class="btn btn-primary" type="button" data-act="browse">Choose folder…</button>
            </div>
            <div class="muted" style="font-size:13px">Steam usually puts it in <span class="mono">steamapps\\common\\Crusader Kings III</span>.</div>
          </form>
          ${notice}
        </section>
      </div>`;
  },
  bind(root) {
    const input = $('#gamedir', root);
    $('[data-act="browse"]', root).addEventListener('click', async () => {
      const path = await pickFolder('Pick the Crusader Kings III folder');
      if (path) await useGameDir(path);
    });
    $('[data-act="form"]', root).addEventListener('submit', async (event) => {
      event.preventDefault();
      if (input.value.trim()) await useGameDir(input.value.trim());
    });
  },
};

SCREENS.mods = {
  html() {
    const cards = state.mods.map((mod) => {
      const status = modStatus(mod);
      const meta = [mod.version ? `v${mod.version}` : null, mod.supportedVersion ? `game ${mod.supportedVersion}` : null].filter(Boolean).join(' · ') || 'no version';
      const tag = { workshop: 'Workshop', local: 'Local', added: 'Added' }[mod.source];
      return `
        <button class="mod-card" data-mod="${esc(mod.modFile)}" data-name="${esc(mod.name.toLowerCase())}" data-source="${mod.source}" aria-pressed="${mod.modFile === state.selectedMod}" title="${esc(mod.name)}">
          <div class="thumb" style="--tint:${tint(mod.name)}" ${mod.picture ? `data-picture="${esc(mod.picture)}"` : ''}>
            <span class="initials">${esc(initials(mod.name))}</span>
            <span class="tag">${tag}</span>
            ${mod.source === 'added' ? `<span class="remove-mod" role="button" tabindex="0" data-remove="${esc(mod.dir)}" aria-label="Remove ${esc(mod.name)} from the list" title="Remove from the list">${icon.close}</span>` : ''}
          </div>
          <div class="mod-body">
            <div class="mod-name">${esc(mod.name)}</div>
            <div class="mod-meta">${esc(meta)}</div>
            <div class="mod-status" style="--status:${status.color}">${esc(status.text)}</div>
          </div>
        </button>`;
    }).join('');
    const empty = '<div class="empty" hidden><div class="q q-confused" style="width:96px;height:96px"></div><span></span></div>';
    const mod = selectedMod();
    let dock;
    if (mod) {
      const sub = mod.lastRun
        ? `Last check ${ago(mod.lastRun)}: ${mod.lastCount === 0 ? 'no reports' : plural(mod.lastCount, 'report')}. Validate again to see what changed.`
        : 'Not checked yet. The first check reads the whole game, so it can take a few minutes.';
      dock = `
        <div class="q q-idle" role="img" aria-label="Qubis" style="width:48px;height:48px"></div>
        <div class="grow">
          <div class="dock-title">${esc(mod.name)}</div>
          <div class="dock-sub">${esc(sub)}</div>
        </div>
        <button class="btn btn-primary btn-lg" data-act="validate">${icon.magnifier}Validate</button>`;
    } else {
      dock = `
        <div class="q q-confused" role="img" aria-label="Qubis" style="width:48px;height:48px"></div>
        <div class="grow">
          <div class="dock-title">No mod picked</div>
          <div class="dock-sub">Add a mod folder, or put your mod in the Paradox mod folder.</div>
        </div>
        <button class="btn btn-primary btn-lg" disabled>${icon.magnifier}Validate</button>`;
    }
    return `
      <div class="page">
        <div class="page-head">
          <h1 class="title-lg">Your mods</h1>
          <span class="muted">${plural(state.mods.length, 'mod')} found</span>
          <span class="spacer"></span>
          <div class="tabs" role="tablist" aria-label="Source">
            <span class="tab-pill"></span>
            ${['all', 'local', 'workshop'].map((source) => `<button class="tab" role="tab" data-source="${source}" aria-selected="${state.source === source}">${source[0].toUpperCase() + source.slice(1)}</button>`).join('')}
          </div>
          <label class="search">
            <span class="sr-only">Search mods</span>
            ${icon.search}
            <input class="field" type="search" placeholder="Search mods" value="${esc(state.search)}" data-act="search" spellcheck="false">
          </label>
        </div>
        <div class="mod-grid stagger">
          ${cards}
          ${empty}
          <button class="add-card" data-act="add">${icon.plus}Add mod folder…</button>
        </div>
        <div class="card dock" id="dock">${dock}</div>
      </div>`;
  },
  bind(root) {
    filterCards(root);
    placeTabPill(root);
    loadPictures(root);
    for (const tab of $$('.tab', root)) {
      tab.addEventListener('click', () => {
        state.source = tab.dataset.source;
        for (const other of $$('.tab', root)) other.setAttribute('aria-selected', String(other === tab));
        placeTabPill(root);
        filterCards(root);
      });
    }
    const search = $('[data-act="search"]', root);
    search.addEventListener('input', () => {
      state.search = search.value;
      filterCards(root);
    });
    const grid = $('.mod-grid', root);
    grid.addEventListener('click', async (event) => {
      const remove = event.target.closest('[data-remove]');
      if (remove) {
        event.stopPropagation();
        await call('remove_mod_folder', { path: remove.dataset.remove });
        await loadMods();
        render();
        return;
      }
      const card = event.target.closest('.mod-card');
      if (!card) return;
      if (state.selectedMod === card.dataset.mod) return;
      state.selectedMod = card.dataset.mod;
      for (const other of $$('.mod-card', grid)) other.setAttribute('aria-pressed', String(other === card));
      const dock = $('#dock');
      const fresh = document.createElement('div');
      fresh.innerHTML = SCREENS.mods.html();
      dock.innerHTML = $('#dock', fresh).innerHTML;
      $('[data-act="validate"]', dock)?.addEventListener('click', () => validate(selectedMod()));
      dock.classList.remove('bump');
      void dock.offsetWidth;
      dock.classList.add('bump');
    });
    grid.addEventListener('dblclick', (event) => {
      const card = event.target.closest('.mod-card');
      if (card && !event.target.closest('[data-remove]')) validate(selectedMod());
    });
    $('[data-act="add"]', root).addEventListener('click', addModFolder);
    $('[data-act="validate"]', root)?.addEventListener('click', () => validate(selectedMod()));
  },
};

SCREENS.validating = {
  html() {
    const v = state.validating;
    if (v.error) {
      return `
        <div class="center-stage">
          <section class="stack stagger" style="width:640px">
            <div class="q q-confused" role="img" aria-label="Qubis looking confused" style="width:160px;height:160px"></div>
            <div style="display:flex;flex-direction:column;gap:6px">
              <div class="eyebrow" style="color:var(--sev-error)">Something went wrong</div>
              <h1 class="title-xl">I couldn’t check ${esc(v.mod.name)}</h1>
            </div>
            <div class="notice"><span class="selectable mono" style="white-space:pre-wrap;font-size:13px">${esc(v.error)}</span></div>
            <div style="display:flex;gap:12px">
              <button class="btn btn-primary" data-act="retry">${icon.again}Try again</button>
              <button class="btn" data-act="back">Back to my mods</button>
            </div>
          </section>
        </div>`;
    }
    return `
      <div class="center-stage">
        <section class="stack stagger" aria-live="polite" style="width:640px">
          <div class="q q-scan" role="img" aria-label="Qubis scanning with a magnifying glass" style="width:192px;height:192px"></div>
          <div style="display:flex;flex-direction:column;gap:6px">
            <div class="eyebrow">Validating</div>
            <h1 class="title-xl">${esc(v.mod.name)}</h1>
          </div>
          <div style="width:100%;display:flex;flex-direction:column;gap:10px">
            <div class="progress ${v.estimate ? '' : 'indeterminate'}" role="progressbar" aria-label="Progress" aria-valuemin="0" aria-valuemax="100">
              <div class="progress-fill" id="fill"></div>
            </div>
            <div class="progress-meta">
              <span id="phase">${v.estimate ? 'Reading the game and your mod' : 'First check of this mod: reading everything'}</span>
              <span class="mono" id="elapsed">0:00${v.estimate ? ` / ~${clock(v.estimate)}` : ''}</span>
            </div>
          </div>
          <ol class="card log" id="log" aria-label="Validator output">
            ${v.log.map((line) => `<li>${esc(line)}</li>`).join('') || '<li>Starting the validator…</li>'}
          </ol>
          <div class="tip" id="tip">Tip: ${esc(TIPS[v.tip % TIPS.length])}</div>
          <button class="btn" data-act="cancel" style="height:44px;padding:0 24px;font-size:15px">Cancel</button>
        </section>
      </div>`;
  },
  bind(root) {
    const v = state.validating;
    if (v.error) {
      $('[data-act="retry"]', root).addEventListener('click', () => validate(v.mod));
      $('[data-act="back"]', root).addEventListener('click', () => go('mods'));
      return;
    }
    $('[data-act="cancel"]', root).addEventListener('click', async () => {
      v.cancelled = true;
      await call('cancel_validation');
      go('mods');
    });
    const fill = $('#fill', root);
    const elapsed = $('#elapsed', root);
    const progress = $('.progress', root);
    const phase = $('#phase', root);
    const tick = () => {
      if (!fill.isConnected || state.validating !== v || v.error) return;
      const ms = Date.now() - v.started;
      elapsed.textContent = clock(ms) + (v.estimate ? ` / ~${clock(v.estimate)}` : '');
      if (v.estimate) {
        // Ease toward 95% so that a slow run never looks finished before it is.
        const ratio = ms / v.estimate;
        const percent = ratio < 1 ? ratio * 90 : 90 + 5 * (1 - Math.exp(-(ratio - 1) * 2));
        fill.style.width = `${percent}%`;
        progress.setAttribute('aria-valuenow', String(Math.round(percent)));
        if (ratio > 1.1) phase.textContent = 'Taking a bit longer than last time';
      }
      v.frame = setTimeout(tick, 250);
    };
    tick();
    v.tipTimer = setInterval(() => {
      const tip = $('#tip');
      if (!tip || state.validating !== v) return clearInterval(v.tipTimer);
      tip.style.opacity = '0';
      setTimeout(() => {
        v.tip += 1;
        tip.textContent = `Tip: ${TIPS[v.tip % TIPS.length]}`;
        tip.style.opacity = '1';
      }, 400);
    }, 7000);
  },
};

SCREENS.results = {
  html() {
    const run = state.run;
    if (!run) {
      return `
        <div class="center-stage">
          <section class="stack stagger" style="width:520px">
            <div class="q q-idle" role="img" aria-label="Qubis" style="width:128px;height:128px"></div>
            <h1 class="title-lg">No results yet</h1>
            <p class="lead">Pick a mod and press Validate. The reports show up here.</p>
            <button class="btn btn-primary btn-lg" data-act="mods">Go to my mods</button>
          </section>
        </div>`;
    }
    const counts = Object.fromEntries(SEVERITIES.map((s) => [s, 0]));
    for (const report of run.reports) counts[severityOf(report)] += 1;
    const total = run.reports.length;
    const version = state.setup.game?.version ? ` against CK3 ${esc(state.setup.game.version)}` : '';
    const change = run.previousCount == null ? 'first check of this mod' : `${plural(run.newCount, 'new one', 'new')} since the last run`;
    const qubis = counts.fatal + counts.error > 0 ? 'q-worried' : 'q-idle';
    const chips = SEVERITIES.filter((s) => s !== 'fatal' || counts.fatal > 0).map((s) => `
      <button class="chip" data-sev="${s}" aria-pressed="${state.view.severities.has(s)}" style="--sev: var(--sev-${s})">
        <span class="dot ${s === 'error' || s === 'fatal' ? 'square' : ''}"></span>${SEVERITY_LABELS[s]}<span class="count">${counts[s]}</span>
      </button>`).join('');
    return `
      <section class="results-head">
        <div class="q ${qubis}" role="img" aria-label="Qubis" style="width:64px;height:64px"></div>
        <div class="grow">
          <h1>${esc(run.mod.name)}</h1>
          <div class="results-sub"><span data-count="${total}">0</span> ${total === 1 ? 'report' : 'reports'}, ${change} · checked in ${formatDuration(run.durationMs)}${version}${run.by ? ` by ${esc(run.by)}` : ''}</div>
        </div>
        <button class="btn" data-act="brief" title="A report on what it takes to bring this mod up to the current game version, to hand to an AI assistant">Prepare update report</button>
        <button class="btn" data-act="export">Export…</button>
        <button class="btn btn-primary" data-act="again">${icon.again}Validate again</button>
      </section>
      <section class="filters" aria-label="Filters">
        ${chips}
        <span class="spacer"></span>
        <label class="check" style="font-size:13px" title="${run.previousCount == null ? 'There is no earlier run to compare with yet' : 'Only reports that the last run did not have'}">
          <input type="checkbox" data-act="only-new" ${state.view.onlyNew ? 'checked' : ''} ${run.previousCount == null ? 'disabled' : ''}>
          Only new
        </label>
        <label for="groupby" class="muted" style="font-size:13px;margin-left:8px">Group by</label>
        <select id="groupby" class="field" data-act="group">
          ${[['file', 'File'], ['key', 'Key'], ['severity', 'Severity']].map(([value, label]) => `<option value="${value}" ${state.view.groupBy === value ? 'selected' : ''}>${label}</option>`).join('')}
        </select>
        <label class="sr-only" for="filter">Filter reports</label>
        <input id="filter" class="field" type="search" placeholder="Filter by text or key  (Ctrl+F)" value="${esc(state.view.text)}" style="width:230px" spellcheck="false">
      </section>
      <div class="split">
        <section class="report-list" id="list" aria-label="Reports" role="listbox" tabindex="0"></section>
        <section class="detail" id="detail" aria-label="Report detail"></section>
      </div>`;
  },
  bind(root) {
    if (!state.run) {
      $('[data-act="mods"]', root).addEventListener('click', () => go('mods'));
      return;
    }
    for (const chip of $$('.chip', root)) {
      chip.addEventListener('click', () => {
        const s = chip.dataset.sev;
        const on = state.view.severities.has(s);
        if (on) state.view.severities.delete(s);
        else state.view.severities.add(s);
        chip.setAttribute('aria-pressed', String(!on));
        state.view.selected = 0;
        renderList();
      });
    }
    $('[data-act="only-new"]', root).addEventListener('change', (event) => {
      state.view.onlyNew = event.target.checked;
      state.view.selected = 0;
      renderList();
    });
    $('[data-act="group"]', root).addEventListener('change', (event) => {
      state.view.groupBy = event.target.value;
      state.view.collapsed.clear();
      state.view.selected = 0;
      renderList();
    });
    const filter = $('#filter', root);
    filter.addEventListener('input', () => {
      state.view.text = filter.value;
      state.view.selected = 0;
      state.view.limit = PAGE_SIZE;
      renderList();
    });
    $('[data-act="again"]', root).addEventListener('click', () => validate(state.run.mod));
    $('[data-act="export"]', root).addEventListener('click', exportReports);
    $('[data-act="brief"]', root).addEventListener('click', openUpdateBrief);

    const list = $('#list', root);
    list.addEventListener('click', (event) => {
      const head = event.target.closest('.group-head');
      if (head) {
        const label = head.dataset.group;
        if (state.view.collapsed.has(label)) state.view.collapsed.delete(label);
        else state.view.collapsed.add(label);
        renderList({ keepScroll: true });
        return;
      }
      if (event.target.closest('[data-act="more"]')) {
        state.view.limit += PAGE_SIZE;
        renderList({ keepScroll: true });
        return;
      }
      const row = event.target.closest('.row');
      if (row) selectRow(Number(row.dataset.index));
    });
    list.addEventListener('dblclick', (event) => {
      const row = event.target.closest('.row');
      if (row) openReport(listedReports()[Number(row.dataset.index)]);
    });

    const detail = $('#detail', root);
    detail.addEventListener('click', (event) => {
      const button = event.target.closest('[data-act]');
      if (!button) return;
      const report = listedReports()[state.view.selected];
      const act = button.dataset.act;
      if (act === 'open-main') openReport(report);
      else if (act === 'copy-report') copy(reportText(report), 'Report');
      else if (act === 'copy-wiki') copy(report.wiki, 'Link');
      else if (act === 'ask-ai') askToFix(report);
      else if (act === 'copy-loc') copy(button.dataset.loc, 'Location');
      else if (act === 'reveal') call('reveal', { path: where(report).fullpath }).catch((e) => toast(String(e)));
      else if (act === 'open-loc') {
        const line = Number(button.dataset.line) || null;
        const column = Number(button.dataset.col) || null;
        call('open_location', { path: button.dataset.path, line, column }).catch((e) => toast(String(e)));
      }
    });
    renderList();
  },
};

function renderList({ keepScroll = false } = {}) {
  const list = $('#list');
  if (!list) return;
  const scroll = list.scrollTop;
  const groups = groupReports(visibleReports());
  let index = 0;
  let budget = state.view.limit;
  let html = '';
  for (const [label, items] of groups) {
    const open = !state.view.collapsed.has(label);
    html += `<button class="group-head" data-group="${esc(label)}" aria-expanded="${open}">${icon.chevron}<span class="label" title="${esc(label)}">${esc(label)}</span><span>${items.length}</span></button>`;
    if (!open) continue;
    for (const report of items) {
      if (budget <= 0) break;
      budget -= 1;
      const severity = severityOf(report);
      const loc = where(report);
      const delay = keepScroll ? '' : `style="animation: rise 0.35s ${Math.min(index, 14) * 25}ms var(--ease-out) both"`;
      html += `
        <button class="row" role="option" data-index="${index}" aria-selected="${index === state.view.selected}" ${delay}>
          <span class="dot ${severity === 'error' || severity === 'fatal' ? 'square' : ''}" style="--sev: var(--sev-${severity})"></span>
          <span class="row-main">
            <span class="row-msg">${esc(report.message)}</span>
            <span class="row-meta">
              ${state.view.groupBy !== 'file' && loc ? `<span title="${esc(loc.path)}">${esc(fileName(loc.path))}</span>` : ''}
              ${loc?.linenr ? `<span>line ${loc.linenr}</span>` : ''}
              ${state.view.groupBy !== 'key' ? `<span>${esc(report.key)}</span>` : ''}
              ${report.isNew ? '<span class="new-tag">NEW</span>' : ''}
            </span>
          </span>
        </button>`;
      index += 1;
    }
  }
  const shown = listedReports().length;
  if (shown > index) html += `<button class="btn btn-sm more" data-act="more">Show ${Math.min(PAGE_SIZE, shown - index)} more of ${shown - index} left</button>`;
  if (groups.length === 0) {
    html = `<div class="list-empty">No report matches these filters.</div>`;
  }
  list.innerHTML = html;
  if (keepScroll) list.scrollTop = scroll;
  if (state.view.selected >= shown) state.view.selected = Math.max(0, shown - 1);
  renderDetail();
}

function selectRow(index) {
  const reports = listedReports();
  if (reports.length === 0) return;
  index = Math.max(0, Math.min(reports.length - 1, index));
  if (index >= state.view.limit) {
    state.view.limit = index + 1;
    renderList({ keepScroll: true });
  }
  state.view.selected = index;
  for (const row of $$('#list .row')) row.setAttribute('aria-selected', String(Number(row.dataset.index) === index));
  $(`#list .row[data-index="${index}"]`)?.scrollIntoView({ block: 'nearest' });
  renderDetail();
}

function renderDetail() {
  const detail = $('#detail');
  if (!detail) return;
  detail.innerHTML = detailHtml(listedReports()[state.view.selected]);
  detail.scrollTop = 0;
}

SCREENS.allclear = {
  html() {
    const run = state.run;
    const version = state.setup.game?.version ? ` against CK3 ${esc(state.setup.game.version)}` : '';
    const delta = run.previousCount == null ? null : -run.previousCount;
    return `
      <div class="center-stage">
        <section class="stack stagger" style="width:620px">
          <div class="party"><div class="q q-happy" role="img" aria-label="Qubis jumping happily" style="width:192px;height:192px"></div></div>
          <div style="display:flex;flex-direction:column;gap:6px">
            <div class="eyebrow good">All clear</div>
            <h1 class="title-xl">${esc(run.mod.name)} has no reports</h1>
          </div>
          <p class="lead">I found nothing to fix. Checked in ${formatDuration(run.durationMs)}${version}.</p>
          <div class="stats stagger">
            <div class="card stat"><div class="stat-value" data-count="0">0</div><div class="stat-label">reports</div></div>
            <div class="card stat"><div class="stat-value">${formatDuration(run.durationMs)}</div><div class="stat-label">to check it</div></div>
            <div class="card stat"><div class="stat-value" ${delta ? `data-count="${delta}"` : ''}>${delta == null ? '—' : delta === 0 ? '0' : delta}</div><div class="stat-label">${delta == null ? 'first check' : 'since the last run'}</div></div>
          </div>
          <div style="display:flex;gap:12px">
            <button class="btn btn-primary" data-act="mods" style="height:44px;padding:0 22px;font-size:15px">Back to my mods</button>
            <button class="btn" data-act="again" style="height:44px;padding:0 22px;font-size:15px">${icon.again}Validate again</button>
          </div>
        </section>
      </div>`;
  },
  bind(root) {
    $('[data-act="mods"]', root).addEventListener('click', () => go('mods'));
    $('[data-act="again"]', root).addEventListener('click', () => validate(state.run.mod));
    const party = $('.party', root);
    setTimeout(() => sparkle(party), 350);
    party.addEventListener('click', () => sparkle(party, 14));
  },
};

/* ---------- AI activity ---------- */

/* What the AI assistants did with xTiger, read from the MCP server's journal. The screen asks for
 * it every second while it is shown and the window is visible, and not at all otherwise. */
const activity = { stamp: null, requestsStamp: null, requests: [], entries: [], running: [], loaded: false, seen: new Set(), timer: null, session: 0, toggled: new Map(), summaries: new Map() };
const ACTIVITY_TICK = 1000;

function modLabel(entry) {
  const mod = entry.mod;
  if (!mod) return '';
  return mod.name || fileName(mod.file).replace(/\.mod$/, '');
}

/* What a call does, as a sentence: [verb while running, verb when done, rest of the sentence]. */
function describeCall(entry) {
  const mod = modLabel(entry);
  const named = mod ? ` <b>${esc(mod)}</b>` : '';
  const args = entry.args ?? {};
  switch (entry.tool) {
    case 'xtiger_validate': return ['is validating', 'Validated', named || ' a mod'];
    case 'xtiger_reports': {
      const filters = ['severity', 'key', 'path', 'pattern'].filter((name) => args[name]).map((name) => `${name} ${esc(args[name])}`);
      const how = args.group_by ? `, grouped by ${esc(args.group_by)}` : filters.length ? ` (${filters.join(', ')})` : '';
      return ['is reading the reports of', 'Read the reports of', `${named || ' the last run'}${how}`];
    }
    case 'xtiger_runs': return ['is listing', 'Listed', ' the saved runs'];
    case 'xtiger_mods': return ['is listing', 'Listed', args.filter ? ` the mods matching “${esc(args.filter)}”` : ' your mods'];
    case 'xtiger_status': return ['is checking', 'Checked', ' the setup'];
    case 'ck3_run': return ['is play-testing', 'Play-tested', `${named} in CK3`];
    case 'ck3_keys': return ['is sending keys to', 'Sent keys to', ' CK3'];
    case 'ck3_logs': return ['is reading', 'Read', ` the CK3 ${esc(args.name ?? 'error')} log`];
    case 'xtiger_session': return args.wrap_up ? ['is wrapping up', 'Wrapped up', ' the job'] : ['is naming', 'Named', ' the job'];
    default: return ['is using', 'Used', ` ${esc(entry.tool)}`];
  }
}

function clientName(entry) {
  return entry.client || 'An AI assistant';
}

function timeOfDay(ms) {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function dayLabel(ms) {
  const day = new Date(ms);
  const today = new Date();
  const start = (date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((start(today) - start(day)) / 86_400_000);
  if (days === 0) return 'Today';
  if (days === 1) return 'Yesterday';
  return day.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' });
}

function reasonHtml(entry) {
  return entry.reason ? `<div class="act-reason">“${esc(entry.reason)}”</div>` : '';
}

function nowHtml() {
  if (activity.running.length === 0) {
    const last = activity.entries[0];
    return `
      <section class="card act-now act-idle">
        <div class="q q-idle" role="img" aria-label="Qubis resting" style="width:48px;height:48px"></div>
        <div class="grow">
          <div class="act-now-title">Nothing running right now</div>
          <div class="act-meta">${last ? `<span>Last call <span data-ago="${last.finished_at ?? last.started_at}">${ago(last.finished_at ?? last.started_at)}</span>, by ${esc(clientName(last))}</span>` : ''}</div>
        </div>
        <span class="act-live" title="This screen updates by itself"><span class="act-pulse"></span>Live</span>
      </section>`;
  }
  return activity.running.map((entry) => {
    const [doing, , rest] = describeCall(entry);
    return `
      <section class="card act-now act-busy" data-running="${esc(entry.id)}" aria-live="polite">
        <div class="q q-scan" role="img" aria-label="Qubis at work" style="width:72px;height:72px"></div>
        <div class="grow">
          <div class="eyebrow">Working now</div>
          <div class="act-now-title">${esc(clientName(entry))} ${doing}${rest}</div>
          ${reasonHtml(entry)}
          <div class="progress indeterminate" role="progressbar" aria-label="Working"><div class="progress-fill"></div></div>
          <div class="progress-meta">
            <span class="act-progress" data-progress>${esc(entry.progress ?? 'Starting…')}</span>
            <span class="mono" data-elapsed="${entry.started_at}">${clock(Date.now() - entry.started_at)}</span>
          </div>
        </div>
      </section>`;
  }).join('');
}

function entryHtml(entry, fresh) {
  const [, did, rest] = describeCall(entry);
  const outcome = entry.outcome ?? 'ok';
  const summary = outcome === 'cancelled' ? 'Cancelled' : entry.summary ?? '';
  const meta = [
    summary && `<span class="act-summary">${esc(summary)}</span>`,
    entry.duration_ms != null && `<span>${formatDuration(entry.duration_ms)}</span>`,
    `<span>${esc(clientName(entry))}</span>`,
  ].filter(Boolean).join('<span class="act-sep">·</span>');
  const link = entry.mod ? `<button class="btn btn-xs" data-open="${esc(entry.id)}">See results</button>` : '';
  return `
    <li class="act-item act-${outcome}${fresh ? ' act-new' : ''}">
      <span class="act-dot" title="${outcome === 'ok' ? 'Worked' : outcome === 'error' ? 'Failed' : 'Cancelled'}"></span>
      <div class="act-body">
        <div class="act-title">${did}${rest}</div>
        ${reasonHtml(entry)}
        <div class="act-meta">${meta}</div>
      </div>
      <div class="act-side">
        <time class="mono" datetime="${new Date(entry.started_at).toISOString()}" title="${esc(new Date(entry.started_at).toLocaleString())}">${timeOfDay(entry.started_at)}</time>
        ${link}
      </div>
    </li>`;
}

/* ---------- Requests ---------- */

const REQUEST_STATUS = { waiting: 'Waiting', taken: 'Picked up', done: 'Done', skipped: 'Skipped' };

function requestHtml(request) {
  const mod = `<b>${esc(request.mod?.name || fileName(request.mod?.file ?? '').replace(/\.mod$/, ''))}</b>`;
  const title = request.kind === 'update'
    ? `Bring ${mod} up to ${request.game_version ? `CK3 ${esc(request.game_version)}` : 'date'}`
    : `Fix ${plural(request.reports_total, 'report')} in ${mod}`;
  const keys = [...new Set((request.reports ?? []).map((report) => report.key))];
  const what = request.kind === 'fix' && keys.length > 0 ? `<span class="mono">${esc(keys.slice(0, 3).join(', '))}${keys.length > 3 ? ` and ${keys.length - 3} more` : ''}</span>` : '';
  const by = esc(request.taken_by || 'an assistant');
  const when = [
    `<span>Asked <span data-ago="${request.created_at}">${ago(request.created_at)}</span></span>`,
    request.status === 'taken' && `<span>picked up by ${by}</span>`,
    request.finished_at && `<span>closed by ${by}</span>`,
  ].filter(Boolean).join('<span class="act-sep">·</span>');
  const open = request.status === 'waiting' || request.status === 'taken';
  return `
    <li class="req req-${request.status}">
      <span class="req-pill">${request.status === 'taken' ? '<span class="act-pulse"></span>' : ''}${REQUEST_STATUS[request.status] ?? esc(request.status)}</span>
      <div class="act-body">
        <div class="act-title">${title}</div>
        ${what ? `<div class="act-meta">${what}</div>` : ''}
        ${request.note ? `<div class="act-reason">“${esc(request.note)}”</div>` : ''}
        ${request.outcome ? `<blockquote class="act-wrapup">${esc(request.outcome)}</blockquote>` : ''}
        <div class="act-meta">${when}</div>
      </div>
      <div class="act-side">
        <button class="btn btn-xs" data-remove-request="${esc(request.id)}">${open ? 'Take back' : 'Clear'}</button>
      </div>
    </li>`;
}

function requestsHtml() {
  if (activity.requests.length === 0) return '';
  const waiting = activity.requests.filter((request) => request.status === 'waiting').length;
  return `
    <section class="card act-requests">
      <div class="act-requests-head">
        <h2>Your requests</h2>
        <span class="act-meta">${waiting > 0 ? `${plural(waiting, 'request')} waiting for an assistant` : 'Nothing waiting'}</span>
      </div>
      <ul class="req-list">${activity.requests.map(requestHtml).join('')}</ul>
    </section>`;
}

async function removeRequest(button) {
  button.disabled = true;
  try {
    await call('remove_request', { id: button.dataset.removeRequest });
    activity.requests = activity.requests.filter((request) => request.id !== button.dataset.removeRequest);
    activity.requestsStamp = null;
    const slot = $('[data-requests]');
    if (slot) slot.innerHTML = requestsHtml();
  } catch (error) {
    button.disabled = false;
    toast(String(error));
  }
}

/* ---------- Work sessions ---------- */

/* The calls come in work sessions: what an assistant did for one job, from naming it to wrapping it
 * up. Calls from before sessions were kept make one session per server run. The summary of a session
 * (what was fixed, what is left, which files changed) is asked for only when its card is open. */
const SESSION_TOOL = 'xtiger_session';
const SESSION_GAP = 30 * 60_000;
const SUMMARY_TICK = 10_000;
const MAX_BARS = 12;
const STATUS_LABELS = { working: 'Working', open: 'In progress', wrapped: 'Wrapped up', ended: 'Ended' };

function sessionKey(entry) {
  return entry.session ?? `pid:${String(entry.id).split('-')[0]}`;
}

function errorsOf(found) {
  return found.fatal + found.error;
}

function reportsOf(found) {
  return found.fatal + found.error + found.warning + found.untidy + found.tips;
}

/* The sessions in the journal, newest first, with what the entries alone tell about them. */
function sessionsOf() {
  const sessions = new Map();
  for (const entry of activity.entries) {
    const key = sessionKey(entry);
    if (!sessions.has(key)) sessions.set(key, { key, saved: entry.session != null, entries: [] });
    sessions.get(key).entries.push(entry);
  }
  return [...sessions.values()].map((session) => {
    const { entries } = session;
    const named = entries.find((entry) => entry.tool === SESSION_TOOL && entry.args?.title);
    const wrapped = entries.find((entry) => entry.tool === SESSION_TOOL && entry.args?.wrap_up);
    const lastAt = Math.max(...entries.map((entry) => entry.finished_at ?? entry.started_at));
    const mods = new Map();
    for (const entry of [...entries].reverse()) {
      if (!entry.mod) continue;
      if (!mods.has(entry.mod.file)) mods.set(entry.mod.file, { name: modLabel(entry), file: entry.mod.file, passes: [] });
      if (entry.tool === 'xtiger_validate' && entry.outcome === 'ok' && entry.found) mods.get(entry.mod.file).passes.push(entry);
    }
    const names = [...mods.values()].map((mod) => mod.name);
    let status = 'ended';
    if (activity.running.some((entry) => sessionKey(entry) === session.key)) status = 'working';
    else if (wrapped) status = 'wrapped';
    else if (session.saved && Date.now() - lastAt < SESSION_GAP) status = 'open';
    return {
      ...session,
      title: named?.args.title ?? (names.length > 0 ? `Worked on ${names.join(', ')}` : 'Looked around'),
      named: Boolean(named),
      wrapUp: wrapped ? wrapped.args.wrap_up : null,
      calls: entries.filter((entry) => entry.tool !== SESSION_TOOL),
      client: clientName(entries.at(-1)),
      startedAt: entries.at(-1).started_at,
      lastAt,
      status,
      mods: [...mods.values()],
    };
  });
}

function sessionOpen(session, index) {
  return activity.toggled.get(session.key) ?? index === 0;
}

/* How the reports went down from one validation to the next. */
function passesHtml(mod) {
  const passes = mod.passes.slice(-MAX_BARS);
  if (passes.length === 0) return '';
  const totals = passes.map((entry) => reportsOf(entry.found));
  const highest = Math.max(1, ...totals);
  const way = totals.length > 5 ? [totals[0], '…', totals.at(-1)] : totals;
  const first = passes[0].found;
  const last = passes.at(-1).found;
  const errors = passes.length > 1 && errorsOf(first) !== errorsOf(last) ? `Errors ${errorsOf(first)} → ${errorsOf(last)}` : plural(errorsOf(last), 'error');
  const bars = passes.map((entry, i) => {
    const { found } = entry;
    const parts = [['error', errorsOf(found)], ['warning', found.warning], ['tips', found.untidy + found.tips]];
    const segments = parts.filter(([, count]) => count > 0).map(([severity, count]) => `<span style="--sev: var(--sev-${severity}); flex-grow: ${count}"></span>`).join('');
    const height = totals[i] === 0 ? 0 : Math.max(8, Math.round((totals[i] / highest) * 100));
    const label = `Check ${mod.passes.length - passes.length + i + 1} at ${timeOfDay(entry.finished_at)}: ${plural(errorsOf(found), 'error')}, ${plural(found.warning, 'warning')}, ${found.untidy + found.tips} other`;
    return `
      <li>
        <button class="act-bar-btn" data-open="${esc(entry.id)}" title="${esc(label)}" aria-label="${esc(label)}">
          <span class="act-bar${totals[i] === 0 ? ' act-bar-zero' : ''}" style="height: ${height}%">${segments}</span>
        </button>
        <span class="act-bar-n mono">${totals[i]}</span>
      </li>`;
  }).join('');
  return `
    <div class="act-chart">
      <div class="act-chart-text">
        <div class="act-chart-mod">${esc(mod.name)}</div>
        <div class="act-chart-way mono">${way.join(' → ')}</div>
        <div class="act-meta">${passes.length === 1 ? 'reports in one check' : `reports in ${passes.length} checks`}<span class="act-sep">·</span>${esc(errors)}</div>
      </div>
      <ol class="act-bars" style="--bars: ${passes.length}">${bars}</ol>
    </div>`;
}

function pendingHtml(row) {
  const severity = SEVERITIES.includes(row.severity) ? row.severity : 'tips';
  const line = Number(String(row.where).split(':').pop()) || null;
  const open = row.file ? `data-file="${esc(row.file)}"${line ? ` data-line="${line}"` : ''}` : 'disabled';
  return `
    <li>
      <button class="act-pending" ${open} title="${esc(row.info ?? row.message ?? '')}">
        <span class="dot ${severity === 'error' || severity === 'fatal' ? 'square' : ''}" style="--sev: var(--sev-${severity})"></span>
        <span class="act-pending-msg">${esc(row.message ?? row.key ?? '')}</span>
        <span class="act-pending-where mono">${esc(row.where)}</span>
      </button>
    </li>`;
}

function modSummaryHtml(mod) {
  const counts = [];
  if (mod.fixed != null) counts.push(`<b class="act-fixed">${plural(mod.fixed, 'fixed', 'fixed')}</b>`);
  if (mod.new) counts.push(`${mod.new} new`);
  if (mod.lastRunId) counts.push(mod.pendingTotal === 0 ? '<b class="act-fixed">nothing left</b>' : `${mod.pendingTotal} left`);
  const more = mod.pendingTotal > mod.pending.length ? `<li class="act-more muted">and ${plural(mod.pendingTotal - mod.pending.length, 'more report')}</li>` : '';
  const seeAll = mod.lastRunId && mod.pendingTotal > 0 ? `<button class="btn btn-xs" data-run-id="${esc(mod.lastRunId)}" data-mod="${esc(mod.mod.file)}" data-name="${esc(mod.mod.name ?? '')}">See all</button>` : '';
  const files = mod.files.map((file) => `
    <li>
      <button class="act-file" data-file="${esc(file.fullPath)}" title="${esc(file.fullPath)}"><span class="mono">${esc(file.path)}</span><time>${timeOfDay(file.modifiedAt)}</time></button>
    </li>`).join('');
  const moreFiles = mod.filesTotal > mod.files.length ? `<li class="act-more muted">and ${plural(mod.filesTotal - mod.files.length, 'more file')}</li>` : '';
  return `
    <div class="act-sum">
      <div class="act-sum-head">
        <span class="act-sum-mod">${esc(mod.mod.name ?? fileName(mod.mod.file))}</span>
        <span class="act-meta">${counts.join('<span class="act-sep">·</span>')}</span>
        <span class="grow"></span>
        ${seeAll}
      </div>
      ${mod.pending.length > 0 ? `<div class="eyebrow">Left to fix</div><ul class="act-pending-list">${mod.pending.map(pendingHtml).join('')}${more}</ul>` : ''}
      ${mod.filesTotal > 0 ? `<details class="act-files"><summary>${plural(mod.filesTotal, 'file')} changed</summary><ul>${files}${moreFiles}</ul></details>` : ''}
    </div>`;
}

function summaryHtml(session) {
  if (!session.saved) return '';
  const cached = activity.summaries.get(session.key);
  if (!cached?.data) return `<p class="act-sum-note muted">${cached?.error ? esc(cached.error) : 'Summing up…'}</p>`;
  const mods = cached.data.mods.filter((mod) => mod.lastRunId || mod.filesTotal > 0);
  return mods.map(modSummaryHtml).join('');
}

function sessionHtml(session, index, fresh) {
  const open = sessionOpen(session, index);
  const range = timeOfDay(session.startedAt) === timeOfDay(session.lastAt) ? timeOfDay(session.startedAt) : `${timeOfDay(session.startedAt)} – ${timeOfDay(session.lastAt)}`;
  const meta = [esc(session.client), range, plural(session.calls.length, 'call')].join('<span class="act-sep">·</span>');
  const charts = session.mods.map(passesHtml).join('');
  return `
    <article class="card act-session act-status-${session.status}" data-session="${esc(session.key)}">
      <button class="act-session-head" aria-expanded="${open}">
        ${icon.chevron}
        <span class="act-session-text">
          <span class="act-session-title${session.named ? '' : ' act-unnamed'}">${esc(session.title)}</span>
          <span class="act-meta">${meta}</span>
        </span>
        <span class="act-status">${session.status === 'working' || session.status === 'open' ? '<span class="act-pulse"></span>' : ''}${STATUS_LABELS[session.status]}</span>
      </button>
      <div class="act-session-body"${open ? '' : ' hidden'}>
        ${session.wrapUp ? `<blockquote class="act-wrapup">${esc(session.wrapUp)}</blockquote>` : ''}
        ${charts ? `<div class="act-charts">${charts}</div>` : ''}
        <div class="act-sums" data-summary>${summaryHtml(session)}</div>
        ${session.calls.length > 0 ? `<ol class="act-list">${session.calls.map((entry) => entryHtml(entry, fresh.has(entry.id))).join('')}</ol>` : '<p class="act-sum-note muted">No calls yet.</p>'}
      </div>
    </article>`;
}

function timelineHtml(fresh = new Set()) {
  const days = [];
  sessionsOf().forEach((session, index) => {
    const label = dayLabel(session.lastAt);
    if (days.at(-1)?.label !== label) days.push({ label, html: [] });
    days.at(-1).html.push(sessionHtml(session, index, fresh));
  });
  return days.map((day) => `
    <section class="act-day">
      <h2>${esc(day.label)}</h2>
      <div class="act-sessions">${day.html.join('')}</div>
    </section>`).join('');
}

/* Ask for the summaries of the open sessions that changed since they were last asked for. A session
 * still going is asked again every few seconds, for the files changed in the meantime. */
function refreshSummaries() {
  sessionsOf().forEach((session, index) => {
    if (!session.saved || !sessionOpen(session, index)) return;
    const going = session.status === 'working' || session.status === 'open';
    const stamp = `${session.entries[0].id}|${session.status}|${going ? Math.floor(Date.now() / SUMMARY_TICK) : ''}`;
    const cached = activity.summaries.get(session.key);
    if (cached?.stamp === stamp || cached?.loading) return;
    activity.summaries.set(session.key, { ...cached, stamp, loading: true });
    call('ai_session', { session: session.key })
      .then((data) => activity.summaries.set(session.key, { stamp, data }))
      .catch((error) => activity.summaries.set(session.key, { stamp, data: cached?.data, error: String(error) }))
      .finally(() => {
        const slot = $(`[data-session="${CSS.escape(session.key)}"] [data-summary]`);
        if (!slot) return;
        const html = summaryHtml(session);
        // Keep what the user opened (the list of files) when nothing changed.
        if (slot.dataset.html !== html) {
          slot.innerHTML = html;
          slot.dataset.html = html;
        }
      });
  });
}

function toggleSession(head) {
  const card = head.closest('[data-session]');
  const open = head.getAttribute('aria-expanded') !== 'true';
  activity.toggled.set(card.dataset.session, open);
  head.setAttribute('aria-expanded', String(open));
  $('.act-session-body', card).hidden = !open;
  if (open) refreshSummaries();
}

function openFile(button) {
  const line = button.dataset.line ? Number(button.dataset.line) : null;
  call('open_location', { path: button.dataset.file, line, column: null }).catch((e) => toast(String(e)));
}

function activityEmpty() {
  return activity.loaded && activity.entries.length === 0 && activity.running.length === 0 && activity.requests.length === 0;
}

SCREENS.activity = {
  html() {
    // The first visit waits for the journal, so the screen comes in once, already filled.
    if (!activity.loaded) return '<div class="page"></div>';
    if (activityEmpty()) {
      return `
        <div class="center-stage">
          <section class="stack stagger" style="width:560px">
            <div class="q q-idle" role="img" aria-label="Qubis waiting" style="width:128px;height:128px"></div>
            <h1 class="title-lg">No AI activity yet</h1>
            <p class="lead">When an AI assistant uses xTiger to check or play-test your mods, you can follow it here as it works: what it does, why, and what it found.</p>
            <button class="btn btn-primary btn-lg" data-act="connect">Connect an assistant</button>
          </section>
        </div>`;
    }
    return `
      <div class="page act-page">
        <div class="act-wrap">
          <header class="act-head">
            <h1 class="title-lg">AI activity</h1>
            <p class="muted">What your AI assistants did with xTiger, and why.</p>
          </header>
          <div class="act-now-list" data-now>${nowHtml()}</div>
          <div data-requests>${requestsHtml()}</div>
          <div class="act-timeline stagger" data-timeline>${timelineHtml()}</div>
        </div>
      </div>`;
  },
  bind(root) {
    $('[data-act="connect"]', root)?.addEventListener('click', () => {
      state.scrollTo = 'ai';
      go('settings');
    });
    for (const entry of activity.entries) activity.seen.add(entry.id);
    $('[data-requests]', root)?.addEventListener('click', (event) => {
      const button = event.target.closest('[data-remove-request]');
      if (button) removeRequest(button);
    });
    $('[data-timeline]', root)?.addEventListener('click', (event) => {
      const head = event.target.closest('.act-session-head');
      if (head) return toggleSession(head);
      const file = event.target.closest('[data-file]');
      if (file) return openFile(file);
      const all = event.target.closest('[data-run-id]');
      if (all) return openRun(all.dataset.runId, { file: all.dataset.mod, name: all.dataset.name }, null, null, all);
      const button = event.target.closest('[data-open]');
      if (button) openActivity(button.dataset.open, button);
    });
    activity.session += 1;
    clearTimeout(activity.timer);
    activity.timer = null;
    pollActivity(activity.session);
  },
};

async function pollActivity(session) {
  // A poll of an older visit leaves the timer alone: it belongs to the current one.
  if (session !== activity.session) return;
  if (state.screen !== 'activity' || document.hidden) {
    activity.timer = null;
    return;
  }
  activity.timer = -1; // A request is on its way.
  try {
    const result = await call('ai_activity', { stamp: activity.stamp, requestsStamp: activity.requestsStamp });
    if (session !== activity.session || state.screen !== 'activity') return;
    const wasEmpty = !activity.loaded || activityEmpty();
    const runningBefore = activity.running.map((entry) => entry.id).join();
    activity.running = result.running;
    const changed = result.entries != null;
    if (changed) activity.entries = result.entries;
    activity.stamp = result.stamp;
    const asked = result.requests != null;
    if (asked) activity.requests = result.requests;
    activity.requestsStamp = result.requestsStamp;
    const firstLoad = !activity.loaded;
    activity.loaded = true;
    refreshSummaries();
    if (firstLoad || wasEmpty !== activityEmpty()) {
      render();
      return;
    }
    showActivity(runningBefore !== activity.running.map((entry) => entry.id).join(), changed, asked);
  } catch {
    // The journal could not be read this time; try again on the next tick.
  } finally {
    if (session === activity.session) {
      const shown = state.screen === 'activity' && !document.hidden;
      activity.timer = shown ? setTimeout(() => pollActivity(session), ACTIVITY_TICK) : null;
    }
  }
}

/* Update the shown screen in place, so nothing jumps while the user reads. */
function showActivity(runningChanged, entriesChanged, requestsChanged) {
  const now = $('[data-now]');
  if (!now) return;
  const requests = $('[data-requests]');
  if (requestsChanged && requests) requests.innerHTML = requestsHtml();
  for (const element of $$('[data-ago]', requests ?? now)) element.textContent = ago(Number(element.dataset.ago));
  if (runningChanged || entriesChanged) {
    now.innerHTML = nowHtml();
  } else {
    for (const entry of activity.running) {
      const card = $(`[data-running="${CSS.escape(entry.id)}"]`, now);
      if (!card) continue;
      const progress = $('[data-progress]', card);
      const text = entry.progress ?? 'Starting…';
      if (progress.textContent !== text) progress.textContent = text;
    }
  }
  for (const element of $$('[data-elapsed]', now)) element.textContent = clock(Date.now() - Number(element.dataset.elapsed));
  for (const element of $$('[data-ago]', now)) element.textContent = ago(Number(element.dataset.ago));
  if (entriesChanged) {
    const fresh = new Set(activity.entries.filter((entry) => !activity.seen.has(entry.id)).map((entry) => entry.id));
    for (const id of fresh) activity.seen.add(id);
    const timeline = $('[data-timeline]');
    timeline.classList.remove('stagger');
    timeline.innerHTML = timelineHtml(fresh);
  }
}

async function openActivity(id, button) {
  const entry = activity.entries.find((item) => item.id === id);
  if (!entry?.mod) return;
  // A call that did not validate (a play-test, say) shows the newest check of the same mod.
  const runId = entry.run_id ?? activity.entries.find((item) => item.run_id && item.mod?.file === entry.mod.file)?.run_id;
  if (!runId) {
    state.selectedMod = entry.mod.file;
    toast('No check of this mod yet');
    go('mods');
    return;
  }
  await openRun(runId, { file: entry.mod.file, name: modLabel(entry) }, clientName(entry), entry.finished_at, button);
}

/* Show a saved run of an assistant on the results screen. */
async function openRun(runId, modRef, by, finishedAt, button) {
  const mod = state.mods.find((item) => item.modFile === modRef.file) ?? { name: modRef.name || fileName(modRef.file).replace(/\.mod$/, ''), modFile: modRef.file };
  button.disabled = true;
  try {
    const run = await call('ai_run', { runId });
    state.run = { ...run, mod, finished: run.finishedAt || finishedAt, by: by ?? run.by ?? activity.entries.find((item) => item.run_id === runId)?.client ?? 'An AI assistant' };
    state.view = { ...state.view, selected: 0, collapsed: new Set(), limit: PAGE_SIZE, onlyNew: false };
    go(run.reports.length > 0 ? 'results' : 'allclear');
  } catch (error) {
    button.disabled = false;
    toast(String(error));
  }
}

document.addEventListener('visibilitychange', () => {
  if (!document.hidden && state.screen === 'activity' && !activity.timer) pollActivity(activity.session);
});

SCREENS.settings = {
  html() {
    const { game, paradox, hasVscode, openInEditor, theme, checkUpdates } = state.setup;
    const update = updates.status?.update;
    const checked = updates.status?.checkedAt;
    const aboutDesc = update
      ? `Version ${esc(update.version)} is out.`
      : checked ? `Up to date. Checked ${ago(checked)}.` : 'A validator for Crusader Kings III mods, built on Tiger. Free software under the GPL.';
    return `
      <div class="page" style="overflow-y:auto;align-items:center">
        <div class="settings stagger">
          <h1 class="title-lg">Settings</h1>
          <h2>Folders</h2>
          <div class="card setting">
            <div class="grow">
              <div class="name">Game folder</div>
              <div class="path" title="${esc(game?.path ?? '')}"><bdi>${esc(game ? shortPath(game.path) : 'Not set')}</bdi></div>
            </div>
            <button class="btn btn-sm" data-act="change-game">Change…</button>
          </div>
          <div class="card setting">
            <div class="grow">
              <div class="name">Paradox documents folder</div>
              <div class="path" title="${esc(paradox ?? '')}"><bdi>${esc(paradox ? shortPath(paradox) : 'Not found')}</bdi></div>
            </div>
            <button class="btn btn-sm" data-act="change-paradox">Change…</button>
          </div>
          <h2>Reports</h2>
          <label class="card setting" style="cursor:pointer">
            <div class="grow">
              <div class="name">Open reports in VS Code</div>
              <div class="desc">${hasVscode ? 'Jump straight to the line. When this is off, files open in their default program.' : 'VS Code was not found, so files open in their default program.'}</div>
            </div>
            <input type="checkbox" class="switch" data-act="vscode" ${openInEditor && hasVscode ? 'checked' : ''} ${hasVscode ? '' : 'disabled'}>
          </label>
          <h2>AI assistants</h2>
          <div class="ai-clients" data-ai>${aiHtml()}</div>
          <h2>Look</h2>
          <div class="card setting">
            <div class="grow">
              <div class="name">Theme</div>
              <div class="desc">System follows the Windows setting.</div>
            </div>
            <div class="segmented" role="group" aria-label="Theme">
              ${['system', 'dark', 'light'].map((value) => `<button data-theme-choice="${value}" aria-pressed="${theme === value}">${value[0].toUpperCase() + value.slice(1)}</button>`).join('')}
            </div>
          </div>
          <h2>Updates</h2>
          <label class="card setting" style="cursor:pointer">
            <div class="grow">
              <div class="name">Look for new versions</div>
              <div class="desc">xTiger asks GitHub for a new release when it opens and every hour after. Nothing about you or your mods is sent.</div>
            </div>
            <input type="checkbox" class="switch" data-act="check-updates" ${checkUpdates ? 'checked' : ''}>
          </label>
          <h2>About</h2>
          <div class="card setting">
            <div class="q q-idle" style="width:48px;height:48px"></div>
            <div class="grow">
              <div class="name">xTiger ${esc(state.version ?? '')}</div>
              <div class="desc">${aboutDesc}</div>
            </div>
            ${update ? `<button class="btn btn-primary btn-sm" data-act="show-update">Update to ${esc(update.version)}</button>` : ''}
          </div>
        </div>
      </div>`;
  },
  bind(root) {
    $('[data-act="change-game"]', root).addEventListener('click', () => changeGame());
    $('[data-act="change-paradox"]', root).addEventListener('click', () => changeParadox());
    $('[data-act="vscode"]', root).addEventListener('change', (event) => setPreference({ openInEditor: event.target.checked }));
    $('[data-act="show-update"]', root)?.addEventListener('click', () => openUpdate());
    const ai = $('[data-ai]', root);
    bindAi(ai);
    if (state.scrollTo === 'ai') {
      state.scrollTo = null;
      ai.previousElementSibling.scrollIntoView({ block: 'start', behavior: reducedMotion() ? 'auto' : 'smooth' });
    }
    $('[data-act="check-updates"]', root).addEventListener('change', async (event) => {
      const on = event.target.checked;
      await setPreference({ checkUpdates: on });
      if (!on) return;
      try {
        setUpdateStatus(await call('check_update'));
      } catch {
        // Offline: the hourly check tries again.
      }
    });
    for (const button of $$('[data-theme-choice]', root)) {
      button.addEventListener('click', async () => {
        const theme = button.dataset.themeChoice;
        for (const other of $$('[data-theme-choice]', root)) other.setAttribute('aria-pressed', String(other === button));
        const apply = () => {
          state.setup.theme = theme;
          applyTheme();
        };
        transition(apply);
        await setPreference({ theme });
      });
    }
  },
};

/* ---------- AI assistants ---------- */

// Filled when Settings opens; `null` until the first answer.
const ai = { overview: null, justConnected: null, busy: null };

function aiRow(client, serverOk) {
  const busy = ai.busy === client.id;
  const disabled = busy || ai.busy ? 'disabled' : '';
  const file = client.file ? `<div class="path" title="${esc(client.file)}"><bdi>${esc(shortPath(client.file))}</bdi></div>` : '';
  let status;
  let desc;
  let action;
  switch (client.state) {
    case 'connected':
      status = '<span class="ai-state ai-on">Connected</span>';
      desc = ai.justConnected === client.id ? esc(client.after) : 'It can validate your mods and read the reports.';
      action = `<button class="btn btn-sm" data-ai-act="disconnect" data-id="${client.id}" ${disabled}>Disconnect</button>`;
      break;
    case 'elsewhere':
      status = '<span class="ai-state ai-warn">Other copy</span>';
      desc = `Set up with another copy of xTiger: <bdi class="mono">${esc(shortPath(client.otherCommand))}</bdi>`;
      action = `<button class="btn btn-primary btn-sm" data-ai-act="connect" data-id="${client.id}" ${serverOk ? disabled : 'disabled'}>Use this copy</button>`;
      break;
    case 'unreadable':
      status = '<span class="ai-state ai-warn">Set up by hand</span>';
      desc = 'Its settings file has comments or is not plain JSON, so xTiger leaves it alone. Copy the settings and paste them in.';
      action = `<button class="btn btn-sm" data-ai-act="copy" data-id="${client.id}">Copy settings</button>`;
      break;
    default:
      status = '';
      desc = 'Not connected.';
      action = `<button class="btn btn-primary btn-sm" data-ai-act="connect" data-id="${client.id}" ${serverOk ? disabled : 'disabled'}>${busy ? 'Connecting…' : 'Connect'}</button>`;
  }
  return `
    <div class="card setting ai-client">
      <div class="grow">
        <div class="name">${esc(client.name)} ${status}</div>
        <div class="desc">${desc}</div>
        ${file}
      </div>
      ${action}
    </div>`;
}

function aiHtml() {
  const overview = ai.overview;
  if (!overview) return '<div class="card setting"><div class="grow"><div class="desc">Looking for AI assistants…</div></div></div>';
  const serverOk = Boolean(overview.server);
  const found = overview.clients.filter((client) => client.state !== 'missing');
  const missing = overview.clients.filter((client) => client.state === 'missing');
  const intro = `
    <div class="desc ai-intro">Connect an assistant and it can validate your mods, read the reports and start the game for you. The assistant sees what it reads, the same as when you paste it into a chat.</div>`;
  const notice = serverOk ? '' : `
    <div class="notice">xtiger-mcp.exe is missing next to xTiger, so assistants cannot be connected. Reinstall xTiger to get it back.</div>`;
  const none = found.length ? '' : `
    <div class="card setting"><div class="grow"><div class="desc">No supported assistant was found. Install Claude Desktop, Claude Code, Cursor, VS Code or Windsurf, or set up another one below.</div></div></div>`;
  const other = `
    <div class="card setting">
      <div class="grow">
        <div class="name">Another assistant</div>
        <div class="desc">Add these settings to its MCP servers.${missing.length ? ` Not found here: ${missing.map((client) => esc(client.name)).join(', ')}.` : ''}</div>
      </div>
      <button class="btn btn-sm" data-ai-act="copy-any">Copy settings</button>
    </div>`;
  return intro + notice + none + found.map((client) => aiRow(client, serverOk)).join('') + other;
}

function bindAi(box) {
  const redraw = () => {
    box.innerHTML = aiHtml();
  };
  const load = async () => {
    try {
      ai.overview = await call('ai_clients');
    } catch (error) {
      toast(String(error));
      return;
    }
    if (box.isConnected) redraw();
  };
  box.addEventListener('click', async (event) => {
    const button = event.target.closest('[data-ai-act]');
    if (!button || !ai.overview) return;
    const act = button.dataset.aiAct;
    const client = ai.overview.clients.find((each) => each.id === button.dataset.id);
    if (act === 'copy-any') return copy(ai.overview.snippet, 'Settings');
    if (act === 'copy') return copy(client.snippet, 'Settings');
    ai.busy = client.id;
    redraw();
    try {
      const status = await call(act === 'connect' ? 'connect_ai' : 'disconnect_ai', { id: client.id });
      ai.overview.clients = ai.overview.clients.map((each) => (each.id === status.id ? status : each));
      ai.justConnected = act === 'connect' ? status.id : null;
      toast(act === 'connect' ? `${status.name} connected` : `${status.name} disconnected`);
    } catch (error) {
      toast(String(error));
    } finally {
      ai.busy = null;
      if (box.isConnected) redraw();
    }
  });
  load();
}

/* ---------- Actions ---------- */

async function useGameDir(path) {
  try {
    state.setup.game = await call('set_game_dir', { path });
    state.notice = null;
    if (!state.modsLoaded) await loadMods();
    go(state.screen === 'notfound' ? 'welcome' : state.screen, { animate: state.screen === 'notfound' });
    toast('Game folder saved');
  } catch (error) {
    state.notice = String(error);
    if (state.screen === 'notfound') render();
    else toast(String(error));
  }
}

async function changeGame() {
  const path = await pickFolder('Pick the Crusader Kings III folder');
  if (path) await useGameDir(path);
}

async function changeParadox() {
  const path = await pickFolder('Pick the Paradox Interactive\\Crusader Kings III folder in your documents');
  if (!path) return;
  try {
    await call('set_paradox_dir', { path });
    state.setup.paradox = path;
    await loadMods();
    render();
    toast('Documents folder saved');
  } catch (error) {
    toast(String(error));
  }
}

async function addModFolder() {
  const path = await pickFolder('Pick a mod folder (the one with descriptor.mod)');
  if (!path) return;
  try {
    const mod = await call('add_mod_folder', { path });
    await loadMods();
    state.selectedMod = mod.modFile;
    render();
    toast(`${mod.name} added`);
  } catch (error) {
    toast(String(error));
  }
}

async function setPreference(change) {
  Object.assign(state.setup, change);
  try {
    await call('set_preferences', { openInEditor: change.openInEditor ?? null, theme: change.theme ?? null, checkUpdates: change.checkUpdates ?? null });
  } catch (error) {
    toast(String(error));
  }
}

/* ---------- Keyboard ---------- */

document.addEventListener('keydown', (event) => {
  if (state.screen !== 'results' || !state.run) return;
  const typing = event.target.matches('input, select, textarea');
  if (event.ctrlKey && event.key.toLowerCase() === 'f') {
    event.preventDefault();
    $('#filter')?.focus();
    $('#filter')?.select();
    return;
  }
  if (event.key === 'Escape' && event.target.id === 'filter') {
    event.target.value = '';
    state.view.text = '';
    renderList();
    $('#list')?.focus();
    return;
  }
  if (typing) return;
  if (event.key === 'ArrowDown' || event.key === 'j') {
    event.preventDefault();
    selectRow(state.view.selected + 1);
  } else if (event.key === 'ArrowUp' || event.key === 'k') {
    event.preventDefault();
    selectRow(state.view.selected - 1);
  } else if (event.key === 'Enter') {
    event.preventDefault();
    openReport(listedReports()[state.view.selected]);
  }
});

/* ---------- Updates ---------- */

/* The Rust side asks GitHub for a new release at startup and every hour after. Nothing about
 * updates is shown until one exists. */

const updates = { status: null, installing: false };

function showUpdatePill() {
  const pill = $('#update-pill');
  const update = updates.installing ? null : updates.status?.update;
  pill.hidden = !update;
  if (update) {
    pill.querySelector('.pill-text').textContent = `Update to ${update.version}`;
    pill.title = `xTiger ${update.version} is available`;
  }
}

function setUpdateStatus(status) {
  updates.status = status;
  showUpdatePill();
  if (state.screen === 'settings' && !updates.installing) render();
}

async function watchUpdates() {
  $('#update-pill').addEventListener('click', () => openUpdate());
  await tauri.event.listen('update-available', ({ payload }) => setUpdateStatus(payload));
  try {
    setUpdateStatus(await call('get_update'));
  } catch {
    // Not knowing about updates is fine.
  }
}

function openModal(html, { dismissable = true } = {}) {
  const root = $('#modal');
  root.innerHTML = `<div class="modal-backdrop"></div><div class="modal card" role="dialog" aria-modal="true" tabindex="-1">${html}</div>`;
  root.hidden = false;
  root.dataset.dismissable = String(dismissable);
  $('.modal-backdrop', root).addEventListener('click', () => {
    if (root.dataset.dismissable === 'true') closeModal();
  });
  ($('.modal .btn-primary', root) ?? $('.modal', root)).focus();
  return $('.modal', root);
}

function closeModal() {
  const root = $('#modal');
  root.hidden = true;
  root.innerHTML = '';
}

document.addEventListener('keydown', (event) => {
  const root = $('#modal');
  if (root.hidden) return;
  // Keep the shortcuts of the screen behind the modal from firing.
  event.stopPropagation();
  if (event.key === 'Escape' && root.dataset.dismissable === 'true') closeModal();
}, true);

/* A small Markdown reader for release notes: headings, lists, code, bold and italics. Everything
 * is escaped first, and links keep only their text. */
function markdown(text) {
  const inline = (line) => esc(line)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^\w*])\*([^*\s][^*]*?)\*(?=[^\w*]|$)/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\([^)\s]+\)/g, '$1');
  const out = [];
  let list = null;
  let paragraph = [];
  let code = null;
  const flush = () => {
    if (paragraph.length) out.push(`<p>${paragraph.map(inline).join(' ')}</p>`);
    paragraph = [];
  };
  const closeList = () => {
    if (list) out.push(`</${list}>`);
    list = null;
  };
  for (const raw of String(text ?? '').replace(/\r/g, '').split('\n')) {
    if (code !== null) {
      if (raw.trim().startsWith('```')) {
        out.push(`<pre>${esc(code.join('\n'))}</pre>`);
        code = null;
      } else {
        code.push(raw);
      }
      continue;
    }
    if (raw.trim().startsWith('```')) {
      flush();
      closeList();
      code = [];
      continue;
    }
    const heading = raw.match(/^(#{1,6})\s+(.*)$/);
    const item = raw.match(/^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/);
    if (heading) {
      flush();
      closeList();
      const level = Math.min(5, heading[1].length + 2);
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
    } else if (item) {
      flush();
      const kind = /^\s*\d/.test(raw) ? 'ol' : 'ul';
      if (list !== kind) {
        closeList();
        out.push(`<${kind}>`);
        list = kind;
      }
      out.push(`<li${item[1].length >= 2 ? ' class="sub"' : ''}>${inline(item[2])}</li>`);
    } else if (!raw.trim()) {
      flush();
      closeList();
    } else if (list && /^\s{2,}\S/.test(raw)) {
      // A list item that continues on the next line.
      out[out.length - 1] = out[out.length - 1].replace(/<\/li>$/, ` ${inline(raw.trim())}</li>`);
    } else {
      closeList();
      paragraph.push(raw.trim());
    }
  }
  if (code !== null) out.push(`<pre>${esc(code.join('\n'))}</pre>`);
  flush();
  closeList();
  return out.join('');
}

function releaseDate(iso) {
  if (!iso) return '';
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? '' : date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
}

function changelog(releases) {
  return releases.map((release) => `
    <section>
      <div class="release-name">${esc(release.name)} <span>${esc(releaseDate(release.date))}</span></div>
      ${release.notes.trim() ? markdown(release.notes) : '<p class="empty">No notes for this release.</p>'}
    </section>`).join('');
}

function openUpdate() {
  const update = updates.status?.update;
  if (!update || updates.installing) return;
  const canInstall = Boolean(update.setup);
  let why = `You have ${esc(state.version)}. Updating takes a few seconds and keeps your settings and history.`;
  if (update.portable) why = 'This is a portable copy, so get the new version from the release page.';
  else if (!canInstall) why = 'This release has no setup to download yet. Get it from the release page.';
  const modal = openModal(`
    <div class="modal-head">
      <div class="modal-burst"><div class="q q-happy" style="width:64px;height:64px" role="img" aria-label="Qubis, happy"></div></div>
      <div>
        <p class="eyebrow good">Update available</p>
        <h2 class="title-lg">xTiger ${esc(update.version)}</h2>
        <div class="desc">${why}</div>
      </div>
    </div>
    <div class="changelog selectable">${changelog(update.releases)}</div>
    <div class="modal-actions">
      <button class="btn btn-sm" data-act="skip">Skip this version</button>
      <span class="grow"></span>
      <button class="btn" data-act="later">Later</button>
      ${canInstall
        ? '<button class="btn btn-primary" data-act="install">Update now</button>'
        : '<button class="btn btn-primary" data-act="page">Open the release page</button>'}
    </div>`);
  $('[data-act="later"]', modal).addEventListener('click', closeModal);
  $('[data-act="skip"]', modal).addEventListener('click', async () => {
    closeModal();
    try {
      await call('skip_update', { version: update.version });
      setUpdateStatus({ ...updates.status, update: null });
      toast(`You won't hear about ${update.version} again`);
    } catch (error) {
      toast(String(error));
    }
  });
  $('[data-act="page"]', modal)?.addEventListener('click', () => {
    call('open_release_page', { url: update.url }).catch((error) => toast(String(error)));
    closeModal();
  });
  $('[data-act="install"]', modal)?.addEventListener('click', () => installUpdate(update));
}

function megabytes(bytes) {
  return (bytes / 1048576).toFixed(1);
}

async function installUpdate(update) {
  updates.installing = true;
  showUpdatePill();
  const modal = openModal(`
    <div class="update-progress">
      <div class="q q-scan" style="width:104px;height:104px" role="img" aria-label="Qubis"></div>
      <p class="eyebrow">Updating</p>
      <h2 class="title-lg">Getting xTiger ${esc(update.version)}</h2>
      <p class="lead">Qubis is fetching the new version. xTiger then closes, installs it and opens again by itself.</p>
      <div class="progress"><div class="progress-fill"></div></div>
      <div class="progress-meta"><span class="step">Connecting…</span><span class="pct">0%</span></div>
      <ol class="update-steps">
        <li class="on" data-step="download"><span class="dot"></span>Download</li>
        <li data-step="install"><span class="dot"></span>Install</li>
        <li data-step="restart"><span class="dot"></span>Restart</li>
      </ol>
    </div>`, { dismissable: false });
  const fill = $('.progress-fill', modal);
  const step = $('.progress-meta .step', modal);
  const pct = $('.progress-meta .pct', modal);
  const progress = new tauri.core.Channel();
  progress.onmessage = ({ done, total }) => {
    const percent = Math.min(100, Math.floor((done * 100) / Math.max(total, 1)));
    fill.style.width = `${percent}%`;
    pct.textContent = `${percent}%`;
    step.textContent = `${megabytes(done)} of ${megabytes(total)} MB`;
  };
  try {
    await call('install_update', { setup: update.setup, onProgress: progress });
    // The setup is running and this window is about to close.
    fill.style.width = '100%';
    pct.textContent = '100%';
    step.textContent = 'Starting the installer…';
    $('[data-step="download"]', modal).className = 'done';
    $('[data-step="install"]', modal).className = 'on';
  } catch (error) {
    updates.installing = false;
    showUpdatePill();
    $('.update-progress', modal).innerHTML = `
      <div class="q q-worried" style="width:96px;height:96px" role="img" aria-label="Qubis, worried"></div>
      <p class="eyebrow">Update failed</p>
      <h2 class="title-lg">That didn't work</h2>
      <div class="error-box selectable">${esc(String(error))}</div>`;
    modal.insertAdjacentHTML('beforeend', `
      <div class="modal-actions"><span class="grow"></span>
        <button class="btn" data-act="close">Close</button>
        <button class="btn btn-primary" data-act="retry">Try again</button>
      </div>`);
    $('#modal').dataset.dismissable = 'true';
    $('[data-act="close"]', modal).addEventListener('click', closeModal);
    $('[data-act="retry"]', modal).addEventListener('click', () => installUpdate(update));
  }
}

/* Shown once, the first time a new version opens after an update. */
async function showWhatsNew() {
  let releases = [];
  try {
    releases = await call('whats_new');
  } catch {
    return;
  }
  if (!releases.length) return;
  const modal = openModal(`
    <div class="modal-head">
      <div class="modal-burst"><div class="q q-happy" style="width:64px;height:64px" role="img" aria-label="Qubis, happy"></div></div>
      <div>
        <p class="eyebrow good">Updated</p>
        <h2 class="title-lg">What's new in xTiger ${esc(state.version)}</h2>
        <div class="desc">You're on the newest version. Here is what changed.</div>
      </div>
    </div>
    <div class="changelog selectable">${changelog(releases)}</div>
    <div class="modal-actions"><span class="grow"></span><button class="btn btn-primary" data-act="ok">Let's go</button></div>`);
  $('[data-act="ok"]', modal).addEventListener('click', closeModal);
  setTimeout(() => sparkle($('.modal-burst', modal), 22), 180);
}

/* ---------- Start ---------- */

function wireWindow() {
  const win = tauri.window.getCurrentWindow();
  $('#win-min').addEventListener('click', () => win.minimize());
  $('#win-max').addEventListener('click', () => win.toggleMaximize());
  $('#win-close').addEventListener('click', () => win.close());
  $('.titlebar').addEventListener('dblclick', (event) => {
    if (!event.target.closest('.win-btn')) win.toggleMaximize();
  });
  for (const button of $$('.rail-btn')) {
    button.addEventListener('click', () => go(button.dataset.nav === 'results' && state.run?.reports.length === 0 ? 'allclear' : button.dataset.nav));
  }
  tauri.event.listen('validate-log', ({ payload }) => {
    const v = state.validating;
    if (!v) return;
    v.log.push(payload);
    if (v.log.length > 40) v.log.shift();
    const log = $('#log');
    if (!log || state.screen !== 'validating') return;
    if (log.children.length === 1 && log.firstElementChild.textContent.startsWith('Starting')) log.innerHTML = '';
    const item = document.createElement('li');
    item.textContent = payload;
    log.append(item);
    while (log.children.length > 8) log.firstElementChild.remove();
  });
}

async function start() {
  if (!tauri) {
    // Opened in a plain browser: use sample data so the screens can be looked at.
    tauri = (await import('./dev-sample.js')).fakeTauri;
  }
  wireWindow();
  state.version = await tauri.app.getVersion().catch(() => '');
  watchUpdates();
  go('boot', { animate: false });
  state.setup = await call('get_setup');
  applyTheme();
  if (!state.setup.game) {
    go('notfound');
    return;
  }
  await loadMods();
  go(state.setup.lastMod ? 'mods' : 'welcome');
  await showWhatsNew();
}

start().catch((error) => {
  $('#screen').innerHTML = `<div class="center-stage"><div class="notice">${esc(String(error))}</div></div>`;
});
