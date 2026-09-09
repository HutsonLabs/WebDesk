'use strict';

/* Canned answers for every network call ui/app.js makes.
 *
 * This loads before app.js and replaces fetch() and WebSocket, so app.js runs
 * exactly as shipped -- same code paths, same error handling -- against data
 * that never touches a real host. Nothing here is reachable from the real
 * build: scripts/preview.py injects it, ui/ never references it.
 *
 * Adding a route: put it in ROUTES below. Anything unmatched falls through to
 * the server, which answers 501 "no mock for ..." so a miss is visible rather
 * than silent.
 */

window.PREVIEW = window.PREVIEW || {};

/* ------------------------------------------------------------- scene state */

const SCENES = {
  'sign-in': { label: 'Sign in', signedIn: false },
  'sign-in-error': { label: 'Sign in — bad password', signedIn: false, loginFails: true },
  desktop: { label: 'Desktop — Files', signedIn: true, open: ['files'] },
  terminal: { label: 'Terminal', signedIn: true, open: ['terminal'] },
  editor: { label: 'Editor', signedIn: true, open: ['editor'] },
  system: { label: 'System — up to date', signedIn: true, open: ['system'] },
  'system-outdated': { label: 'System — update available', signedIn: true, open: ['system'], update: 'behind' },
  'system-updating': { label: 'System — updating', signedIn: true, open: ['system'], update: 'running' },
  'system-failed': { label: 'System — update failed', signedIn: true, open: ['system'], update: 'failed' },
  'system-locked': { label: 'System — not an admin', signedIn: true, open: ['system'], admin: false },
  crowded: { label: 'Several windows', signedIn: true, open: ['files', 'editor', 'terminal', 'system'] },
  autohide: { label: 'Window — auto-hiding title bar', signedIn: true, open: ['terminal'], autohide: true },
  'files-denied': { label: 'Files — permission denied', signedIn: true, open: ['files'], fsError: true },
  'files-empty': { label: 'Files — empty folder', signedIn: true, open: ['files'], emptyDir: true },
  'dialog-rename': { label: 'Dialog — rename', signedIn: true, open: ['files'], dialog: 'rename' },
  'dialog-delete': { label: 'Dialog — delete', signedIn: true, open: ['files'], dialog: 'delete' },
  apps: { label: 'Apps', signedIn: true, open: ['apps'] },
  'apps-empty': { label: 'Apps — no links yet', signedIn: true, open: ['apps'], links: 'none' },
  'apps-guest': { label: 'Apps — not an admin', signedIn: true, open: ['apps'], admin: false },
  'apps-detail': { label: 'Apps — a link\'s page', signedIn: true, open: ['apps'], page: 'detail' },
};

const params = new URLSearchParams(location.search);
const saved = params.get('scene') || localStorage.getItem('wd-preview-scene') || 'desktop';
const sceneName = SCENES[saved] ? saved : 'desktop';
const scene = SCENES[sceneName];

PREVIEW.scenes = SCENES;
PREVIEW.sceneName = sceneName;
PREVIEW.scene = scene;
PREVIEW.setScene = (name) => {
  localStorage.setItem('wd-preview-scene', name);
  const url = new URL(location.href);
  url.searchParams.delete('scene');
  location.replace(url.toString());
};

/* --------------------------------------------------------------- fake data */

const USER = {
  username: 'hutson',
  home: '/home/hutson',
  admin: scene.admin !== false,
};

const HOUR = 3600;
const NOW = Math.floor(Date.now() / 1000);

const dir = (name, ago) => ({ name, kind: 'dir', mode: '755', mtime: NOW - ago });
const file = (name, size, ago, mode = '644') => ({
  name, kind: 'file', size, mode, mtime: NOW - ago,
});

/* A tree wide enough to exercise the icon set, the size and mode columns, long
   names, and the sort order the server promises (dirs first, then names). */
