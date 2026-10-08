/* Sample data for opening the UI in a plain browser, without the Rust side. The app only loads
 * this file when window.__TAURI__ is missing. Add `?welcome` to the URL for the first-start
 * screen, `?run` to validate a mod straight away, `?update` for an available update,
 * `?whatsnew` for the notes shown after an update, `?noserver` for a copy without its MCP
 * server, or `?noactivity` for an AI activity screen with nothing in it yet. */

const GAME = 'C:\\Games\\Steam\\steamapps\\common\\Crusader Kings III';
const DOCS = 'D:\\Documents\\Paradox Interactive\\Crusader Kings III';

const mods = [
  ['Silk Road Events', 'local', '1.4.2', '1.20.*', 12, 2],
  ['Better Courts', 'workshop', '2.0.1', '1.20.*', 0, 30],
  ['Northern Sagas', 'local', '0.9.0', '1.19.*', null, null],
  ['Faith Expanded', 'workshop', '3.2.0', '1.20.*', 41, 300],
  ['Coat of Arms Pack', 'workshop', '1.1.0', '1.20.*', 0, 1500],
  ['Steppe Life', 'local', '0.3.1', '1.20.*', 3, 4000],
  ['More Bookmarks', 'workshop', '1.0.0', '1.18.*', null, null],
  ['Test Sandbox', 'added', '0.0.1', '1.20.*', null, null],
].map(([name, source, version, supportedVersion, lastCount, minutesAgo]) => {
  const slug = name.toLowerCase().replace(/\W+/g, '_');
  return {
    modFile: `${DOCS}\\mod\\${slug}.mod`,
    dir: `${DOCS}\\mod\\${slug}`,
    name,
    version,
    supportedVersion,
    source,
    picture: null,
    lastCount,
    lastRun: minutesAgo == null ? null : Date.now() - minutesAgo * 60_000,
  };
});

function loc(path, linenr, column, length, line, tag = null) {
  return { path, from: 'MOD', stage: null, fullpath: `${DOCS}\\mod\\silk_road_events\\${path.replaceAll('/', '\\')}`, linenr, column, length, line, tag };
}

const reports = [
  { severity: 'error', confidence: 'strong', key: 'missing-item', message: 'trait `silk_trader` not defined in common/traits/', info: 'Check the spelling, or add the trait to common/traits/.', wiki: null, isNew: true, locations: [loc('events/silk_road_events.txt', 142, 13, 11, '\t\t\tadd_trait = silk_trader')] },
  { severity: 'error', confidence: 'reasonable', key: 'scopes', message: '`liege` is for characters but scope seems to be province', info: null, wiki: null, isNew: false, locations: [loc('events/silk_road_events.txt', 188, 4, 5, '\t\t\tliege = { add_gold = 50 }'), loc('events/silk_road_events.txt', 170, 2, 6, '\tscope:caravan_stop = {', 'scope set here')] },
  { severity: 'warning', confidence: 'reasonable', key: 'missing-localization', message: 'missing english localization key silk_road.0012.desc', info: null, wiki: null, isNew: true, locations: [loc('events/silk_road_events.txt', 201, 2, 4, '\tdesc = silk_road.0012.desc')] },
  { severity: 'warning', confidence: 'strong', key: 'unused-field', message: 'field `weight_multipler` is not used here', info: 'Maybe you meant weight_multiplier.', wiki: null, isNew: false, locations: [loc('common/decisions/silk_decisions.txt', 33, 3, 17, '\t\tweight_multipler = { base = 1 }')] },
  { severity: 'untidy', confidence: 'weak', key: 'duplicate-field', message: 'field `is_shown` is set twice in the same block', info: 'Only the second one has any effect.', wiki: null, isNew: false, locations: [loc('common/decisions/silk_decisions.txt', 58, 2, 8, '\tis_shown = {')] },
  { severity: 'tips', confidence: 'reasonable', key: 'unneeded-modifier', message: 'this `limit` is not needed because it is always true', info: null, wiki: null, isNew: false, locations: [loc('common/on_action/silk_on_actions.txt', 12, 3, 5, '\t\tlimit = { always = yes }')] },
  { severity: 'error', confidence: 'strong', key: 'localization', message: 'unknown character in localization key silk_road.title', info: null, wiki: null, isNew: false, locations: [loc('localization/english/silk_road_l_english.yml', 4, 2, 15, ' silk_road.title:0 "The Silk Road$"')] },
];

