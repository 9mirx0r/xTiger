// The setup window: install, update or uninstall xTiger. The work is done on the Rust side
// (xtiger-app/setup); this file only shows the screens.

const tauri = window.__TAURI__ ?? (await import('./dev-sample.js')).fakeTauri;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();

const $ = (selector) => document.querySelector(selector);
const screen = $('#screen');
const closeButton = $('#win-close');
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const escape = (text) =>
  String(text).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

const arrow = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12h14"/><path d="M13 6l6 6-6 6"/></svg>`;
const sparkles = `<span class="sparkles" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>`;

const info = await invoke('info');
const choice = {
  dir: info.defaultDir ?? '',
  desktop: info.existing ? info.existing.desktop : true,
  launch: true,
  removeData: false,
};

$('#badge').textContent = `${info.version.replace('-', ' ')} · ${info.releaseName}`;
$('#win-min').addEventListener('click', () => win.minimize());
closeButton.addEventListener('click', () => win.close());

function show(html) {
  screen.innerHTML = html;
}

function busy(on) {
  closeButton.disabled = on;
}

// ---------- Installing ----------

function welcome() {
  const existing = info.existing;
  const action = !existing ? 'Install' : existing.version === info.version ? 'Reinstall' : 'Update';
  const lead = !existing
    ? 'Finds the mistakes in your Crusader Kings III mods before the game does.'
    : existing.version === info.version
      ? `xTiger ${escape(info.version)} is already installed. Install it again to repair it.`
      : `Updates xTiger ${escape(existing.version)} to <strong>${escape(info.version)}</strong>. Your settings and history stay as they are.`;
  show(`
    <p class="eyebrow">${existing ? 'Welcome back' : 'Welcome'}</p>
    <h1 class="title">xTiger</h1>
    <p class="lead">${lead}</p>
    <div class="place">
      <span class="label">Install to</span>
      <div class="path-row">
        <span class="path" title="${escape(choice.dir)}"><bdi>${escape(choice.dir)}</bdi></span>
        <button class="btn" data-act="browse">Change…</button>
      </div>
    </div>
    <label class="toggle"><input type="checkbox" class="switch" data-opt="desktop" ${choice.desktop ? 'checked' : ''}> Add a shortcut to the desktop</label>
    ${info.ready ? '' : '<div class="banner">This copy of the setup was built without xTiger inside, so it can only show its screens.</div>'}
    <span class="spacer"></span>
    <div class="actions">
      <span class="note">Free software under the GNU GPL v3.<br>No admin rights needed.</span>
      <button class="btn btn-primary btn-lg" data-act="install" ${info.ready ? '' : 'disabled'}>${action} ${arrow}</button>
    </div>`);
}

async function browse() {
  const picked = await tauri.dialog.open({ directory: true, defaultPath: choice.dir || undefined, title: 'Install xTiger to' });
  if (!picked) return;
  // Picking a general folder such as D:\Games installs into an xTiger folder inside it.
  const name = picked.split(/[\\/]/).filter(Boolean).pop() ?? '';
  choice.dir = name.toLowerCase() === 'xtiger' ? picked : `${picked.replace(/[\\/]+$/, '')}\\xTiger`;
  const path = $('.path');
  path.title = choice.dir;
  path.firstElementChild.textContent = choice.dir;
}

async function install() {
  busy(true);
  show(`
    <p class="eyebrow">Installing</p>
    <h2 class="heading">Installing xTiger…</h2>
    <div class="stage">
      <div class="q q-scan" role="img" aria-label="Qubis"></div>
      <p class="lead">Qubis is putting everything in place. This only takes a moment.</p>
    </div>
    <div class="progress-block">
      <div class="progress"><div class="progress-fill"></div></div>
      <div class="progress-meta"><span class="file">Starting…</span><span class="pct">0%</span></div>
    </div>`);

  const fill = $('.progress-fill');
  const file = $('.progress-meta .file');
  const pct = $('.progress-meta .pct');
  const progress = new tauri.core.Channel();
  progress.onmessage = ({ done, total, file: name }) => {
    const percent = Math.floor((done * 100) / Math.max(total, 1));
    fill.style.width = `${percent}%`;
    pct.textContent = `${percent}%`;
    file.textContent = name;
  };

  try {
    // Installing takes well under a second; keep the screen up long enough to be seen.
    await Promise.all([
      invoke('install', { dir: choice.dir, desktop: choice.desktop, onProgress: progress }),
      sleep(1600),
    ]);
    fill.style.width = '100%';
    pct.textContent = '100%';
    file.textContent = 'Done';
    await sleep(350);
    installed();
  } catch (error) {
    failed(error, install);
  } finally {
    busy(false);
  }
}

function installed() {
  const where = choice.desktop ? 'in the Start menu and on your desktop' : 'in the Start menu';
  show(`
    <p class="eyebrow">All done</p>
    <h2 class="heading">xTiger is ready</h2>
    <div class="stage">
      <div class="q q-happy" role="img" aria-label="Qubis, happy">${sparkles}</div>
      <p class="lead">You'll find it ${where}. Pick a mod, double-click it, and Qubis will tell you what's wrong with it.</p>
    </div>
    <label class="toggle"><input type="checkbox" class="switch" data-opt="launch" ${choice.launch ? 'checked' : ''}> Open xTiger now</label>
    <span class="spacer"></span>
    <div class="actions">
      <span class="note"></span>
      <button class="btn btn-primary btn-lg" data-act="finish">Finish</button>
    </div>`);
}

async function finish() {
  if (choice.launch) {
    try {
      await invoke('launch', { dir: choice.dir });
    } catch (error) {
      return failed(error, null);
    }
  }
  win.close();
}