const TREE = {
  '/': [dir('boot', 90 * 24 * HOUR), dir('etc', 3 * 24 * HOUR), dir('home', 30 * 24 * HOUR),
        dir('opt', 12 * 24 * HOUR), dir('srv', 60 * 24 * HOUR), dir('usr', 30 * 24 * HOUR),
        dir('var', 2 * HOUR)],
  '/home': [dir('hutson', 2 * HOUR)],
  '/home/hutson': [
    dir('projects', 3 * HOUR), dir('notes', 26 * HOUR), dir('.ssh', 40 * 24 * HOUR),
    dir('Downloads', 5 * HOUR),
    file('.bashrc', 3771, 40 * 24 * HOUR),
    file('.gitignore', 118, 9 * 24 * HOUR),
    file('.vimrc', 2044, 120 * 24 * HOUR),
    file('README.md', 18254, 4 * HOUR),
    file('deploy.sh', 1312, 26 * HOUR, '755'),
    file('inventory.csv', 44210, 2 * 24 * HOUR),
    file('notes.txt', 812, 20 * 60),
    file('screenshot-2026-08-21-at-14.02.11.png', 1_842_004, 47 * HOUR),
    file('server.log', 9_120_733, 60),
    file('webdesk.service', 604, 8 * 24 * HOUR),
  ],
  '/home/hutson/projects': [
    dir('webdesk', 30 * 60),
    file('Cargo.lock', 36398, 30 * 60),
    file('Cargo.toml', 1073, 30 * 60),
    file('build.rs', 3830, 31 * 60),
    file('main.rs', 12723, 30 * 60),
    file('config.toml', 402, 6 * HOUR),
    file('docker-compose.yml', 1180, 3 * 24 * HOUR),
    file('index.html', 2044, 2 * HOUR),
    file('style.css', 6810, 2 * HOUR),
    file('app.js', 29711, 2 * HOUR),
    file('schema.sql', 8801, 5 * 24 * HOUR),
    file('LICENSE', 4633, 200 * 24 * HOUR),
  ],
  '/home/hutson/projects/webdesk': [
    dir('src', 30 * 60), dir('ui', 30 * 60),
    file('.env', 210, 9 * 24 * HOUR, '600'),
    file('Makefile', 1420, 12 * 24 * HOUR),
    file('notes-on-a-very-long-file-name-that-should-truncate-cleanly.md', 3300, HOUR),
  ],
  '/home/hutson/notes': [
    file('meeting-2026-08-20.md', 4210, 3 * 24 * HOUR),
    file('todo.md', 980, 90 * 60),
    file('reading.txt', 2200, 10 * 24 * HOUR),
  ],
  '/home/hutson/Downloads': [
    file('archive.tar.gz', 88_120_442, 8 * HOUR),
    file('report.pdf', 2_910_004, 30 * HOUR),
    file('installer.bin', 44_000_000, 70 * HOUR, '755'),
  ],
  '/home/hutson/.ssh': [
    file('authorized_keys', 1420, 40 * 24 * HOUR, '600'),
    file('id_ed25519', 464, 400 * 24 * HOUR, '600'),
    file('id_ed25519.pub', 96, 400 * 24 * HOUR),
    file('known_hosts', 8820, 2 * 24 * HOUR, '600'),
  ],
  /* Root promises these, so the path bar's completion can walk into them:
     typing "/v" offers /var, and it takes the slash before /var/lib shows. */
  '/var': [
    dir('cache', 6 * HOUR), dir('lib', 90 * 24 * HOUR), dir('log', 2 * HOUR),
    dir('spool', 30 * 24 * HOUR), dir('tmp', HOUR),
  ],
  '/var/lib': [
    dir('docker', 20 * HOUR), dir('systemd', 90 * 24 * HOUR),
    dir('webdesk', 4 * HOUR),
  ],
  '/var/log': [
    file('auth.log', 220_411, 90 * 60),
    file('syslog', 4_120_882, 120),
    file('webdesk.log', 88_204, 300),
  ],
  '/usr': [dir('bin', 30 * 24 * HOUR), dir('lib', 30 * 24 * HOUR),
           dir('local', 12 * 24 * HOUR), dir('share', 30 * 24 * HOUR)],
  '/usr/local': [dir('bin', 4 * HOUR), dir('share', 12 * 24 * HOUR)],
  '/opt': [dir('webdesk', 4 * HOUR)],
  '/srv': [dir('http', 60 * 24 * HOUR)],
  '/boot': [file('config-6.8.0', 220_411, 90 * 24 * HOUR),
            file('vmlinuz', 12_204_882, 90 * 24 * HOUR)],
  '/etc': [
    file('hostname', 12, 90 * 24 * HOUR),
    file('hosts', 220, 90 * 24 * HOUR),
    file('os-release', 402, 90 * 24 * HOUR),
    file('resolv.conf', 108, 4 * HOUR),
  ],
};

