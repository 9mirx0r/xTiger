'use strict';

/* The load order screen: the mods that must load before a mod when it is checked or play-tested,
 * in game order. The list is kept per mod, in the records the assistants share (`xtiger_loadorder`).
 * It is loaded after app.js and uses its helpers. */

const lo = {
  modFile: null,
  // What `loadorder_get` answered; null while it loads.
  view: null,
  error: null,
  playsets: [],
  search: '',
  exportOpen: false,
  // Saves go one after another so a quick second change cannot be overwritten by the first.
  queue: Promise.resolve(),
};

Object.assign(icon, {
  grip: '<svg width="14" height="18" viewBox="0 0 14 18" fill="currentColor" aria-hidden="true"><circle cx="4" cy="3.5" r="1.5"/><circle cx="10" cy="3.5" r="1.5"/><circle cx="4" cy="9" r="1.5"/><circle cx="10" cy="9" r="1.5"/><circle cx="4" cy="14.5" r="1.5"/><circle cx="10" cy="14.5" r="1.5"/></svg>',
  back: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M19 12H5"/><path d="M11 6l-6 6 6 6"/></svg>',
  layers: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3.5l9 4.5-9 4.5L3 8z"/><path d="M3 12.5l9 4.5 9-4.5"/><path d="M3 16.5l9 4.5 9-4.5"/></svg>',
});

const samePath = (a, b) => String(a ?? '').replaceAll('\\', '/').toLowerCase() === String(b ?? '').replaceAll('\\', '/').toLowerCase();

function loMod() {
  return state.mods.find((mod) => samePath(mod.modFile, lo.modFile)) ?? null;
}

function openLoadOrder(mod) {
  if (!mod) return;
  lo.modFile = mod.modFile;
  lo.view = null;
  lo.error = null;
  lo.search = '';
  go('loadorder');
}

function loName() {
  return lo.view?.name ?? loMod()?.name ?? 'this mod';
}

/* ---------- Pieces ---------- */

function loListHtml() {
  if (lo.error) return `<div class="lo-empty"><div class="q q-worried" role="img" aria-label="Qubis" style="width:72px;height:72px"></div><p>${esc(lo.error)}</p></div>`;
  if (!lo.view) return '<p class="lo-loading muted">Reading the saved order…</p>';
  const items = lo.view.saved;
  const self = `
    <li class="lo-item lo-self">
      <span class="lo-pos">${items.length + 1}</span>
      <span class="lo-name">${esc(loName())}</span>
      <span class="lo-tag lo-tag-self">Your mod, loads last</span>
    </li>`;
  if (items.length === 0) {
    return `
      <div class="lo-empty">
        <div class="q q-confused" role="img" aria-label="Qubis" style="width:72px;height:72px"></div>
        <p><b>Nothing loads before ${esc(loName())}.</b> Tick the mods it needs on the right. They load in the order you set here.</p>
      </div>
      <ol class="lo-items lo-items-self">${self}</ol>`;
  }
  const rows = items.map((item, i) => `
    <li class="lo-item" data-mod="${esc(item.mod)}" ${item.installed ? '' : 'data-missing'}>
      <button type="button" class="lo-grip" aria-label="Move ${esc(item.name)}, now number ${i + 1} of ${items.length}. Drag, or use the up and down arrow keys." title="Drag to reorder">${icon.grip}</button>
      <span class="lo-pos">${i + 1}</span>
      <span class="lo-name" title="${esc(item.name)}">${esc(item.name)}</span>
      ${item.installed ? '' : '<span class="lo-tag lo-tag-warn" title="This mod is not installed here, so a test goes on without it.">Not installed</span>'}
      <button type="button" class="lo-remove" data-act="remove" aria-label="Remove ${esc(item.name)} from the load order" title="Remove">${icon.close}</button>
    </li>`).join('');
  return `<ol class="lo-items" aria-label="Mods that load before ${esc(loName())}, first to last">${rows}${self}</ol>`;
}

