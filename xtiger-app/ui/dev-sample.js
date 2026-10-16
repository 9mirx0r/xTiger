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
  { id: 'codex', name: 'Codex', state: 'available', file: `${HOME}\\.codex\\config.toml`, otherCommand: null, after: 'Start a new Codex session; it reads its settings when it opens.', snippet: aiSnippet },
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


/* The update project of the first mod, for the update status and the notes on reports. `?tracked`
 * starts with one, `?stale` makes its latest run out of date and leaves two of its notes written
 * for code that has changed since (they need a new look). It behaves like the real records:
 * a note never removes a report. */
let sampleProject = params.has('tracked') ? { annotations: [] } : null;
const followedClean = new Set();
const lookAgain = new Set();
let seededNotes = false;

/* A note's id is the report's key, its first place and its normalised message: the fake has no
 * definitions, so the place stands for the definition that the real identity also holds. */
const normalized = (text) => String(text ?? '').split(/\s+/).join(' ').trim();
const noteId = (report) => `${report.key}-${report.locations[0].path}-${report.locations[0].linenr}-${normalized(report.message)}`;

function seedNotes() {
  seededNotes = true;
  if (!params.has('stale')) return;
  const kinds = ['intentional_override', 'unresolved_defect', 'advisory_formatting'];
  const words = ['Replaces the game event on purpose.', 'Real bug, the trait is still missing.', 'Style only, left as it is.'];
  reports.slice(0, 3).forEach((report, index) => {
    const { key, locations } = report;
    const message = normalized(report.message);
    const path = locations[0].path;
    const id = noteId(report);
    sampleProject.annotations.push({ id, kind: kinds[index], note: words[index], by: 'Claude', key, message, path, at: Date.now() - 86_400_000 });
    if (index < 2) lookAgain.add(id);
  });
}

function projectStatus(noReports = false) {
  if (!sampleProject && !noReports) return null;
  if (noReports) {
    return {
      project: { id: 'clean-1', name: 'Clean mod', mod_file: mods[1].modFile, created_at: Date.now(), annotations: [] },
      game_version_now: '1.20.0.4', game_changed: false, mod_changed: false,
      latest_run: { run_id: 'sample-clean', finished_at: Date.now() - 60_000, reports: 0, errors: 0, files: 'current' },
      triage: { diagnostics: 0, reviewed: {}, needs_review: 0, unreviewed: 0, notes_without_diagnostic: 0, examples: [] },
      update: {
        process: 'completed', static: 'clean', runtime: 'not_run', review: 'done', update_complete: false,
        blocking: ['The game has not been run with this mod (ck3_run).'],
      },
      notes: [],
    };
  }
  if (!seededNotes) seedNotes();
  const notes = sampleProject.annotations;
  const noteFor = (report) => notes.find((note) => note.id === noteId(report)) ?? null;
  const reviewed = {};
  const noted = [];
  let unreviewed = 0;
  const needsLook = [];
  const examples = [];
  for (const report of reports) {
    const note = noteFor(report);
    const first = report.locations[0];
    const spot = `${first.path}:${first.linenr} ${report.key}`;
    if (note) {
      const applies = !lookAgain.has(note.id);
      noted.push({ key: report.key, path: first.path, line: first.linenr, message: normalized(report.message), id: note.id, applies });
      if (applies) reviewed[note.kind] = (reviewed[note.kind] ?? 0) + 1;
      else {
        needsLook.push({ key: report.key, path: first.path, line: first.linenr });
        if (examples.length < 5) examples.push(spot);
      }
    } else {
      unreviewed += 1;
      if (examples.length < 5) examples.push(spot);
    }
  }
  const open = needsLook.length + unreviewed + (reviewed.unresolved_defect ?? 0);
  const errors = reports.filter((report) => report.severity === 'error' || report.severity === 'fatal').length;
  const stale = params.has('stale');
  const blocking = [];
  if (stale) blocking.push('The latest run is for other files than the ones on disk: validate again.');
  else if (errors) blocking.push(`The latest run has ${errors} errors.`);
  blocking.push('The game has not been run with this mod (ck3_run).');
  if (open) blocking.push(`${open} diagnostics are unreviewed, need a new look or are marked as defects.`);
  return {
    project: { id: 'silk-road-events-1', name: 'Silk Road Events', mod_file: mods[0].modFile, created_at: Date.now() - 3_600_000, annotations: notes },
    game_version_now: '1.20.0.4', game_changed: false, mod_changed: false,
    latest_run: { run_id: 'sample-silk', finished_at: Date.now() - 60_000, reports: reports.length, errors, files: stale ? 'stale' : 'current' },
    triage: { diagnostics: reports.length, reviewed, needs_review: needsLook.length, needs_look: needsLook, noted, unreviewed, notes_without_diagnostic: 0, examples },
    update: {
      process: 'completed', static: stale ? 'stale' : errors ? 'errors' : 'clean', runtime: 'not_run',
      review: open ? 'pending' : 'done', update_complete: false, blocking,
    },
    notes: [],
  };
}