const params = new URLSearchParams(location.search);

// `?run` validates the selected mod right after start, for screenshots of the results.
if (params.has('run')) {
  setTimeout(() => document.querySelector('[data-act=validate]')?.click(), 800);
}

const listeners = new Map();

const NOTES = [
  '### Added',
  '- The app **updates itself**: it looks for a new release every hour and shows what changed.',
  '- Checks for `common/puppets` and `common/spiritual_fulfillment` (CK3 1.20).',
  '### Fixed',
  '- `holy_site` iterators now give a holy site, not a title.',
  '- Fewer false alarms on vanilla files.',
].join('\n');
const sampleUpdate = {
  version: '1.0.0-alpha.2',
  url: 'https://github.com/9mirx0r/xTiger/releases/tag/v1.0.0-alpha.2',
  portable: false,
  setup: { url: 'https://github.com/9mirx0r/xTiger/releases/download/v1.0.0-alpha.2/xTiger_1.0.0-alpha.2_x64-setup.exe', size: 12_582_912, sha256: null },
  releases: [
    { version: '1.0.0-alpha.2', name: 'xTiger 1.0 alpha 2', notes: NOTES, date: '2026-10-20T12:00:00Z', url: '' },
    { version: '1.0.0-alpha.1', name: 'xTiger 1.0 alpha 1', notes: '- Small fixes.', date: '2026-10-12T12:00:00Z', url: '' },
  ],
};

class Channel {
  onmessage = () => {};
}
let running = null;

const SERVER = 'C:\\Program Files\\xTiger\\xtiger-mcp.exe';
const HOME = 'C:\\Users\\you';
const aiSnippet = `{\n  "mcpServers": {\n    "xtiger": {\n      "command": ${JSON.stringify(SERVER)}\n    }\n  }\n}`;
const aiClients = [
  { id: 'claude-desktop', name: 'Claude Desktop', state: 'available', file: `${HOME}\\AppData\\Roaming\\Claude\\claude_desktop_config.json`, otherCommand: null, after: 'Quit Claude Desktop completely (also from the tray) and open it again.', snippet: aiSnippet },
  { id: 'claude-code', name: 'Claude Code', state: 'connected', file: `${HOME}\\.claude.json`, otherCommand: null, after: 'Start a new Claude Code session.', snippet: aiSnippet },
  { id: 'cursor', name: 'Cursor', state: 'elsewhere', file: `${HOME}\\.cursor\\mcp.json`, otherCommand: 'D:\\Tools\\xTiger 0.9\\xtiger-mcp.exe', after: 'Cursor picks it up by itself; check Settings > MCP.', snippet: aiSnippet },
  { id: 'vscode', name: 'VS Code', state: 'unreadable', file: `${HOME}\\AppData\\Roaming\\Code\\User\\mcp.json`, otherCommand: null, after: 'In VS Code, run "MCP: List Servers" and start xtiger, or reload the window.', snippet: aiSnippet },
  { id: 'windsurf', name: 'Windsurf', state: 'missing', file: null, otherCommand: null, after: "Press refresh in Windsurf's MCP panel.", snippet: aiSnippet },
];
const setAi = (id, state) => {
  const client = aiClients.find((each) => each.id === id);
  Object.assign(client, { state, otherCommand: null });
  return new Promise((resolve) => setTimeout(() => resolve({ ...client }), 500));
};

/* The AI activity journal: calls placed in time around the moment the page opened, in three work
 * sessions. Yesterday's predates session titles; this morning's updated Better Courts and was
 * wrapped up; the current one fixes Silk Road Events. A validation runs when the page opens and
 * lands in the journal a few seconds later, then a query follows. */