function loPickHtml() {
  const main = loMod();
  const needs = new Set((main?.dependencies ?? []).map((name) => name.toLowerCase()));
  const rows = state.mods.filter((mod) => !samePath(mod.modFile, lo.modFile)).map((mod) => {
    const meta = [mod.version ? `v${mod.version}` : null, { workshop: 'Workshop', local: 'Local', added: 'Added' }[mod.source]].filter(Boolean).join(' · ');
    return `
      <label class="lo-pick" data-name="${esc(mod.name.toLowerCase())}" data-mod="${esc(mod.modFile)}">
        <input type="checkbox" data-pick="${esc(mod.modFile)}">
        <span class="lo-thumb" style="--tint:${tint(mod.name)}" aria-hidden="true">${esc(initials(mod.name))}</span>
        <span class="lo-pick-body">
          <span class="lo-pick-name">${esc(mod.name)}</span>
          <span class="lo-pick-meta">${esc(meta)}${needs.has(mod.name.toLowerCase()) ? ' · named in its descriptor' : ''}</span>
        </span>
        <span class="lo-pick-pos" aria-hidden="true"></span>
      </label>`;
  }).join('');
  return rows || '<p class="lo-loading muted">No other mod is installed.</p>';
}

function loPreviewHtml() {
  const view = lo.view;
  if (!view) return '';
  const saved = new Set(view.saved.map((item) => item.mod.replaceAll('\\', '/').toLowerCase()));
  const chain = view.willLoad.map((item) => {
    const extra = !saved.has(item.mod.replaceAll('\\', '/').toLowerCase());
    return `
      <li class="lo-chip" ${extra ? 'data-extra' : ''} title="${esc(item.why)}">
        <span class="lo-chip-name">${esc(item.name)}</span>
        ${extra ? `<span class="lo-chip-why">${esc(item.why)}</span>` : ''}
      </li>`;
  }).join('');
  // The dependencies block already says which ones are not installed.
  const absent = new Set((view.dependencies ?? []).filter((dep) => dep.state === 'missing').map((dep) => dep.name.toLowerCase()));
  const notes = [
    ...view.unresolved.filter((item) => !absent.has(item.name.toLowerCase())).map((item) => `${item.name} is not loaded: ${item.reason}.`),
    ...view.warnings,
  ];
  const sentence = view.willLoad.length === 0
    ? `${esc(view.name)} is tested on its own.`
    : `${esc(plural(view.willLoad.length, 'mod'))} load before ${esc(view.name)}, in this order.`;
  return `
    <h2 class="lo-h">What the next test loads</h2>
    <p class="lo-sub">${sentence} Mods listed later win when two change the same file.</p>
    <ol class="lo-chain" aria-label="Load order of the next test">
      ${chain}
      <li class="lo-chip lo-chip-self"><span class="lo-chip-name">${esc(view.name)}</span><span class="lo-chip-why">your mod</span></li>
    </ol>
    ${notes.length ? `
      <div class="lo-notes" role="status">
        <div class="q q-worried" role="img" aria-label="Qubis" style="width:40px;height:40px"></div>
        <ul>${notes.map((note) => `<li>${esc(note)}</li>`).join('')}</ul>
      </div>` : ''}`;
}

/* The mods that descriptors name as needed: in the list, loaded on their own, or not installed. */
function loDepsHtml() {
  const deps = lo.view?.dependencies ?? [];
  if (deps.length === 0) return '';
  const lacking = deps.filter((dep) => dep.state === 'missing').length;
  const loose = deps.filter((dep) => dep.state === 'automatic');
  const sentence = lacking > 0
    ? `${plural(lacking, 'mod')} that a descriptor names ${lacking === 1 ? 'is' : 'are'} not installed here. Install ${lacking === 1 ? 'it' : 'them'} (from the Workshop, or add the folder in Settings) and the next test loads ${lacking === 1 ? 'it' : 'them'} too.`
    : 'Every mod that a descriptor names is installed.';
  const tag = {
    listed: '<span class="lo-tag lo-tag-ok">In the list</span>',
    automatic: '<span class="lo-tag lo-tag-self">Loads on its own</span>',
    configured: '<span class="lo-tag lo-tag-ok">Loaded by ck3-tiger.conf</span>',
    missing: '<span class="lo-tag lo-tag-warn">Not installed</span>',
  };
  const rows = deps.map((dep) => `
    <li class="lo-dep" data-state="${esc(dep.state)}">
      <span class="lo-dep-body">
        <span class="lo-dep-name">${esc(dep.name)}</span>
        <span class="lo-dep-by">needed by ${esc(dep.needed_by)}</span>
      </span>
      ${tag[dep.state] ?? ''}
      ${dep.state === 'automatic' ? `<button type="button" class="btn btn-sm" data-act="dep-add" data-mod="${esc(dep.mod)}" data-name="${esc(dep.name)}" title="Keep ${esc(dep.name)} in the load order yourself, so you can move it">Add to list</button>` : ''}
    </li>`).join('');
  return `
    <h2 class="lo-h">Dependencies</h2>
    <p class="lo-sub">${sentence}${loose.length > 0 ? ' Mods that load on their own come before the mod that needs them.' : ''}</p>
    <ul class="lo-deps" aria-label="Dependencies named by the descriptors">${rows}</ul>`;
}

