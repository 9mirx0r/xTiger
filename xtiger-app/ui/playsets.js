'use strict';

/* The playsets screen: named lists of mods that a mod is checked and play-tested with. xTiger keeps
 * them, and the assistants edit the very same ones through `xtiger_playsets`. The launcher's own
 * playsets are never changed; one can be copied to start from. Loaded after loadorder.js, whose
 * helpers and styles it shares. */

Object.assign(icon, {
  add: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14"/><path d="M5 12h14"/></svg>',
});

Object.assign(icon, {
  trash: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16"/><path d="M9 7V4h6v3"/><path d="M6 7l1 13h10l1-13"/></svg>',
});

const ps = {
  // What `playsets_overview` answered: { ours: [{ name, notes, mods }], launcher: [...] }; null while it loads.
  overview: null,
  error: null,
  selected: null,
  search: '',
  creating: false,
  confirmDelete: false,
  queue: Promise.resolve(),
};

function psCurrent() {
  return ps.overview?.ours.find((set) => set.name === ps.selected) ?? null;
}

const psKey = (name) => String(name).trim().toLowerCase();

/* ---------- Pieces ---------- */

function psBarHtml() {
  const ours = ps.overview?.ours ?? [];
  const chips = ours.map((set) => {
    const on = set.mods.filter((item) => item.enabled).length;
    return `
      <button type="button" class="ps-chip" role="tab" data-set="${esc(set.name)}" aria-selected="${set.name === ps.selected}">
        <span class="ps-chip-name">${esc(set.name)}</span>
        <span class="ps-chip-count">${on}</span>
      </button>`;
  }).join('');
  const make = ps.creating
    ? `
      <form class="ps-new" data-act="create">
        <label class="sr-only" for="ps-new-name">Name of the new playset</label>
        <input class="field" id="ps-new-name" maxlength="80" placeholder="Name, for example Heraldry test" autocomplete="off" spellcheck="false">
        <button type="submit" class="btn btn-primary btn-sm">Create</button>
        <button type="button" class="btn btn-sm" data-act="create-cancel">Cancel</button>
      </form>`
    : `<button type="button" class="ps-chip ps-chip-new" data-act="new">${icon.add}New playset</button>`;
  return `<div class="ps-bar" role="tablist" aria-label="Your playsets">${chips}${make}</div>`;
}

function psToolsHtml() {
  const sets = ps.overview?.launcher ?? [];
  if (sets.length === 0) return '';
  const options = sets.map((set) => `<option value="${esc(set.name)}" ${set.active ? 'selected' : ''}>${esc(set.name)} (${set.enabled})</option>`).join('');
  return `
    <label class="sr-only" for="ps-launcher">Launcher playset</label>
    <select class="field" id="ps-launcher">${options}</select>
    <button type="button" class="btn" data-act="from-launcher" title="Make a playset of xTiger from this one of the Paradox launcher. The launcher's own is not changed.">${icon.layers}Copy from launcher</button>`;
}

function psEmptyHtml() {
  if (ps.error) return `<div class="lo-empty"><div class="q q-worried" role="img" aria-label="Qubis" style="width:72px;height:72px"></div><p>${esc(ps.error)}</p></div>`;
  if (!ps.overview) return '<p class="lo-loading muted">Reading your playsets…</p>';
  return `
    <div class="lo-empty ps-welcome">
      <div class="q q-confused" role="img" aria-label="Qubis" style="width:84px;height:84px"></div>
      <p><b>No playset yet.</b> Make one, then tick the mods you want to try together. Your assistant can make them too: ask it to put a few mods in a playset and test a mod with it.</p>
    </div>`;
}

const NEED_WORDS = { add: 'is not in the playset', late: 'loads after it', missing: 'is not installed' };

function psNeedTag(owed) {
  if (owed.length === 0) return '';
  const title = owed.map((need) => `${need.name}: ${NEED_WORDS[need.state]}`).join('\n');
  const more = owed.length > 1 ? ` +${owed.length - 1}` : '';
  return `<span class="lo-tag lo-tag-warn ps-need-tag" title="${esc(title)}">Needs ${esc(owed[0].name)}${more}</span>`;
}