const opened = Date.now();
const MIN = 60_000;
const silk = { name: mods[0].name, file: mods[0].modFile };
const courts = { name: mods[1].name, file: mods[1].modFile };
const sagas = { name: mods[2].name, file: mods[2].modFile };
const VALIDATE_LINES = [
  `Using CK3 directory: ${GAME}`,
  'Tiger was made for Crusader Kings version 1.20.0.4,',
  'which matches the detected game version 1.20.0.4.',
  `Using mod directory: ${DOCS}\\mod\\silk_road_events`,
];
const COURTS_WRAP_UP = 'Updated Better Courts to 1.20: renamed six triggers, replaced the removed court grandeur modifier and fixed two localization files. Nothing is left to fix.';
const tally = (fatal, error, warning, untidy, tips) => ({ fatal, error, warning, untidy, tips });
const aiCalls = [
  // [session, start, seconds, client, tool, reason, mod, args, outcome, summary, run id, found]
  ['sample-sagas', -26 * 60 * MIN, 0.1, 'Claude Desktop', 'xtiger_status', 'Making sure xTiger can find the game before I start', null, {}, 'ok', 'Everything found'],
  ['sample-sagas', -26 * 60 * MIN + 0.5 * MIN, 0.2, 'Claude Desktop', 'xtiger_mods', 'Finding the exact name of the sagas mod', null, { filter: 'sagas' }, 'ok', '1 mod'],
  ['sample-sagas', -26 * 60 * MIN + MIN, 48, 'Claude Desktop', 'xtiger_validate', 'First look at the sagas mod after the 1.20 update', sagas, { mod_path: 'Northern Sagas' }, 'ok', '1 error, 6 warnings · first check', 'run-sagas', tally(0, 1, 6, 0, 0)],
  ['sample-sagas', -26 * 60 * MIN + 3 * MIN, 0.1, 'Claude Desktop', 'xtiger_reports', 'Seeing which kinds of problems there are', sagas, { group_by: 'key' }, 'ok', '7 reports in 4 groups by key', 'run-sagas'],
  ['sample-courts', -300 * MIN, 0.1, 'Claude Code', 'xtiger_session', 'Naming the job', null, { title: 'Update Better Courts to 1.20' }, 'ok', 'Started: Update Better Courts to 1.20'],
  ['sample-courts', -299 * MIN, 52, 'Claude Code', 'xtiger_validate', 'Seeing what the 1.20 update broke', courts, { mod_path: 'Better Courts' }, 'ok', '2 fatal, 31 errors, 8 warnings · first check', 'run-courts-1', tally(2, 31, 8, 0, 0)],
  ['sample-courts', -297 * MIN, 0.1, 'Claude Code', 'xtiger_reports', 'Grouping the problems by kind', courts, { group_by: 'key' }, 'ok', '41 reports in 7 groups by key', 'run-courts-1'],
  ['sample-courts', -270 * MIN, 47, 'Claude Code', 'xtiger_validate', 'Checking the renamed triggers', courts, { mod_path: 'Better Courts' }, 'ok', '12 errors, 6 warnings · 1 new, 24 fixed', 'run-courts-2', tally(0, 12, 6, 0, 0)],
  ['sample-courts', -262 * MIN, 0.1, 'Claude Code', 'xtiger_reports', 'Listing the modifiers the game no longer knows', courts, { key: 'missing-item' }, 'ok', '7 reports', 'run-courts-2'],
  ['sample-courts', -240 * MIN, 45, 'Claude Code', 'xtiger_validate', 'Checking the new court grandeur modifier', courts, { mod_path: 'Better Courts' }, 'ok', '4 errors, 5 warnings · 0 new, 9 fixed', 'run-courts-3', tally(0, 4, 5, 0, 0)],
  ['sample-courts', -228 * MIN, 140, 'Claude Code', 'ck3_run', 'Opening the royal court to see the new modifier', courts, { commands: ['add_court_grandeur 50'], mods: ['Better Courts'] }, 'ok', 'Played as k_france, 1 console command, screenshot taken'],
  ['sample-courts', -224 * MIN, 44, 'Claude Code', 'xtiger_validate', 'Last check before wrapping up', courts, { mod_path: 'Better Courts' }, 'ok', 'Nothing found · 9 fixed', 'run-courts-4', tally(0, 0, 0, 0, 0)],
  ['sample-courts', -222 * MIN, 0.1, 'Claude Code', 'xtiger_session', 'Wrapping up', null, { wrap_up: COURTS_WRAP_UP }, 'ok', COURTS_WRAP_UP],
  ['sample-silk', -96 * MIN, 0.1, 'Claude Code', 'xtiger_session', 'Naming the job', null, { title: 'Fix the errors in Silk Road Events' }, 'ok', 'Started: Fix the errors in Silk Road Events'],
  ['sample-silk', -95 * MIN, 41, 'Claude Code', 'xtiger_validate', 'Checking Silk Road Events before fixing anything', silk, { mod_path: 'Silk Road Events' }, 'ok', '3 errors, 3 warnings, 1 untidy, 1 tip · first check', 'run-silk-1', tally(0, 3, 3, 1, 1)],
  ['sample-silk', -93 * MIN, 0.1, 'Claude Code', 'xtiger_reports', 'Listing the errors first, they matter most', silk, { severity: 'error' }, 'ok', '3 reports', 'run-silk-1'],
  ['sample-silk', -90 * MIN, 0.1, 'Claude Code', 'xtiger_reports', 'Opening an older run to compare', null, { run_id: '20260101-120000-silk_road_events' }, 'error', 'No saved run 20260101-120000-silk_road_events. Use xtiger_runs to list them.'],
  ['sample-silk', -60 * MIN, 37, 'Claude Code', 'xtiger_validate', 'Validating again after fixing the trait and the scope', silk, { mod_path: 'Silk Road Events' }, 'ok', '3 errors, 2 warnings, 1 untidy, 1 tip · 2 new, 3 fixed', 'run-silk-2', tally(0, 3, 2, 1, 1)],
  ['sample-silk', -40 * MIN, 212, 'Claude Code', 'ck3_run', 'Firing the caravan event in the game to see it with my own eyes', silk, { commands: ['event silk_road.0012'], mods: ['Silk Road Events'] }, 'ok', 'Played as k_france, 1 console command, screenshot taken'],
  ['sample-silk', -34 * MIN, 0.3, 'Claude Code', 'ck3_logs', 'Looking for errors the event left in the game log', null, { name: 'error', pattern: 'silk' }, 'ok', 'error.log: 214 lines, 3 kept'],
  ['sample-silk', -20 * MIN, 64, 'Claude Code', 'ck3_run', 'Trying the event again as a Muslim ruler', silk, { commands: ['event silk_road.0012'], play: 'k_persia' }, 'cancelled', 'Cancelled'],
  ['sample-silk', 0, 8, 'Claude Code', 'xtiger_validate', 'Checking that the trait fix loads now', silk, { mod_path: 'Silk Road Events' }, 'ok', '2 errors, 2 warnings, 1 untidy, 1 tip · 1 fixed', 'run-silk-3', tally(0, 2, 2, 1, 1)],
  ['sample-silk', 11_000, 2, 'Claude Code', 'xtiger_reports', 'Reading the two errors that are left', silk, { severity: 'error' }, 'ok', '2 reports', 'run-silk-3'],
].map(([session, start, seconds, client, tool, reason, mod, args, outcome, summary, runId, found], i) => ({
  id: `sample-${i}`,
  tool,
  client,
  reason,
  mod,
  args,
  started_at: opened + start,
  finished_at: opened + start + seconds * 1000,
  duration_ms: seconds * 1000,
  outcome,
  summary,
  run_id: runId ?? null,
  ...(found ? { found } : {}),
  session,
}));
// Only the second Silk Road check found something new.
const old = (list) => list.map((report) => ({ ...report, isNew: false }));
const aiRuns = {
  'run-sagas': { mod: sagas, reports: old(reports.slice(1, 4)), previousCount: null, durationMs: 48_000 },
  'run-courts-1': { mod: courts, reports: old(reports), previousCount: null, durationMs: 52_000 },
  'run-courts-2': { mod: courts, reports: old(reports.slice(2)), previousCount: 41, durationMs: 47_000 },
  'run-courts-3': { mod: courts, reports: old(reports.slice(3)), previousCount: 18, durationMs: 45_000 },
  'run-courts-4': { mod: courts, reports: [], previousCount: 9, durationMs: 44_000 },
  'run-silk-1': { mod: silk, reports: old(reports), previousCount: null, durationMs: 41_000 },
  'run-silk-2': { mod: silk, reports, previousCount: 8, durationMs: 37_000 },
  'run-silk-3': { mod: silk, reports: old(reports.slice(1)), previousCount: 7, durationMs: 8000 },
};
/* What a session summary says besides the journal: [fixed, new] between the first and the last
 * check, by the last check's run, and the mod files that changed, [path, minutes after opening]. */