const FILES = {
  '/home/hutson/notes.txt':
    'Preview data -- nothing here is on a real disk.\n\n' +
    'The editor opens for anything matching TEXT_EXT under 2 MB; everything\n' +
    'else opens in a new tab against /api/fs/read, which the shim also answers.\n',
  '/home/hutson/README.md':
    '# WebDesk\n\nA desktop in a browser tab, served by one static Rust binary.\n\n' +
    '## Preview\n\n    scripts/preview.py\n\nSaving any file under ui/ reloads the tab.\n\n' +
    '## Notes\n\n- The editor state line tracks loading / modified / saved.\n' +
    '- Saving here is a no-op that reports success, so the saved state is reachable.\n',
  '/home/hutson/projects/webdesk/Makefile':
    'build:\n\tcargo build --release\n\nrun:\n\t./target/release/webdesk\n\npreview:\n\tscripts/preview.py\n',
};

const LONG_TEXT = Array.from(
  { length: 60 },
  (_, i) => `${String(i + 1).padStart(3, ' ')}  line of sample content for the editor to scroll`
).join('\n');

const BUILD = {
  version: '26.8.1',
  commit: '0a6f29f2c41b8ed3a9075c2b6f1e4a8d3b90cc12',
  ref: 'main',
  repo: 'HutsonLabs/WebDesk',
  built: NOW - 4 * HOUR,
};

const UPDATE_LOG = [
  '+ fetching HutsonLabs/WebDesk @ main',
  '+ 7f3a91c  Tighten the dock spacing on narrow viewports',
  '+ cargo build --release',
  '   Compiling webdesk v26.8.1',
  '    Finished `release` profile [optimized] target(s) in 1m 48s',
  '+ installing to /usr/local/bin/webdesk',
  '+ restarting webdesk.service',
].join('\n');

/* ----------------------------------------------------------------- routing */

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

const text = (body, status = 200) =>
  new Response(body, { status, headers: { 'content-type': 'text/plain' } });

const unauthorized = () => json({ error: 'not signed in' }, 401);

// Mutable across a session: signing in and out inside the preview should
// behave like the real thing without reloading.
let signedIn = scene.signedIn;
let updatePhase = scene.update || 'idle';
let updateTicks = 0;

function fsList(path) {
  if (scene.fsError && path !== USER.home) {
    return json({ error: 'permission denied' }, 403);
  }
  if (scene.emptyDir) {
    return json({ path, parent: '/home', entries: [] });
  }
  const entries = TREE[path];
  if (!entries) return json({ error: `no such directory: ${path}` }, 404);
  const parent = path === '/' ? null : path.slice(0, path.lastIndexOf('/')) || '/';
  return json({ path, parent, entries });
}

function fsRead(path) {
  return text(FILES[path] || `${path}\n\n${LONG_TEXT}\n`);
}

function systemInfo() {
  const supported = true;
  return json({
    build: BUILD,
    hostname: 'orchard',
    user: { username: USER.username, admin: USER.admin },
    updates: {
      allowed: USER.admin && supported,
      supported,
      reason: null,
      admin_groups: ['wheel', 'sudo'],
    },
  });
}