// ---------- Updating from inside the app ----------

// The app downloaded this setup and closed itself. Nothing to ask: wait for it to be gone,
// install over it, and open the new version.
async function update() {
  busy(true);
  const from = info.existing ? `${escape(info.existing.version)} → ` : '';
  show(`
    <p class="eyebrow">Updating</p>
    <h2 class="heading">Updating xTiger</h2>
    <div class="stage">
      <div class="q q-scan" role="img" aria-label="Qubis"></div>
      <p class="lead">${from}<strong>${escape(info.version)}</strong>. Qubis is swapping in the new version. xTiger opens again by itself when it's done.</p>
    </div>
    <div class="progress-block">
      <div class="progress indeterminate"><div class="progress-fill"></div></div>
      <div class="progress-meta"><span class="file">Waiting for xTiger to close…</span><span class="pct"></span></div>
    </div>`);

  const bar = $('.progress');
  const fill = $('.progress-fill');
  const file = $('.progress-meta .file');
  const pct = $('.progress-meta .pct');
  const progress = new tauri.core.Channel();
  progress.onmessage = ({ done, total, file: name }) => {
    bar.classList.remove('indeterminate');
    const percent = Math.floor((done * 100) / Math.max(total, 1));
    fill.style.width = `${percent}%`;
    pct.textContent = `${percent}%`;
    file.textContent = name;
  };

  try {
    await invoke('wait_for_app', { dir: choice.dir });
    await Promise.all([
      invoke('install', { dir: choice.dir, desktop: choice.desktop, onProgress: progress }),
      sleep(1400),
    ]);
    bar.classList.remove('indeterminate');
    fill.style.width = '100%';
    pct.textContent = '100%';
    file.textContent = 'Opening xTiger…';
    $('.stage .q').className = 'q q-happy';
    $('.stage .q').innerHTML = sparkles;
    await sleep(900);
    await invoke('launch', { dir: choice.dir });
    win.close();
  } catch (error) {
    failed(error, update);
  } finally {
    busy(false);
  }
}

// ---------- Uninstalling ----------

function confirmRemoval() {
  const existing = info.existing;
  const what = existing
    ? `This removes xTiger ${escape(existing.version)} from <strong>${escape(existing.dir)}</strong>, along with its shortcuts.`
    : 'This removes xTiger and its shortcuts from this computer.';
  show(`
    <p class="eyebrow">Uninstall</p>
    <h2 class="heading">Leaving already?</h2>
    <div class="stage">
      <div class="q q-worried" role="img" aria-label="Qubis, worried"></div>
      <p class="lead">${what} Your mods are not touched.</p>
    </div>
    <label class="toggle"><input type="checkbox" class="switch danger" data-opt="removeData"> Also delete my settings and check history</label>
    <span class="spacer"></span>
    <div class="actions">
      <span class="note"></span>
      <button class="btn btn-lg" data-act="cancel">Keep it</button>
      <button class="btn btn-danger btn-lg" data-act="uninstall">Uninstall</button>
    </div>`);
}

async function uninstall() {
  busy(true);
  show(`
    <p class="eyebrow">Uninstall</p>
    <h2 class="heading">Removing xTiger…</h2>
    <div class="stage">
      <div class="q q-scan" role="img" aria-label="Qubis"></div>
      <p class="lead">Tidying up.</p>
    </div>
    <div class="progress-block"><div class="progress indeterminate"><div class="progress-fill"></div></div></div>`);
  try {
    await Promise.all([invoke('uninstall', { removeData: choice.removeData }), sleep(1200)]);
    removed();
  } catch (error) {
    failed(error, uninstall);
  } finally {
    busy(false);
  }
}

function removed() {
  show(`
    <p class="eyebrow">Uninstall</p>
    <h2 class="heading">xTiger is gone</h2>
    <div class="stage">
      <div class="q q-idle" role="img" aria-label="Qubis"></div>
      <p class="lead">Thanks for giving it a try. ${choice.removeData ? 'Your settings were deleted too.' : 'Your settings are kept in case you come back.'}</p>
    </div>
    <span class="spacer"></span>
    <div class="actions">
      <span class="note"></span>
      <button class="btn btn-primary btn-lg" data-act="close">Close</button>
    </div>`);
}

// ---------- Errors ----------

let retry = null;

function failed(error, again) {
  retry = again;
  show(`
    <p class="eyebrow">Something went wrong</p>
    <h2 class="heading">That didn't work</h2>
    <div class="stage">
      <div class="q q-worried" role="img" aria-label="Qubis, worried"></div>
      <p class="lead">Here is what went wrong. Fix it and try again.</p>
    </div>
    <div class="error-box">${escape(error)}</div>
    <span class="spacer"></span>
    <div class="actions">
      <span class="note"></span>
      <button class="btn btn-lg" data-act="close">Close</button>
      ${again ? `<button class="btn btn-primary btn-lg" data-act="retry">Try again</button>` : ''}
    </div>`);
}

// ---------- Wiring ----------

const actions = {
  browse,
  install,
  finish,
  uninstall,
  cancel: () => win.close(),
  close: () => win.close(),
  retry: () => retry?.(),
  update,
};

screen.addEventListener('click', (event) => {
  const target = event.target.closest('[data-act]');
  if (!target || target.disabled) return;
  event.preventDefault();
  actions[target.dataset.act]?.();
});

screen.addEventListener('change', (event) => {
  const option = event.target.dataset.opt;
  if (option) choice[option] = event.target.checked;
});

if (info.mode === 'uninstall') confirmRemoval();
else if (info.mode === 'update' && info.ready && choice.dir) update();
else welcome();