const SESSION_CHANGES = { 'run-courts-4': [41, 0], 'run-silk-2': [3, 2], 'run-silk-3': [3, 1] };
const SESSION_FILES = {
  'sample-courts': [
    ['common/court_positions/types/court_physician.txt', -285], ['common/court_positions/types/royal_architect.txt', -281],
    ['common/scripted_triggers/bc_triggers.txt', -276], ['common/modifiers/bc_grandeur_modifiers.txt', -252],
    ['common/court_types/bc_court_types.txt', -247], ['localization/english/bc_courts_l_english.yml', -233],
    ['localization/english/bc_positions_l_english.yml', -231], ['descriptor.mod', -223],
  ],
  'sample-silk': [['common/traits/silk_traits.txt', -75], ['events/silk_road_events.txt', -68], ['events/silk_road_events.txt', -2]],
};
const SEVERITY_ORDER = ['fatal', 'error', 'warning', 'untidy', 'tips'];

/* What the user asked the assistants for, newest first. */
const plain = (list) => list.map(({ isNew, ...report }) => report);
let requestsVersion = 0;
let aiRequests = [
  {
    id: 'req-3', kind: 'fix', created_at: opened - 3 * MIN, mod: silk, game_version: '1.20.0.4', note: 'Keep the event texts as they are.',
    reports: plain(reports.slice(0, 2)), reports_total: 2, status: 'waiting',
  },
  {
    id: 'req-2', kind: 'fix', created_at: opened - 40 * MIN, mod: sagas, game_version: '1.20.0.4', note: null,
    reports: plain(reports.slice(1, 4)), reports_total: 3, status: 'taken', taken_at: opened - 12 * MIN, taken_by: 'Claude Code',
  },
  {
    id: 'req-1', kind: 'update', created_at: opened - 300 * MIN, mod: courts, game_version: '1.20.0.4', note: null,
    reports: [], reports_total: 41, brief: '# Bring Better Courts up to CK3 1.20.0.4', status: 'done', taken_at: opened - 290 * MIN,
    taken_by: 'Claude Code', finished_at: opened - 222 * MIN, outcome: COURTS_WRAP_UP,
  },
];
if (params.has('norequests')) aiRequests = [];