/* One quiet line when the loaded mods are covered, otherwise a short list of what is wrong. */
function psNeedsHtml(set) {
  const loaded = set.mods.filter((item) => item.enabled && item.installed).length;
  if (loaded === 0) return '';
  const needs = set.needs ?? [];
  if (needs.length === 0 && !set.suggested) {
    return '<p class="ps-ok">Dependencies are covered and the order works.</p>';
  }
  const rows = needs.map((need) => {
    const by = need.needed_by.join(', ');
    const action = need.state === 'add'
      ? `<button type="button" class="btn btn-sm" data-act="add-need" data-mod="${esc(need.mod)}" aria-label="Add ${esc(need.name)} to the playset">Add</button>`
      : `<span class="muted">${need.state === 'missing' ? 'Not installed' : 'Loads too late'}</span>`;
    return `
      <li class="ps-need" data-state="${need.state}">
        <span class="ps-need-name" title="${esc(need.name)}">${esc(need.name)}</span>
        <span class="ps-need-by muted" title="Needed by ${esc(by)}">for ${esc(by)}</span>
        ${action}
      </li>`;
  }).join('');
  const addable = needs.filter((need) => need.state === 'add').length;
  const tools = [
    addable > 1 ? `<button type="button" class="btn btn-sm" data-act="add-all">Add all ${addable}</button>` : '',
    set.suggested ? '<button type="button" class="btn btn-sm" data-act="fix-order" title="Move each mod after the mods it needs. Nothing else changes.">Fix order</button>' : '',
  ].join('');
  return `
    <div class="ps-needs" role="group" aria-label="Dependencies">
      <div class="ps-needs-head">
        <b>${needs.length ? `${esc(plural(needs.length, 'dependency', 'dependencies'))} to look at` : 'The order needs a change'}</b>
        <span class="spacer"></span>${tools}
      </div>
      ${rows ? `<ul class="ps-need-list">${rows}</ul>` : '<p class="muted ps-needs-note">A mod loads before one it needs.</p>'}
    </div>`;
}

function psListHtml(set) {
  const items = set.mods;
  if (items.length === 0) {
    return `<div class="lo-empty"><div class="q q-confused" role="img" aria-label="Qubis" style="width:64px;height:64px"></div><p><b>${esc(set.name)} is empty.</b> Tick mods on the right. They load in the order you set here.</p></div>`;
  }
  const needsOf = (item) => (set.needs ?? []).filter((need) => need.needed_by.includes(item.name));
  const rows = items.map((item, i) => `
    <li class="lo-item ps-item" data-mod="${esc(item.mod)}" ${item.enabled ? '' : 'data-off'} ${item.installed ? '' : 'data-missing'}>
      <button type="button" class="lo-grip" aria-label="Move ${esc(item.name)}, now number ${i + 1} of ${items.length}. Drag, or use the up and down arrow keys." title="Drag to reorder">${icon.grip}</button>
      <span class="lo-pos">${i + 1}</span>
      <label class="ps-on" title="${item.enabled ? 'Loaded. Untick to leave it out without losing its place.' : 'Left out. Tick to load it again.'}">
        <input type="checkbox" data-act="toggle" ${item.enabled ? 'checked' : ''} aria-label="Load ${esc(item.name)}">
      </label>
      <span class="lo-name" title="${esc(item.name)}">${esc(item.name)}</span>
      ${item.installed ? '' : '<span class="lo-tag lo-tag-warn" title="This mod is not installed here, so a test goes on without it.">Not installed</span>'}
      ${item.enabled ? psNeedTag(needsOf(item)) : ''}
      <button type="button" class="lo-remove" data-act="remove" aria-label="Remove ${esc(item.name)} from ${esc(set.name)}" title="Remove">${icon.close}</button>
    </li>`).join('');
  return `<ol class="lo-items" aria-label="Mods of ${esc(set.name)}, first to last">${rows}</ol>`;
}