function updateStatus() {
  if (updatePhase === 'running') {
    updateTicks++;
    const phases = ['fetching', 'building', 'installing'];
    // Land on 'ok' after a few polls so the finished state is reachable
    // without waiting on a real build.
    if (updateTicks > 6) updatePhase = 'ok';
    return json({
      status: { state: 'running', phase: phases[Math.min(updateTicks - 1, 2)] },
      log: UPDATE_LOG.split('\n').slice(0, 2 + updateTicks).join('\n'),
      build: BUILD,
    });
  }
  if (updatePhase === 'failed') {
    return json({
      status: { state: 'failed', error: 'cargo build exited 101' },
      log: UPDATE_LOG.split('\n').slice(0, 4).join('\n') +
        '\nerror[E0308]: mismatched types\n  --> src/pty.rs:88:21\nerror: could not compile `webdesk`',
      build: BUILD,
    });
  }
  if (updatePhase === 'ok') {
    return json({ status: { state: 'ok' }, log: UPDATE_LOG, build: BUILD });
  }
  return json({ status: {}, log: '', build: BUILD });
}

function updateCheck() {
  if (updatePhase === 'behind' || scene.update === 'behind') {
    return json({
      comparable: true,
      behind: true,
      current: BUILD.commit,
      latest: '7f3a91cd42e8b0175a3c9f6021de4b8a7c135ee9',
      ref: 'main',
      message: 'Tighten the dock spacing on narrow viewports',
      date: new Date(Date.now() - 40 * 60 * 1000).toISOString(),
    });
  }
  return json({
    comparable: true,
    behind: false,
    current: BUILD.commit,
    latest: BUILD.commit,
    ref: 'main',
  });
}

/* ---------------------------------------------------------------- links */

/* A link is a name, a URL and an icon: an application this desk points at
   rather than one it runs. There is nothing in src/ for this file to drift from
   any more -- the catalog it used to be checked against is gone, and what a
   link says is whatever somebody typed. So these are simply four plausible
   ones, chosen to cover the states the window and the dock can be in.

   Note which is which: two open framed and one opens in a tab, and one of the
   framed ones is on http://localhost, which is the case the mixed-content rule
   lets through and every other http:// address fails. */
let LINKS = scene.links === 'none' ? [] : [
  {
    // A pasted icon -- simple-icons:jellyfin, reduced to geometry the way the
    // paste dialog reduces it. This is what a link looks like once somebody has
    // been past the ten built-in marks, and it is the state worth having in the
    // preview: nothing is fetched to draw it, here or on a real host.
    id: 'k3f9q2', name: 'Jellyfin', url: 'http://localhost:8096/', icon: 'a-box',
    glyph: {
      w: 24, h: 24,
      shapes: [{ t: 'path', a: { fill: 'currentColor', d: 'M12 .002C8.826.002-1.398 18.537.16 21.666c1.56 3.129 22.14 3.094 23.682 0S15.177 0 12 0zm7.76 18.949c-1.008 2.028-14.493 2.05-15.514 0C3.224 16.9 9.92 4.755 12.003 4.755c2.081 0 8.77 12.166 7.759 14.196zM12 9.198c-1.054 0-4.446 6.15-3.93 7.189c.518 1.04 7.348 1.027 7.86 0c.511-1.027-2.874-7.19-3.93-7.19z' } }],
    },
    open: 'frame', width: 1400, height: 900, added: NOW - 30 * 24 * HOUR,
    actor: 'hutson', scope: 'host', editable: USER.admin,
  },
  {
    id: 'm7t2wx', name: 'Grafana', url: 'https://grafana.internal.example/', icon: 'a-layout',
    open: 'frame', width: 1600, height: 1000, added: NOW - 9 * 24 * HOUR,
    actor: 'hutson', scope: 'host', editable: USER.admin,
  },
  {
    id: 'p4n8rc', name: 'Gmail', url: 'https://mail.google.com/', icon: 'a-globe',
    open: 'tab', width: 1200, height: 800, added: NOW - 2 * 24 * HOUR,
    actor: USER.username, scope: 'me', editable: true,
  },
  {
    id: 'z9k5vd', name: 'Router', url: 'http://192.168.1.1/', icon: 'a-globe',
    open: 'tab', width: 1200, height: 800, added: NOW - 4 * HOUR,
    actor: USER.username, scope: 'me', editable: true,
  },
];