function askAi({ request }) {
  const made = {
    id: `req-${Date.now()}`, kind: request.kind, created_at: Date.now(), mod: { name: request.modName, file: request.modFile },
    game_version: request.gameVersion, note: request.note, reports: request.reports.slice(0, 200), reports_total: request.reports.length,
    brief: request.brief ?? null, status: 'waiting',
  };
  aiRequests = [made, ...aiRequests];
  requestsVersion += 1;
  return made;
}

function updateBrief({ info, gameVersion, reports: found }) {
  const keys = new Map();
  for (const report of found) keys.set(report.key, (keys.get(report.key) ?? 0) + 1);
  return [
    `# Bring ${info.name} up to CK3 ${gameVersion}`,
    '',
    `- **Mod version:** ${info.version ?? 'unknown'}`,
    `- **Made for:** ${info.supportedVersion ?? 'unknown'}`,
    `- **Reports:** ${found.length}`,
    '',
    '## What the validator found',
    '',
    ...[...keys].map(([key, count]) => `- \`${key}\`: ${count}`),
    '',
    '## How to go about it',
    '',
    '1. Fix the errors first: they break things in the game.',
    '2. Validate again after each batch, and compare the runs.',
  ].join('\n');
}

function sampleRow(report) {
  const first = report.locations[0];
  return { severity: report.severity, key: report.key, message: report.message, info: report.info, where: `${first.path}:${first.linenr}`, file: first.fullpath, line: first.line.trim() };
}