function psEditorHtml() {
  const set = psCurrent();
  if (!set) return psEmptyHtml();
  const on = set.mods.filter((item) => item.enabled).length;
  return `
    <div class="ps-head">
      <label class="sr-only" for="ps-name">Playset name</label>
      <input class="field ps-name" id="ps-name" value="${esc(set.name)}" maxlength="80" spellcheck="false" autocomplete="off">
      ${ps.confirmDelete ? `
        <span class="ps-confirm" role="alertdialog" aria-label="Delete ${esc(set.name)}?">
          <span>Delete this playset?</span>
          <button type="button" class="btn btn-sm btn-danger" data-act="delete-yes">Delete</button>
          <button type="button" class="btn btn-sm" data-act="delete-no">Keep</button>
        </span>` : `
        <button type="button" class="btn btn-sm" data-act="duplicate" title="Make a copy to try a variation">Duplicate</button>
        <button type="button" class="btn btn-sm" data-act="delete" title="Delete this playset">${icon.trash}Delete</button>`}
    </div>
    <label class="sr-only" for="ps-notes">What this playset is for</label>
    <input class="field ps-notes" id="ps-notes" value="${esc(set.notes)}" placeholder="What is this playset for? (optional)" maxlength="300" spellcheck="false" autocomplete="off">
    <p class="ps-count muted">${esc(plural(on, 'mod'))} loaded${set.mods.length > on ? `, ${set.mods.length - on} left out` : ''}. The top of the list loads first.</p>
    ${psNeedsHtml(set)}`;
}

function psPickHtml() {
  const rows = state.mods.map((mod) => {
    const meta = [mod.version ? `v${mod.version}` : null, { workshop: 'Workshop', local: 'Local', added: 'Added' }[mod.source]].filter(Boolean).join(' · ');
    return `
      <label class="lo-pick" data-name="${esc(mod.name.toLowerCase())}" data-mod="${esc(mod.modFile)}">
        <input type="checkbox" data-pick="${esc(mod.modFile)}">
        <span class="lo-thumb" style="--tint:${tint(mod.name)}" aria-hidden="true">${esc(initials(mod.name))}</span>
        <span class="lo-pick-body">
          <span class="lo-pick-name">${esc(mod.name)}</span>
          <span class="lo-pick-meta">${esc(meta)}</span>
        </span>
        <span class="lo-pick-pos" aria-hidden="true"></span>
      </label>`;
  }).join('');
  return rows || '<p class="lo-loading muted">No mod is installed.</p>';
}

/* ---------- Painting ---------- */

function paintPlaysets() {
  const bar = $('#ps-bar');
  if (!bar) return;
  bar.innerHTML = psBarHtml();
  $('#ps-tools').innerHTML = psToolsHtml();
  const set = psCurrent();
  const body = $('#ps-body');
  body.hidden = !set;
  $('#ps-empty').hidden = !!set;
  $('#ps-empty').innerHTML = set ? '' : psEmptyHtml();
  if (!set) return;
  // The name and notes boxes keep what is being typed while the list under them is painted again.
  const typing = document.activeElement?.closest?.('#ps-editor') ? document.activeElement : null;
  if (!typing) $('#ps-editor').innerHTML = psEditorHtml();
  else if (!typing.matches('#ps-name, #ps-notes')) $('#ps-editor').innerHTML = psEditorHtml();
  $('#ps-list').innerHTML = psListHtml(set);
  $('#ps-title').textContent = set.name;
  const inSet = new Map(set.mods.map((item, i) => [psKey(item.mod.replaceAll('\\', '/')), i + 1]));
  for (const row of $$('.lo-pick', $('#ps-picks'))) {
    const position = inSet.get(psKey(row.dataset.mod.replaceAll('\\', '/')));
    $('input', row).checked = position != null;
    $('.lo-pick-pos', row).textContent = position != null ? String(position) : '';
  }
}

function filterPsPicks() {
  const needle = ps.search.trim().toLowerCase();
  let shown = 0;
  for (const row of $$('.lo-pick', $('#ps-picks'))) {
    const match = !needle || row.dataset.name.includes(needle);
    row.hidden = !match;
    if (match) shown += 1;
  }
  const empty = $('#ps-pick-empty');
  if (empty) empty.hidden = shown > 0 || state.mods.length === 0;
}

/* ---------- Changes ---------- */

function toastError(error) {
  toast(String(error));
}

/* Keep `mods` (and the notes) as the playset: shown at once, saved behind it, and the answer painted
 * when it comes. Saves go one after another so a quick second change cannot be overwritten. */