function projectAnnotate({ key, path, line, kind, note, message = null }) {
  if (!sampleProject) throw new Error('This mod has no update to follow yet. Start following it first.');
  if (!note.trim()) throw new Error('Say why in `note`: it is kept with the project.');
  // With a message, only that one diagnostic gets the note, even when another shares its line.
  const picked = message == null
    ? reports.filter((report) => report.key === key
      && report.locations.some((place) => place.path.includes(path) && (line === 0 || place.linenr === line)))
    : reports.filter((report) => report.key === key && normalized(report.message) === normalized(message)
      && report.locations[0].path === path && report.locations[0].linenr === line);
  if (picked.length === 0) throw new Error(message == null
    ? 'No diagnostic of the latest run matches that key, path and line.'
    : 'No diagnostic with that message is at that place. Validate the mod again.');
  if (picked.length > 25) throw new Error(`${picked.length} diagnostics match: narrow it with path or line (at most 25 at a time).`);
  for (const report of picked) {
    const id = noteId(report);
    sampleProject.annotations = sampleProject.annotations.filter((entry) => entry.id !== id);
    lookAgain.delete(id);
    sampleProject.annotations.push({ id, kind, note: note.trim(), by: 'xTiger app', key: report.key, message: normalized(report.message), path: report.locations[0].path, at: Date.now() });
  }
  return { annotated: picked.length, kind, status: projectStatus() };
}

// Saved load orders by mod file, as the app keeps them. The first mod starts with a list so the screen
// has something to show; ?emptyorder starts it empty, ?lostmod adds a mod that is no longer installed.
const loadOrders = new Map();
let loadOrderConf = false;
function loadOrderView(modFile) {
  const mod = mods.find((item) => item.modFile === modFile) ?? mods[0];
  if (!loadOrders.has(mod.modFile)) {
    const start = mod === mods[0] && !params.has('emptyorder') ? [mods[3], mods[1]] : [];
    const list = start.map((item) => ({ name: item.name, mod: item.modFile, installed: true }));
    if (mod === mods[0] && params.has('lostmod')) list.push({ name: 'Ancient Banners', mod: `${DOCS}\\mod\\ancient_banners.mod`, installed: false });
    loadOrders.set(mod.modFile, list);
  }
  const saved = loadOrders.get(mod.modFile);
  const willLoad = saved.filter((item) => item.installed).map((item) => ({ name: item.name, mod: item.mod, why: 'in your load order' }));
  const needed = willLoad.some((item) => item.name === 'Faith Expanded') && !willLoad.some((item) => item.name === 'Coat of Arms Pack');
  if (mod === mods[0] && needed && !params.has('nodeps')) {
    willLoad.unshift({ name: 'Coat of Arms Pack', mod: mods[4].modFile, why: 'needed by Faith Expanded' });
  }
  // What the descriptors name: Faith Expanded builds on a pack that is installed and on one that is not.
  const dependencies = [];
  if (mod === mods[0] && saved.some((item) => item.name === 'Faith Expanded') && !params.has('nodeps')) {
    const coat = saved.some((item) => item.name === 'Coat of Arms Pack');
    dependencies.push({ name: 'Coat of Arms Pack', needed_by: 'Faith Expanded', state: coat ? 'listed' : 'automatic', mod: mods[4].modFile });
    dependencies.push({ name: 'Heraldry Core', needed_by: 'Faith Expanded', state: 'missing' });
  }
  return {
    mod: mod.modFile,
    name: mod.name,
    saved: saved.map((item) => ({ ...item })),
    willLoad,
    dependencies,
    unresolved: saved.filter((item) => !item.installed).map((item) => ({ name: item.name, reason: 'it is not installed' })),
    warnings: params.has('warnorder') ? ['Faith Expanded asks to load after Better Courts.'] : [],
    conf: { path: `${mod.dir}\\ck3-tiger.conf`, exists: loadOrderConf, hasBlock: loadOrderConf && saved.length > 0 },
  };
}