function aiSession({ session }) {
  const now = Date.now();
  const calls = aiCalls.filter((entry) => entry.session === session && entry.finished_at <= now);
  if (calls.length === 0) throw new Error('That session is no longer in the journal.');
  const startedAt = calls[0].started_at;
  const lastAt = calls.at(-1).finished_at;
  const wrapUp = calls.findLast((entry) => entry.tool === 'xtiger_session' && entry.args.wrap_up)?.summary ?? null;
  const worked = new Map();
  for (const entry of calls) {
    if (!entry.mod) continue;
    if (!worked.has(entry.mod.file)) worked.set(entry.mod.file, { mod: entry.mod, passes: [] });
    if (entry.tool === 'xtiger_validate' && entry.found) worked.get(entry.mod.file).passes.push({ at: entry.finished_at, runId: entry.run_id, found: entry.found });
  }
  const touched = new Map();
  for (const [path, minutes] of SESSION_FILES[session] ?? []) {
    if (opened + minutes * MIN <= now) touched.set(path, opened + minutes * MIN);
  }
  const mods = [...worked.values()].map(({ mod, passes }) => {
    const lastRunId = passes.at(-1)?.runId ?? null;
    const left = aiRuns[lastRunId]?.reports ?? [];
    const [fixed, added] = passes.length > 1 ? SESSION_CHANGES[lastRunId] ?? [null, null] : [null, null];
    const files = [...touched].map(([path, modifiedAt]) => ({ path, fullPath: `${DOCS}\\mod\\${path.replaceAll('/', '\\')}`, modifiedAt })).sort((a, b) => b.modifiedAt - a.modifiedAt);
    return {
      mod,
      passes,
      fixed,
      new: added,
      pending: left.map(sampleRow).sort((a, b) => SEVERITY_ORDER.indexOf(a.severity) - SEVERITY_ORDER.indexOf(b.severity)).slice(0, 5),
      pendingTotal: left.length,
      lastRunId,
      files,
      filesTotal: files.length,
    };
  });
  const title = calls.findLast((entry) => entry.tool === 'xtiger_session' && entry.args.title)?.args.title ?? null;
  return { session, title, wrapUp, ended: wrapUp != null || now - lastAt > 30 * MIN, calls: calls.filter((entry) => entry.tool !== 'xtiger_session').length, startedAt, lastAt, mods };
}

function aiActivity({ stamp, requestsStamp }) {
  if (params.has('noactivity')) return { stamp: '', running: [], entries: stamp === '' ? null : [], requestsStamp: '', requests: [] };
  const now = Date.now();
  const entries = aiCalls.filter((entry) => entry.finished_at <= now).reverse();
  const running = aiCalls.filter((entry) => entry.started_at <= now && entry.finished_at > now).map((entry) => {
    const { finished_at, duration_ms, outcome, summary, ...rest } = entry;
    const step = Math.floor((now - entry.started_at) / 1500);
    const progress = entry.tool === 'xtiger_validate' ? VALIDATE_LINES[Math.min(step, VALIDATE_LINES.length - 1)] : null;
    return { ...rest, run_id: null, progress };
  });
  const newStamp = String(entries.length);
  const asked = `r${requestsVersion}`;
  return {
    stamp: newStamp,
    running,
    entries: newStamp === stamp ? null : entries,
    requestsStamp: asked,
    requests: asked === requestsStamp ? null : aiRequests,
  };
}