function savePlayset(change, { focus = null } = {}) {
  const set = psCurrent();
  if (!set) return;
  const name = set.name;
  const next = { ...set, ...change };
  ps.overview.ours = ps.overview.ours.map((other) => (other === set ? next : other));
  paintPlaysets();
  if (focus) $$('.ps-item').find((row) => samePath(row.dataset.mod, focus))?.querySelector('.lo-grip')?.focus();
  ps.queue = ps.queue.then(async () => {
    try {
      const saved = await call('playsets_save', {
        playset: { name, notes: next.notes, mods: next.mods.map(({ name: modName, mod, enabled }) => ({ name: modName, mod, enabled })) },
      });
      ps.overview.ours = ps.overview.ours.map((other) => (other.name === name ? saved : other));
    } catch (error) {
      toastError(error);
      await loadPlaysets();
    }
    if (state.screen === 'playsets') paintPlaysets();
  });
}

const psItems = () => psCurrent()?.mods ?? [];

/* Put the mods a playset needs in it, each just before the first mod that needs it. */
function addNeeded(files) {
  const set = psCurrent();
  if (!set) return;
  const items = [...set.mods];
  for (const file of files) {
    if (items.some((item) => samePath(item.mod, file))) continue;
    const need = set.needs.find((entry) => entry.mod && samePath(entry.mod, file));
    const info = state.mods.find((entry) => samePath(entry.modFile, file));
    const before = items.findIndex((item) => need?.needed_by.includes(item.name));
    items.splice(before < 0 ? 0 : before, 0, { name: info?.name ?? need?.name ?? file, mod: file, enabled: true, installed: true });
  }
  savePlayset({ mods: items });
}

function moveInSet(mod, offset) {
  const items = [...psItems()];
  const from = items.findIndex((item) => samePath(item.mod, mod));
  const to = from + offset;
  if (from < 0 || to < 0 || to >= items.length) return;
  items.splice(to, 0, items.splice(from, 1)[0]);
  savePlayset({ mods: items }, { focus: mod });
}

/* Dragging a row by its grip: the row follows the pointer through the list, and the new order is
 * saved when it is let go. */
function startPsDrag(event, grip) {
  const row = grip.closest('.ps-item');
  const list = row?.parentElement;
  if (!row || event.button !== 0) return;
  event.preventDefault();
  list.setPointerCapture(event.pointerId);
  row.classList.add('lifted');
  const move = (e) => {
    const others = $$('.ps-item', list).filter((other) => other !== row);
    const before = others.find((other) => {
      const box = other.getBoundingClientRect();
      return e.clientY < box.top + box.height / 2;
    });
    if (row.nextElementSibling !== (before ?? null)) slide(list, () => list.insertBefore(row, before ?? null));
  };
  const end = (e, commit) => {
    list.removeEventListener('pointermove', move);
    list.removeEventListener('pointerup', up);
    list.removeEventListener('pointercancel', cancel);
    if (list.hasPointerCapture(e.pointerId)) list.releasePointerCapture(e.pointerId);
    row.classList.remove('lifted');
    if (!commit) {
      paintPlaysets();
      return;
    }
    const order = $$('.ps-item', list).map((item) => item.dataset.mod);
    const items = psItems();
    const next = order.map((mod) => items.find((item) => samePath(item.mod, mod)));
    if (next.some((item, i) => item !== items[i])) savePlayset({ mods: next }, { focus: row.dataset.mod });
  };
  const up = (e) => end(e, true);
  const cancel = (e) => end(e, false);
  list.addEventListener('pointermove', move);
  list.addEventListener('pointerup', up);
  list.addEventListener('pointercancel', cancel);
}

async function loadPlaysets() {
  try {
    ps.overview = await call('playsets_overview');
    ps.error = null;
    if (!psCurrent()) ps.selected = ps.overview.ours[0]?.name ?? null;
  } catch (error) {
    ps.error = String(error);
  }
  if (state.screen === 'playsets') paintPlaysets();
}

