/* A stand-in for the Rust side, for opening the setup in a plain browser. The setup only loads
 * this file when window.__TAURI__ is missing. URL parameters:
 *   ?update     an older version is installed     ?uninstall  the uninstaller
 *   ?fail       installing fails                  ?empty      built without anything inside
 *   ?go         presses the main button after a second
 *   ?slow       installs over twenty seconds instead of two
 *   ?from-app   started by the app's updater (--update) */

const params = new URLSearchParams(location.search);
const DIR = 'C:\\Users\\you\\AppData\\Local\\Programs\\xTiger';
const FILES = ['xTiger.exe', 'ck3-tiger.exe', 'licenses/LICENSE.txt', 'licenses/OFL-Geist.txt'];

class Channel {
  onmessage = () => {};
}

async function install(args) {
  const total = 48_000_000;
  const steps = 40;
  for (let i = 0; i <= steps; i += 1) {
    if (params.has('fail') && i === 25) throw 'Cannot write C:\\Users\\you\\AppData\\Local\\Programs\\xTiger\\ck3-tiger.exe: Access is denied. (os error 5)';
    args.onProgress.onmessage({ done: (total * i) / steps, total, file: FILES[Math.min(3, Math.floor(i / 12))] });
    await new Promise((resolve) => setTimeout(resolve, params.has('slow') ? 500 : 40));
  }
}

const commands = {
  info: () => ({
    mode: params.has('uninstall') ? 'uninstall' : params.has('from-app') ? 'update' : 'install',
    version: '1.0.0-alpha',
    releaseName: 'Frankokratia',
    ready: !params.has('empty'),
    defaultDir: DIR,
    existing: params.has('update') || params.has('uninstall') || params.has('from-app') ? { dir: DIR, version: '0.9.0', desktop: true } : null,
  }),
  install,
  uninstall: () => new Promise((resolve) => setTimeout(resolve, 600)),
  launch: () => {},
  wait_for_app: () => new Promise((resolve) => setTimeout(resolve, 1500)),
};

export const fakeTauri = {
  core: {
    Channel,
    invoke: async (command, args) => commands[command](args),
  },
  dialog: { open: async () => 'D:\\Games' },
  window: { getCurrentWindow: () => ({ minimize() {}, close() { document.body.style.opacity = 0.3; } }) },
};

if (params.has('go')) {
  setTimeout(() => document.querySelector('.btn-primary, .btn-danger')?.click(), 1000);
}