/* The mods an export would write: everything the next test loads, minus what the config loads itself.
 * Dependencies that load on their own count, so an empty list can still leave lines. */
function loExportCount(view) {
  return view.willLoad.filter((item) => !item.why.startsWith('loaded by the config')).length;
}

function loExportHtml() {
  const view = lo.view;
  if (!view) return '';
  const conf = view.conf;
  const done = conf.hasBlock;
  const none = loExportCount(view) === 0;
  return `
    <details class="lo-export" ${lo.exportOpen ? 'open' : ''}>
      <summary>Use this order outside xTiger ${done ? '<span class="lo-tag lo-tag-ok">Written</span>' : ''}</summary>
      <div class="lo-export-body">
        <p>The plain validator and CI only know what <span class="mono">ck3-tiger.conf</span> says. xTiger can write this order there as <span class="mono">load_mod</span> lines between two marker comments, and takes them out again when the list is empty. The old file is kept next to it as <span class="mono">ck3-tiger.conf.xtiger-backup</span>.</p>
        <p class="lo-warn"><b>The lines hold paths from this computer.</b> Keep that file out of anything you publish or commit.</p>
        <div class="lo-export-row">
          <button type="button" class="btn btn-sm" data-act="export" ${none && !done ? 'disabled' : ''}>${done ? (none ? 'Remove the lines' : 'Update ck3-tiger.conf') : 'Write ck3-tiger.conf'}</button>
          <span class="path selectable" title="${esc(shortPath(conf.path))}">${esc(shortPath(conf.path))}</span>
        </div>
      </div>
    </details>`;
}

function loPlaysetsHtml() {
  if (lo.playsets.length === 0) return '';
  const options = lo.playsets.map((set) => `<option value="${esc(set.name)}" ${set.active ? 'selected' : ''}>${esc(set.name)} (${set.enabled})${set.source === 'xtiger' ? ' · xTiger' : ''}</option>`).join('');
  const replacing = (lo.view?.saved.length ?? 0) > 0;
  return `
    <label class="sr-only" for="lo-playset">Playset</label>
    <select class="field" id="lo-playset">${options}</select>
    <button type="button" class="btn" data-act="playset" title="The enabled mods of the playset that come before ${esc(loName())}${replacing ? ', in place of this list' : ''}">${icon.layers}${replacing ? 'Replace from playset' : 'Start from playset'}</button>`;
}

/* ---------- Painting ---------- */

function paintLoadOrder() {
  const list = $('#lo-list');
  if (!list) return;
  list.innerHTML = loListHtml();
  const count = $('#lo-count');
  if (count) count.textContent = lo.view ? plural(lo.view.saved.length, 'mod') : '';
  const inList = new Map((lo.view?.saved ?? []).map((item, i) => [item.mod.replaceAll('\\', '/').toLowerCase(), i + 1]));
  for (const row of $$('.lo-pick')) {
    const position = inList.get(row.dataset.mod.replaceAll('\\', '/').toLowerCase());
    $('input', row).checked = position != null;
    $('.lo-pick-pos', row).textContent = position != null ? String(position) : '';
  }
  $('#lo-preview').innerHTML = loPreviewHtml();
  $('#lo-deps').innerHTML = loDepsHtml();
  $('#lo-export').innerHTML = loExportHtml();
  $('#lo-tools').innerHTML = loPlaysetsHtml();
}

function filterPicks() {
  const needle = lo.search.trim().toLowerCase();
  let shown = 0;
  for (const row of $$('.lo-pick')) {
    const match = !needle || row.dataset.name.includes(needle);
    row.hidden = !match;
    if (match) shown += 1;
  }
  const empty = $('#lo-pick-empty');
  if (empty) empty.hidden = shown > 0 || state.mods.length < 2;
}

/* ---------- Changes ---------- */

function currentItems() {
  return (lo.view?.saved ?? []).map(({ name, mod }) => ({ name, mod }));
}

/* Keep `items` as the list: shown at once, saved behind it, and the answer (which also says what
 * a test will load now) painted when it comes. */