// The playsets xTiger keeps, as the app shows them. ?noplaysets starts with none.
const ownPlaysets = params.has('noplaysets') ? [] : [
  { name: 'Heraldry test', notes: 'Patch for the coat of arms mods', mods: [mods[4], mods[3], mods[1]].map((item) => ({ name: item.name, mod: item.modFile, enabled: item.name !== 'Better Courts', installed: true })) },
  { name: 'Everything', notes: '', mods: [mods[2], mods[5]].map((item) => ({ name: item.name, mod: item.modFile, enabled: true, installed: true })) },
];
const sameName = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();
const sortedPlaysets = () => ownPlaysets.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
const launcherPlaysets = [{ name: 'Main playset', active: true, mods: 16, enabled: 14 }, { name: 'Vanilla plus', active: false, mods: 3, enabled: 3 }];

const commands = {
  loadorder_get: ({ modFile }) => loadOrderView(modFile),
  loadorder_save: ({ modFile, requires }) => {
    loadOrderView(modFile);
    loadOrders.set(modFile, requires.map((entry) => ({ name: entry.name, mod: entry.mod, installed: mods.some((item) => item.modFile === entry.mod) })));
    if (loadOrderConf && requires.length === 0) loadOrderConf = false;
    return loadOrderView(modFile);
  },
  loadorder_playsets: () => [...ownPlaysets.map((set) => ({ name: set.name, source: 'xtiger', active: false, enabled: set.mods.filter((item) => item.enabled).length })), ...launcherPlaysets.map((set) => ({ name: set.name, source: 'launcher', active: set.active, enabled: set.enabled }))],
  loadorder_from_playset: ({ modFile }) => {
    loadOrders.set(modFile, [mods[2], mods[5], mods[1]].filter((item) => item.modFile !== modFile).map((item) => ({ name: item.name, mod: item.modFile, installed: true })));
    return loadOrderView(modFile);
  },
  loadorder_export_conf: ({ modFile }) => {
    loadOrderView(modFile);
    loadOrderConf = loadOrders.get(modFile).length > 0;
    return loadOrderView(modFile);
  },
  playsets_overview: () => ({ ours: sortedPlaysets().map((set) => ({ ...set })), launcher: launcherPlaysets }),
  playsets_save: ({ playset }) => {
    const at = ownPlaysets.findIndex((set) => sameName(set.name, playset.name));
    const saved = { name: playset.name, notes: playset.notes, mods: playset.mods.map((item) => ({ ...item, installed: mods.some((known) => known.modFile === item.mod) })) };
    if (at < 0) ownPlaysets.push(saved);
    else ownPlaysets[at] = saved;
    return saved;
  },
  playsets_create: ({ name, from }) => {
    if (ownPlaysets.some((set) => sameName(set.name, name))) throw new Error(`A playset called ${name} already exists.`);
    const source = from ? ownPlaysets.find((set) => sameName(set.name, from)) : null;
    const copied = source ? source.mods : from ? [mods[2], mods[5], mods[1]].map((item) => ({ name: item.name, mod: item.modFile, enabled: true, installed: true })) : [];
    const made = { name, notes: '', mods: copied.map((item) => ({ ...item })) };
    ownPlaysets.push(made);
    return made;
  },
  playsets_rename: ({ name, newName }) => {
    const set = ownPlaysets.find((item) => sameName(item.name, name));
    if (ownPlaysets.some((item) => item !== set && sameName(item.name, newName))) throw new Error(`A playset called ${newName} already exists.`);
    set.name = newName;
    return set;
  },
  playsets_delete: ({ name }) => {
    const at = ownPlaysets.findIndex((set) => sameName(set.name, name));
    if (at >= 0) ownPlaysets.splice(at, 1);
  },
  get_setup: () => ({ game: { path: GAME, version: '1.20.0.4' }, paradox: DOCS, hasVscode: true, openInEditor: true, theme: 'system', checkUpdates: true, asciiBackground: !params.has('noascii'), startOnHome: !params.has('nohome'), gameTestsBackground: true, lastMod: params.has('welcome') ? null : mods[0].modFile }),
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
  // Only the first mod has a project, as in the real records; any other mod follows with no reports.
  project_status: ({ modFile }) => {
    if (modFile === mods[0].modFile) return projectStatus();
    return followedClean.has(modFile) ? projectStatus(true) : null;
  },
  project_track: ({ modFile }) => {
    if (modFile !== mods[0].modFile) {
      followedClean.add(modFile);
      return projectStatus(true);
    }
    sampleProject ??= { annotations: [] };
    return projectStatus();
  },
  project_annotate: projectAnnotate,
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