/* A playset of ours with this name, taking the next free name when it is already used. */
function freePlaysetName(wanted) {
  const used = new Set((ps.overview?.ours ?? []).map((set) => psKey(set.name)));
  if (!used.has(psKey(wanted))) return wanted;
  for (let n = 2; ; n += 1) {
    const name = `${wanted} ${n}`;
    if (!used.has(psKey(name))) return name;
  }
}

async function createPlayset(name, from = null) {
  await ps.queue;
  try {
    const made = await call('playsets_create', { name, from });
    ps.overview.ours = [...ps.overview.ours, made].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
    ps.selected = made.name;
    ps.creating = false;
    ps.confirmDelete = false;
    toast(from ? `${made.name} made from ${from}` : `${made.name} made`);
  } catch (error) {
    toastError(error);
  }
  if (state.screen === 'playsets') paintPlaysets();
}

/* ---------- The screen ---------- */

SCREENS.playsets = {
  html() {
    return `
      <div class="page lo-page ps-page">
        <div class="page-head">
          <div class="lo-title">
            <h1 class="title-lg">Playsets</h1>
            <span class="muted lo-for">mods to test together</span>
          </div>
          <span class="spacer"></span>
          <div class="lo-tools" id="ps-tools"></div>
        </div>
        <p class="lo-lead">A playset is a named list of mods that a check or a game test can load together, in order. You and your AI assistant edit the same ones: ask it to “test my mod with the playset ${esc(ps.selected ?? 'Heraldry')}”.</p>
        <div id="ps-bar"></div>
        <div id="ps-empty" hidden></div>
        <div id="ps-body" hidden>
          <div class="ps-editor" id="ps-editor"></div>
          <div class="lo-grid">
            <section class="card lo-card" aria-labelledby="ps-title">
              <header class="lo-card-head">
                <h2 id="ps-title"></h2>
              </header>
              <div class="lo-scroll" id="ps-list"></div>
            </section>
            <section class="card lo-card" aria-labelledby="ps-h-pick">
              <header class="lo-card-head">
                <h2 id="ps-h-pick">Installed mods</h2>
                <label class="search lo-search">
                  <span class="sr-only">Search installed mods</span>
                  ${icon.search}
                  <input class="field" type="search" placeholder="Search mods" value="${esc(ps.search)}" data-act="search" spellcheck="false">
                </label>
              </header>
              <div class="lo-scroll lo-picks" id="ps-picks">
                ${psPickHtml()}
                <p class="lo-loading muted" id="ps-pick-empty" hidden>No mod matches.</p>
              </div>
            </section>
          </div>
        </div>
      </div>`;
  },
  bind(root) {
    const bar = $('#ps-bar', root);
    bar.addEventListener('click', (event) => {
      const chip = event.target.closest('[data-set]');
      if (chip) {
        ps.selected = chip.dataset.set;
        ps.confirmDelete = false;
        paintPlaysets();
        return;
      }
      if (event.target.closest('[data-act="new"]')) {
        ps.creating = true;
        paintPlaysets();
        $('#ps-new-name')?.focus();
      } else if (event.target.closest('[data-act="create-cancel"]')) {
        ps.creating = false;
        paintPlaysets();
      }
    });
    bar.addEventListener('submit', (event) => {
      event.preventDefault();
      const name = $('#ps-new-name').value.trim();
      if (!name) {
        $('#ps-new-name').focus();
        return;
      }
      createPlayset(name);
    });
    bar.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && ps.creating) {
        ps.creating = false;
        paintPlaysets();
      }
    });
    $('#ps-tools', root).addEventListener('click', (event) => {
      if (!event.target.closest('[data-act="from-launcher"]')) return;
      const from = $('#ps-launcher').value;
      createPlayset(freePlaysetName(from), from);
    });
    // The empty state sits outside the bar; its text has no buttons, the bar has "New playset".
    const editor = $('#ps-editor', root);
    editor.addEventListener('change', async (event) => {
      const set = psCurrent();
      if (!set) return;
      if (event.target.id === 'ps-notes') {
        savePlayset({ notes: event.target.value.trim() });
      } else if (event.target.id === 'ps-name') {
        const input = event.target;
        const name = input.value.trim();
        if (!name || name === set.name) {
          input.value = set.name;
          return;
        }
        await ps.queue;
        try {
          const renamed = await call('playsets_rename', { name: set.name, newName: name });
          ps.overview.ours = ps.overview.ours.map((other) => (other.name === set.name ? renamed : other))
            .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
          ps.selected = renamed.name;
        } catch (error) {
          toastError(error);
          input.value = set.name;
        }
        paintPlaysets();
      }
    });
    editor.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' && event.target.matches('#ps-name, #ps-notes')) event.target.blur();
    });
    editor.addEventListener('click', async (event) => {
      const set = psCurrent();
      const act = event.target.closest('[data-act]')?.dataset.act;
      if (!set || !act) return;
      if (act === 'duplicate') {
        await createPlayset(freePlaysetName(`${set.name} copy`), set.name);
      } else if (act === 'delete') {
        ps.confirmDelete = true;
        paintPlaysets();
        $('[data-act="delete-no"]', editor)?.focus();
      } else if (act === 'delete-no') {
        ps.confirmDelete = false;
        paintPlaysets();
      } else if (act === 'add-need') {
        addNeeded([event.target.closest('[data-act]').dataset.mod]);
      } else if (act === 'add-all') {
        addNeeded(set.needs.filter((need) => need.state === 'add').map((need) => need.mod));
      } else if (act === 'fix-order') {
        const order = (set.suggested ?? []).map((file) => psItems().find((item) => samePath(item.mod, file))).filter(Boolean);
        // The mods that are switched off or cannot be found keep their places among the others.
        const loaded = new Set(order);
        const slots = psItems().map((item, i) => [item, i]).filter(([item]) => loaded.has(item)).map(([, i]) => i);
        const next = [...psItems()];
        slots.forEach((slot, n) => { next[slot] = order[n]; });
        if (order.length === slots.length) savePlayset({ mods: next });
      } else if (act === 'delete-yes') {
        ps.confirmDelete = false;
        await ps.queue;
        try {
          await call('playsets_delete', { name: set.name });
          ps.overview.ours = ps.overview.ours.filter((other) => other.name !== set.name);
          ps.selected = ps.overview.ours[0]?.name ?? null;
          toast(`${set.name} deleted`);
        } catch (error) {
          toastError(error);
        }
        paintPlaysets();
      }
    });
    editor.addEventListener('keydown', (event) => {
      if (event.key === 'Escape' && ps.confirmDelete) {
        ps.confirmDelete = false;
        paintPlaysets();
      }
    });
    const list = $('#ps-list', root);
    list.addEventListener('pointerdown', (event) => {
      const grip = event.target.closest('.lo-grip');
      if (grip) startPsDrag(event, grip);
    });
    list.addEventListener('click', (event) => {
      const remove = event.target.closest('[data-act="remove"]');
      if (!remove) return;
      const mod = remove.closest('.ps-item').dataset.mod;
      savePlayset({ mods: psItems().filter((item) => !samePath(item.mod, mod)) });
    });
    list.addEventListener('change', (event) => {
      const box = event.target.closest('[data-act="toggle"]');
      if (!box) return;
      const mod = box.closest('.ps-item').dataset.mod;
      savePlayset({ mods: psItems().map((item) => (samePath(item.mod, mod) ? { ...item, enabled: box.checked } : item)) });
    });
    list.addEventListener('keydown', (event) => {
      const grip = event.target.closest('.lo-grip');
      if (!grip || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
      event.preventDefault();
      moveInSet(grip.closest('.ps-item').dataset.mod, event.key === 'ArrowUp' ? -1 : 1);
    });
    $('#ps-picks', root).addEventListener('change', (event) => {
      const box = event.target.closest('[data-pick]');
      if (!box || !psCurrent()) return;
      const mod = state.mods.find((entry) => samePath(entry.modFile, box.dataset.pick));
      const items = psItems().filter((item) => !samePath(item.mod, box.dataset.pick));
      if (box.checked && mod) items.push({ name: mod.name, mod: mod.modFile, enabled: true, installed: true });
      savePlayset({ mods: items });
    });
    const search = $('[data-act="search"]', root);
    search.addEventListener('input', () => {
      ps.search = search.value;
      filterPsPicks();
    });
    filterPsPicks();
    paintPlaysets();
    loadPlaysets();
  },
};