function saveItems(items, { focus = null } = {}) {
  const modFile = lo.modFile;
  const previous = lo.view;
  if (!previous) return;
  lo.view = { ...previous, saved: items.map((item) => ({ ...item, installed: previous.saved.find((old) => samePath(old.mod, item.mod))?.installed ?? true })) };
  paintLoadOrder();
  if (focus) $$('.lo-item').find((row) => samePath(row.dataset.mod, focus))?.querySelector('.lo-grip')?.focus();
  lo.queue = lo.queue.then(async () => {
    try {
      const view = await call('loadorder_save', { modFile, requires: items });
      if (lo.modFile === modFile) lo.view = view;
    } catch (error) {
      toast(String(error));
      lo.view = await call('loadorder_get', { modFile }).catch(() => previous);
    }
    if (lo.modFile === modFile && state.screen === 'loadorder') {
      paintLoadOrder();
      if (focus) $$('.lo-item').find((row) => samePath(row.dataset.mod, focus))?.querySelector('.lo-grip')?.focus();
    }
  });
}

function moveItem(mod, offset) {
  const items = currentItems();
  const from = items.findIndex((item) => samePath(item.mod, mod));
  const to = from + offset;
  if (from < 0 || to < 0 || to >= items.length) return;
  items.splice(to, 0, items.splice(from, 1)[0]);
  saveItems(items, { focus: mod });
}