const commands = {
  get_setup: () => ({ game: { path: GAME, version: '1.20.0.4' }, paradox: DOCS, hasVscode: true, openInEditor: true, theme: 'system', checkUpdates: true, lastMod: params.has('welcome') ? null : mods[0].modFile }),
  list_mods: () => mods,
  mod_picture: () => null,
  last_duration: ({ modFile }) => (modFile === mods[0].modFile ? 6000 : null),
  set_preferences: () => null,
  set_game_dir: ({ path }) => ({ path, version: '1.20.0.4' }),
  set_paradox_dir: () => null,
  add_mod_folder: () => mods[7],
  remove_mod_folder: () => null,
  open_location: () => null,
  reveal: () => null,
  write_text: () => null,
  get_update: () => ({ update: params.has('update') ? sampleUpdate : null, checkedAt: Date.now() - 4 * 60_000 }),
  check_update: () => commands.get_update(),
  ai_clients: () => ({ server: params.has('noserver') ? null : SERVER, clients: aiClients.map((client) => ({ ...client })), snippet: aiSnippet }),
  connect_ai: ({ id }) => setAi(id, 'connected'),
  disconnect_ai: ({ id }) => setAi(id, 'available'),
  ai_activity: aiActivity,
  ai_session: aiSession,
  ask_ai: askAi,
  remove_request: ({ id }) => {
    aiRequests = aiRequests.filter((request) => request.id !== id);
    requestsVersion += 1;
  },
  update_brief: updateBrief,
  ai_run: ({ runId }) => {
    const run = aiRuns[runId];
    if (!run) throw new Error('That run is no longer saved: only the newest 20 are kept.');
    return { reports: run.reports, durationMs: run.durationMs, previousCount: run.previousCount, newCount: run.reports.filter((report) => report.isNew).length, modFile: run.mod.file, modName: run.mod.name, finishedAt: Date.now() - MIN };
  },
  skip_update: () => null,
  open_release_page: () => null,
  whats_new: () => (params.has('whatsnew') ? sampleUpdate.releases.slice(0, 1) : []),
  install_update: ({ setup, onProgress }) => new Promise((resolve) => {
    let done = 0;
    const timer = setInterval(() => {
      done = Math.min(setup.size, done + 700_000);
      onProgress.onmessage({ done, total: setup.size });
      if (done === setup.size) {
        clearInterval(timer);
        resolve();
      }
    }, 120);
  }),
  cancel_validation: () => {
    running?.reject('cancelled');
    running = null;
  },
  validate: ({ modFile }) => new Promise((resolve, reject) => {
    running = { reject };
    const lines = [
      `Using CK3 directory: ${GAME}`,
      'Tiger was made for Crusader Kings version 1.20.0.4,',
      'which matches the detected game version 1.20.0.4.',
      `Using mod directory: ${DOCS}\\mod\\silk_road_events`,
    ];
    lines.forEach((line, i) => setTimeout(() => listeners.get('validate-log')?.({ payload: line }), 400 + i * 500));
    setTimeout(() => {
      if (!running) return;
      running = null;
      const clean = modFile === mods[1].modFile;
      resolve({ reports: clean ? [] : reports, durationMs: 5400, previousCount: clean ? 5 : 12, newCount: clean ? 0 : 2 });
    }, 5400);
  }),
};

export const fakeTauri = {
  core: {
    Channel,
    invoke: async (cmd, args = {}) => {
      if (!(cmd in commands)) throw new Error(`no sample for ${cmd}`);
      return commands[cmd](args);
    },
  },
  app: { getVersion: async () => '1.20.0' },
  dialog: { open: async () => null, save: async () => null },
  event: { listen: async (name, callback) => listeners.set(name, callback) },
  window: { getCurrentWindow: () => ({ minimize() {}, toggleMaximize() {}, close() {} }) },
};