/* The marks a link may name. The same list src/links.rs allows, and it is a
   list rather than "any id in the sprite" for the reason that file gives: an id
   the sprite has not got draws an empty square, silently, in somebody's dock. */
const LINK_ICONS = [
  'a-globe', 'a-box', 'a-apps', 'a-terminal', 'a-files', 'a-home', 'a-user',
  'a-layout', 'a-external', 'a-refresh',
];

/* The refusals src/links.rs makes, in the same words, because the form shows
   whatever comes back and a preview that always says yes would hide the one
   part of this feature with a security argument behind it. */
function checkLink(body) {
  const url = (body.url || '').trim();
  const lower = url.toLowerCase();
  if (!(body.name || '').trim()) return 'a name is needed';
  if (!lower.startsWith('http://') && !lower.startsWith('https://')) {
    return 'an address has to start with http:// or https://. Nothing else is accepted here ' +
           '-- a javascript:, data: or file: address in a dock tile would run in this page ' +
           'rather than open a site.';
  }
  const authority = lower.replace(/^https?:\/\//, '').split(/[/?#]/)[0];
  if (!authority) return 'that address has no host in it';
  if (authority.includes('@')) {
    return 'take the username and password out of the address. They would be stored here in ' +
           'plain text and shown back to you in the form; sign in to the site itself instead.';
  }
  if (authority === location.host || authority === location.hostname) {
    return "that is WebDesk's own address. A page from this origin cannot be framed safely " +
           'here -- point this at the application you want instead.';
  }
  if (body.icon && !LINK_ICONS.includes(body.icon)) {
    return 'that is not one of the icons this build has';
  }
  return null;
}

function newId() {
  const a = 'abcdefghijkmnpqrstuvwxyz23456789';
  return Array.from({ length: 6 }, () => a[Math.floor(Math.random() * a.length)]).join('');
}

const ROUTES = [
  ['GET', /^\/api\/me$/, () => (signedIn ? json(USER) : unauthorized())],
  ['POST', /^\/api\/login$/, (_m, _q, body) => {
    if (scene.loginFails || (body && body.password === 'wrong')) {
      return json({ error: 'invalid username or password' }, 401);
    }
    signedIn = true;
    return json({ ...USER, username: (body && body.username) || USER.username });
  }],
  ['POST', /^\/api\/logout$/, () => { signedIn = false; return json({ ok: true }); }],

  ['GET', /^\/api\/fs\/list$/, (_m, q) => fsList(q.get('path') || USER.home)],
  ['GET', /^\/api\/fs\/read$/, (_m, q) => fsRead(q.get('path') || '')],
  ['PUT', /^\/api\/fs\/write$/, () => json({ ok: true })],
  ['POST', /^\/api\/fs\/mkdir$/, () => json({ ok: true })],
  ['POST', /^\/api\/fs\/rename$/, () => json({ ok: true })],
  ['POST', /^\/api\/fs\/remove$/, () => json({ ok: true })],

  ['GET', /^\/api\/system\/info$/, () => (signedIn ? systemInfo() : unauthorized())],
  ['GET', /^\/api\/update\/status$/, () => (signedIn ? updateStatus() : unauthorized())],
  ['POST', /^\/api\/update\/check$/, () => updateCheck()],
  ['POST', /^\/api\/update\/apply$/, () => {
    updatePhase = 'running';
    updateTicks = 0;
    return json({ ok: true });
  }],

  /* Links. The store is this array; a real host keeps host-wide ones in
     /var/lib/webdesk/links.json and each person's own in
     ~/.config/webdesk/links.json, written through the privilege-dropping
     helper -- see src/links.rs. */
  ['GET', /^\/api\/links$/, () => (signedIn ? json({
    links: LINKS, admin: USER.admin, icons: LINK_ICONS,
  }) : unauthorized())],
  ['POST', /^\/api\/links$/, (_m, _q, body) => {
    if (!signedIn) return unauthorized();
    if (body.scope === 'host' && !USER.admin) {
      return json({
        error: 'a link for everyone on this host requires membership of wheel or sudo. ' +
               'You can add one for yourself instead.',
      }, 403);
    }
    const why = checkLink(body);
    if (why) return json({ error: why }, 400);
    const link = {
      id: newId(),
      name: body.name.trim(),
      url: body.url.trim(),
      icon: body.icon || 'a-globe',
      glyph: body.glyph || null,
      open: body.open === 'tab' ? 'tab' : 'frame',
      width: body.width || 1200,
      height: body.height || 800,
      added: NOW,
      actor: USER.username,
      scope: body.scope === 'host' ? 'host' : 'me',
      editable: true,
    };
    LINKS = [...LINKS, link];
    return json(link);
  }],
  ['PUT', /^\/api\/links\/[a-z0-9]+$/, (path, _q, body) => {
    if (!signedIn) return unauthorized();
    const id = path.split('/').pop();
    const at = LINKS.findIndex((l) => l.id === id);
    if (at < 0) return json({ error: 'there is no link with that id' }, 404);
    if (!LINKS[at].editable) {
      return json({ error: 'that link belongs to this host, not to you' }, 403);
    }
    const why = checkLink(body);
    if (why) return json({ error: why }, 400);
    const link = { ...LINKS[at], ...body, id };
    LINKS = LINKS.map((l, n) => (n === at ? link : l));
    return json(link);
  }],
  ['DELETE', /^\/api\/links\/[a-z0-9]+$/, (path) => {
    if (!signedIn) return unauthorized();
    const id = path.split('/').pop();
    const at = LINKS.findIndex((l) => l.id === id);
    if (at < 0) return json({ error: 'there is no link with that id' }, 404);
    if (!LINKS[at].editable) {
      return json({ error: 'that link belongs to this host, not to you' }, 403);
    }
    LINKS = LINKS.filter((l) => l.id !== id);
    return json({ ok: true });
  }],
];

/* Writes are accepted and reported as successful because the point is to reach
   the "saved" and "renamed" states in the UI, not to keep a filesystem. The
   listing is static, so a rename shows the original name again on refresh. */

const realFetch = window.fetch.bind(window);

window.fetch = async function (input, init = {}) {
  const url = new URL(typeof input === 'string' ? input : input.url, location.origin);
  const method = (init.method || (input && input.method) || 'GET').toUpperCase();

  if (!url.pathname.startsWith('/api/')) return realFetch(input, init);

  let body = null;
  if (init.body && typeof init.body === 'string') {
    try { body = JSON.parse(init.body); } catch (_) { /* not json; fine */ }
  }

  for (const [verb, pattern, handler] of ROUTES) {
    if (verb === method && pattern.test(url.pathname)) {
      // A little latency, so spinners and disabled states are actually visible.
      await new Promise((r) => setTimeout(r, PREVIEW.latency ?? 90));
      const res = handler(url.pathname, url.searchParams, body);
      PREVIEW.log?.(`${method} ${url.pathname}${url.search} -> ${res.status}`);
      return res;
    }
  }

  PREVIEW.log?.(`${method} ${url.pathname} -> unmocked`);
  return json({ error: `no mock for ${method} ${url.pathname}` }, 501);
};

/* --------------------------------------------------------------- terminal */

/* A shell that only knows how to look like one. Enough to show the prompt,
   wrapping, colour and the cursor at a real font size -- which is all the
   terminal contributes visually. */

const MOTD = [
  '\x1b[38;5;80mWebDesk preview\x1b[0m -- this terminal is a stub, no shell behind it.',
  'Try: ls, pwd, whoami, uname -a, neofetch, clear, help',
  '',
].join('\r\n');

const PROMPT = '\x1b[38;5;80mhutson\x1b[0m@\x1b[38;5;110morchard\x1b[0m:\x1b[38;5;180m~\x1b[0m$ ';

const FAKE_LS = [
  '\x1b[38;5;110mDownloads\x1b[0m  \x1b[38;5;110mnotes\x1b[0m      \x1b[38;5;110mprojects\x1b[0m',
  'README.md  \x1b[38;5;114mdeploy.sh\x1b[0m  inventory.csv  notes.txt  server.log',
].join('\r\n');

const COMMANDS = {
  ls: FAKE_LS,
  'ls -la': FAKE_LS,
  pwd: '/home/hutson',
  whoami: 'hutson',
  hostname: 'orchard',
  date: () => new Date().toString(),
  'uname -a': 'Linux orchard 6.9.7-200.fc40.x86_64 #1 SMP x86_64 GNU/Linux',
  uptime: ' 14:02:11 up 6 days,  3:41,  1 user,  load average: 0.08, 0.12, 0.09',
  help: 'Stub commands: ls, pwd, whoami, hostname, date, uname -a, uptime, neofetch, clear',
  neofetch: [
    '\x1b[38;5;80m      ___      \x1b[0m  hutson@orchard',
    '\x1b[38;5;80m     (o o)     \x1b[0m  ---------------',
    '\x1b[38;5;80m    (  V  )    \x1b[0m  OS: Fedora 40 x86_64',
    '\x1b[38;5;80m   /--m-m--/   \x1b[0m  Shell: bash 5.2.26',
    '\x1b[38;5;80m               \x1b[0m  WebDesk: 26.8.1 (preview)',
  ].join('\r\n'),
};

const RealWebSocket = window.WebSocket;

class MockSocket {
  constructor(url) {
    this.url = url;
    this.readyState = 0;
    this.binaryType = 'blob';
    this.onopen = this.onmessage = this.onclose = this.onerror = null;
    this._line = '';
    setTimeout(() => {
      this.readyState = 1;
      this.onopen && this.onopen({});
      this._write(MOTD + '\r\n' + PROMPT);
    }, 120);
  }

  _write(s) {
    this.onmessage && this.onmessage({ data: s });
  }

  send(data) {
    // app.js sends resize frames as JSON strings and keystrokes as bytes.
    if (typeof data === 'string' && data.startsWith('{')) return;
    const s = typeof data === 'string' ? data : new TextDecoder().decode(data);
    for (const ch of s) this._key(ch);
  }

  _key(ch) {
    if (ch === '\r') {
      const cmd = this._line.trim();
      this._line = '';
      this._write('\r\n');
      if (cmd === 'clear') {
        this._write('\x1b[2J\x1b[H');
      } else if (cmd) {
        const out = COMMANDS[cmd];
        const body = typeof out === 'function' ? out() : out;
        this._write(
          (body !== undefined ? body : `\x1b[31m${cmd.split(' ')[0]}: command not found\x1b[0m`) + '\r\n'
        );
      }
      this._write(PROMPT);
      return;
    }
    if (ch === '\x7f') {
      if (this._line) {
        this._line = this._line.slice(0, -1);
        this._write('\b \b');
      }
      return;
    }
    if (ch === '\x03') { // ctrl-c
      this._line = '';
      this._write('^C\r\n' + PROMPT);
      return;
    }
    if (ch < ' ') return;
    this._line += ch;
    this._write(ch);
  }

  close() {
    this.readyState = 3;
    this.onclose && this.onclose({});
  }

  addEventListener(type, fn) { this['on' + type] = fn; }
  removeEventListener(type) { this['on' + type] = null; }
}

window.WebSocket = function (url, protocols) {
  if (String(url).includes('/ws/term')) return new MockSocket(url);
  return new RealWebSocket(url, protocols);
};
window.WebSocket.prototype = MockSocket.prototype;