/* Let the rows that move slide to their new place instead of jumping. */
function slide(list, change) {
  const before = new Map([...list.children].map((child) => [child, child.getBoundingClientRect().top]));
  change();
  if (reducedMotion()) return;
  for (const child of list.children) {
    const delta = before.get(child) - child.getBoundingClientRect().top;
    if (delta && !child.classList.contains('lifted')) {
      child.animate([{ transform: `translateY(${delta}px)` }, { transform: 'none' }], { duration: 180, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' });
    }
  }
}

/* Dragging a row by its grip: the row follows the pointer through the list, and the new order is
 * saved when it is let go. */
function startDrag(event, grip) {
  const row = grip.closest('.lo-item');
  const list = row?.parentElement;
  if (!row || event.button !== 0) return;
  event.preventDefault();
  list.setPointerCapture(event.pointerId);
  row.classList.add('lifted');
  const move = (e) => {
    const others = $$('.lo-item:not(.lo-self)', list).filter((other) => other !== row);
    const before = others.find((other) => {
      const box = other.getBoundingClientRect();
      return e.clientY < box.top + box.height / 2;
    });
    const anchor = before ?? $('.lo-self', list);
    if (row.nextElementSibling !== anchor) slide(list, () => list.insertBefore(row, anchor));
  };
  const end = (e, commit) => {
    list.removeEventListener('pointermove', move);
    list.removeEventListener('pointerup', up);
    list.removeEventListener('pointercancel', cancel);
    if (list.hasPointerCapture(e.pointerId)) list.releasePointerCapture(e.pointerId);
    row.classList.remove('lifted');
    if (!commit) {
      paintLoadOrder();
      return;
    }
    const order = $$('.lo-item:not(.lo-self)', list).map((item) => item.dataset.mod);
    const items = currentItems();
    const next = order.map((mod) => items.find((item) => samePath(item.mod, mod)));
    if (next.some((item, i) => item !== items[i])) saveItems(next, { focus: row.dataset.mod });
  };
  const up = (e) => end(e, true);
  const cancel = (e) => end(e, false);
  list.addEventListener('pointermove', move);
  list.addEventListener('pointerup', up);
  list.addEventListener('pointercancel', cancel);
}

async function loadLoadOrder() {
  const modFile = lo.modFile;
  try {
    const view = await call('loadorder_get', { modFile });
    if (lo.modFile !== modFile) return;
    lo.view = view;
    lo.error = null;
  } catch (error) {
    lo.error = String(error);
  }
  if (state.screen === 'loadorder') paintLoadOrder();
}

async function runWithView(button, action, done) {
  const modFile = lo.modFile;
  button.disabled = true;
  try {
    await lo.queue;
    const view = await action();
    if (lo.modFile === modFile) lo.view = view;
    toast(done);
  } catch (error) {
    toast(String(error));
  }
  if (state.screen === 'loadorder') paintLoadOrder();
}

/* ---------- The screen ---------- */

SCREENS.loadorder = {
  html() {
    const mod = loMod();
    return `
      <div class="page lo-page">
        <div class="page-head">
          <button type="button" class="btn btn-icon" data-act="back" aria-label="Back to your mods" title="Back to your mods">${icon.back}</button>
          <div class="lo-title">
            <h1 class="title-lg">Load order</h1>
            <span class="muted lo-for" title="${esc(mod?.name ?? '')}">for ${esc(mod?.name ?? 'this mod')}</span>
          </div>
          <span class="spacer"></span>
          <div class="lo-tools" id="lo-tools"></div>
        </div>
        <p class="lo-lead">Checks and game tests of this mod load these mods first, in this order, so a patch or a mod built on others is tested with them. The top of the list loads first.</p>
        <div class="lo-grid">
          <section class="card lo-card" aria-labelledby="lo-h-list">
            <header class="lo-card-head">
              <h2 id="lo-h-list">Loads before the mod</h2>
              <span class="muted" id="lo-count"></span>
            </header>
            <div class="lo-scroll" id="lo-list">${loListHtml()}</div>
          </section>
          <section class="card lo-card" aria-labelledby="lo-h-pick">
            <header class="lo-card-head">
              <h2 id="lo-h-pick">Installed mods</h2>
              <label class="search lo-search">
                <span class="sr-only">Search installed mods</span>
                ${icon.search}
                <input class="field" type="search" placeholder="Search mods" value="${esc(lo.search)}" data-act="search" spellcheck="false">
              </label>
            </header>
            <div class="lo-scroll lo-picks" id="lo-picks">
              ${loPickHtml()}
              <p class="lo-loading muted" id="lo-pick-empty" hidden>No mod matches.</p>
            </div>
          </section>
        </div>
        <section class="lo-preview" id="lo-preview" aria-live="polite"></section>
        <section class="lo-preview" id="lo-deps"></section>
        <div id="lo-export"></div>
      </div>`;
  },
  bind(root) {
    $('[data-act="back"]', root).addEventListener('click', () => go('mods'));
    const list = $('#lo-list', root);
    list.addEventListener('pointerdown', (event) => {
      const grip = event.target.closest('.lo-grip');
      if (grip) startDrag(event, grip);
    });
    list.addEventListener('click', (event) => {
      const remove = event.target.closest('[data-act="remove"]');
      if (!remove) return;
      const mod = remove.closest('.lo-item').dataset.mod;
      saveItems(currentItems().filter((item) => !samePath(item.mod, mod)));
    });
    list.addEventListener('keydown', (event) => {
      const grip = event.target.closest('.lo-grip');
      if (!grip || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
      event.preventDefault();
      moveItem(grip.closest('.lo-item').dataset.mod, event.key === 'ArrowUp' ? -1 : 1);
    });
    $('#lo-picks', root).addEventListener('change', (event) => {
      const box = event.target.closest('[data-pick]');
      if (!box || !lo.view) return;
      const mod = state.mods.find((entry) => samePath(entry.modFile, box.dataset.pick));
      const items = currentItems().filter((item) => !samePath(item.mod, box.dataset.pick));
      if (box.checked && mod) items.push({ name: mod.name, mod: mod.modFile });
      saveItems(items);
    });
    const search = $('[data-act="search"]', root);
    search.addEventListener('input', () => {
      lo.search = search.value;
      filterPicks();
    });
    $('#lo-tools', root).addEventListener('click', async (event) => {
      const button = event.target.closest('[data-act="playset"]');
      if (!button) return;
      const playset = $('#lo-playset').value;
      await runWithView(button, () => call('loadorder_from_playset', { modFile: lo.modFile, playset }), `Load order taken from ${playset}`);
    });
    $('#lo-deps', root).addEventListener('click', (event) => {
      const add = event.target.closest('[data-act="dep-add"]');
      if (!add || !lo.view) return;
      const items = currentItems().filter((item) => !samePath(item.mod, add.dataset.mod));
      items.push({ name: add.dataset.name, mod: add.dataset.mod });
      saveItems(items);
    });
    $('#lo-export', root).addEventListener('click', async (event) => {
      const button = event.target.closest('[data-act="export"]');
      if (!button) return;
      const wrote = loExportCount(lo.view) > 0;
      await runWithView(button, () => call('loadorder_export_conf', { modFile: lo.modFile }), wrote ? 'ck3-tiger.conf written' : 'The lines were removed from ck3-tiger.conf');
    });
    $('#lo-export', root).addEventListener('toggle', (event) => {
      lo.exportOpen = event.target.open;
    }, true);
    filterPicks();
    paintLoadOrder();
    if (!lo.view && !lo.error) loadLoadOrder();
    call('loadorder_playsets').then((sets) => {
      lo.playsets = sets;
      if (state.screen === 'loadorder') $('#lo-tools').innerHTML = loPlaysetsHtml();
    }).catch(() => {});
  },
};
