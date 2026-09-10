# WebDesk
A web desktop for Linux servers. Sign in with your system account, get a file manager and a real terminal in the browser. One binary, no runtime, no buildstep, no npm.

Targets Debian/Ubuntu/Mint, RHEL/Fedora/Rocky, and Arch, on x86_64 and aarch64.

## Install
On the target host:

```sh
curl -fsSL https://raw.githubusercontent.com/HutsonLabs/WebDesk/main/bootstrap.sh | sudo sh
```

That fetches the source, installs build dependencies, builds, installs a systemd unit and a PAM service file, opens the firewall port, and starts the service. Then open **https://\<host\>:61443** and sign in with any normal account on that box. The certificate is self-signed until you give it a real one, so the browser asks about it once per host — see [HTTPS by default](#https-by-default).

The first build takes a few minutes; it is compiling Rust with fat LTO on the host. Nothing is left running that was not asked for, and everything it writes is listed under [What gets installed](#what-gets-installed).

Knobs, all optional:

```sh
curl -fsSL .../bootstrap.sh | sudo PORT=9000 WD_REF=v0.2.0 sh
```

| | |
| --- | --- |
| `PORT` | listen port (default 61443) |
| `WD_TLS=off` | serve plain http, for a host with a TLS proxy in front |
| `WD_TLS_CERT` / `WD_TLS_KEY` | PEM paths, instead of the self-signed pair |
| `PREFIX` | where the binary goes (default `/usr/local/bin`) |
| `WD_REF` | branch, tag or commit to install (default `main`) |
| `WD_REPO` | source repository, for a fork |
| `WD_ADMIN_GROUPS` | who may update from the browser (default `wheel,sudo`) |
| `WD_UPDATE=off` | build without the update capability at all |

### Nothing else to install

**A WebDesk install adds WebDesk and nothing else**: a binary, a systemd unit, a
PAM service file and a conf. That is the whole of it — see [What gets
installed](#what-gets-installed).

There used to be a `WD_APPS` knob here, because WebDesk used to run applications
itself and each way of doing that needed software on the host that WebDesk is
not: a container engine, then `flatpak` and a compositor and `wayvnc`, then
`cockpit-bridge`. It ran applications through them, and it does not any more —
Files and Terminal are the binary's own, and everything else is [a
link](#apps), which needs nothing installed anywhere. The knob is gone, and one
left in an older `install.conf` is read and ignored.

### From a checkout instead

```sh
./deploy.sh 10.1.2.40          # or user@host
PORT=61443 ./deploy.sh 10.1.2.40
```

rsyncs the working tree to the host and runs `install.sh` there, which is useful for testing a change you have not pushed. The settings in the table above are forwarded to that remote `install.sh`, so it is also how you move an existing host onto a different port. It rebuilds on the host rather than reusing what is already in that tree's `target/`, which the sync leaves untouched and would otherwise be a binary from the previous deploy. Or, already on the target:

```sh
sudo bash install.sh
sudo systemctl status webdesk
journalctl -u webdesk -f
```

Every one of these is also an upgrade path, and re-running any of them is safe.

### Coming from `linuxwebdesk`

This project was called `linuxwebdesk` until August 2026, and before that `rockywebde`. **There is no in-place upgrade from either.** Everything moved at once -- the binary, the systemd unit, the PAM service, `/etc/`, `/var/lib/`, `/usr/local/src/`, the release asset names, the `WD_*` environment prefix and the session cookie -- so an old install and a new one share no paths and would simply coexist, with the old unit still holding the port.

Remove the old install first, then install as above:

```sh
sudo systemctl disable --now linuxwebdesk
sudo rm -f /etc/systemd/system/linuxwebdesk.service /etc/pam.d/linuxwebdesk \
           /usr/local/bin/linuxwebdesk /usr/local/bin/linuxwebdesk-update \
           /usr/local/libexec/linuxwebdesk-update
sudo rm -rf /usr/local/src/linuxwebdesk /etc/linuxwebdesk /var/lib/linuxwebdesk
sudo systemctl daemon-reload
```

Settings are not carried over, so pass `PORT` and any other knobs again if the old install used something other than the defaults.

## Updating

Click the account button in the upper left of the desktop — the one whose tooltip is your `you@host` — and take the top row of the menu, which says the same name, to open **System**. It shows the running build and, for a member of `wheel` or `sudo`, checks the tracked ref for a newer commit and updates on a button, streaming the build log into the window as it goes. Everyone else sees the build information and a note saying why the update controls are not theirs to use.

It opens as a single window: taking that row again raises the one already open rather than stacking another copy. The second row of the menu signs you out.

The same thing from a shell, doing exactly the same work:

```sh
sudo webdesk-update
```

An update fetches the source for the tracked ref, rebuilds it on the host,
reinstalls, and restarts the service. Worth knowing before pressing it:

- **Usually it is quick.** CI publishes a binary for each architecture and libc
  family on every push to `main`, and the updater installs that when it matches
  the commit being installed — a download rather than a build. It falls back to
  compiling on the host when there is no matching artifact, which takes a few
  minutes. See [Release binaries](#release-binaries).
- **Everyone gets signed out.** Sessions live in memory and the service
  restarts. Open terminals end with them.
- **A failed build changes nothing.** The new binary is only installed after it
  compiles, so a broken commit leaves the running version untouched. The log
  stays in the System window and in `journalctl -u webdesk-update`.
- **Settings survive.** Port, prefix and tracked ref are recorded in
  `/etc/webdesk/install.conf` at install time and read back on update, so a
  host installed on port 9000 comes back on port 9000.
- **The installer comes from the tracked ref**, not from the copy the last
  update left on disk, so a change to how installing works takes effect on the
  next update rather than the one after it. If it cannot be fetched, the copy on
  disk is used instead, which still installs the source already there.

To follow something other than `main`, edit `WD_REF` in that file and restart.
To remove the capability from a host entirely, set `WD_UPDATE=off` there and in
the unit — the endpoints then refuse everyone, including admins.

## Release binaries

Every push to `main` is built by GitHub Actions for four targets — `x86_64` and
`aarch64`, each against the Debian and RHEL families — and published twice: to a
numbered, immutable release of its own, and to a rolling `latest-main`
prerelease that carries the same assets. Both get `SHA256SUMS`, a
`manifest.json` naming the commit and the version, and a signed build-provenance
attestation — published both to GitHub's attestation API and as an
`attestations.jsonl` asset, so that a host can verify it without a token. Hosts
tracking a branch install from `latest-main`; the numbered release is the
permanent record of what that version was.

### Version numbers

Releases are numbered `YY.MM.Build` — two-digit year, month with no leading
zero, then a counter that restarts each month. `26.8.1` is the first release of
August 2026; the next is `26.8.2`, and the first of September is `26.9.1`.

The counter is assigned by CI, not stored in the tree: it is derived from the
highest `v26.8.*` tag already published. That is deliberate. A number kept in a
file would have to be committed, so two commits landing close together would
race to claim the same one, and the commit that bumped it would itself trigger
another build. Deriving it from the tag list means the tags *are* the ledger.

It reads the highest number rather than counting the tags, so deleting a release
leaves a gap instead of handing a used number to a different commit.

`version` in `Cargo.toml` is only a floor. It is what a build reports when no CI
number was stamped into it — a working copy, or a host that had to compile from
source because no release matched its commit. A binary installed from a release
reports the real number, and `bootstrap.sh` records it in `.wd-source` so a
later rebuild of that same tree still reports it.

Pushing a `v*` tag by hand still works and takes the tag's own number verbatim,
which is the escape hatch for cutting a release out of band. Tags that CI
creates do not re-trigger the workflow: GitHub suppresses workflow triggers for
refs pushed with the default `GITHUB_TOKEN`, which is the only reason the `v*`
trigger is not a loop.

Arch has no artifact of its own and does not need one: glibc is backward
compatible and Arch's is newer than either build base, so it takes the RHEL
binary, which is built against the older of the two.

`latest-main` is only ever used to *find* a build. The manifest names the
numbered release it came from, and the binary and `SHA256SUMS` are then fetched
from that release instead, which is published once and never rewritten — so no
cache in front of the rolling pointer can hand a host a previous build's bytes.
The manifest fetch itself is cache-busted for the same reason.

Before installing one, `bootstrap.sh`:

1. refuses unless `manifest.json` names **exactly** the commit it was about to
   build — a release for any other commit is ignored rather than installed;
2. verifies `SHA256SUMS`, using `sha256sum` or `openssl`, and declines if
   neither is available rather than installing something it could not check;
3. verifies the provenance attestation with `gh attestation verify` when `gh`
   is installed **and new enough to have that command** (2.49+). It verifies
   against the `attestations.jsonl` published with the release, which makes no
   API call and needs no GitHub credentials; only a release cut before those
   bundles existed falls back to fetching the attestation from the API, which
   does need a token.

   A check that runs and says no is always fatal. A check that could not run is
   not, because it is declining to have an opinion rather than accusing the
   binary — that covers `gh` being absent, `gh` being too old, and `gh` having
   no credentials for the API fallback (its exit code 4). Whatever `gh` printed
   goes into the log either way, so a host that is not checking says why.

Any of these failing costs a compile, not an install. The knobs:

| | |
| --- | --- |
| `WD_PREBUILT=off` | never use a release binary; always compile on the host |
| `WD_REQUIRE_ATTESTATION=1` | refuse to install unless provenance is verified — implies `gh` 2.49+ must be present, and turns "could not check" into a hard failure |
| `WD_RELEASE_TAG=tag` | take the binary from a specific release |

Be clear about what the checksum does and does not buy you: it comes from the
same origin as the binary, so it catches corruption, not a compromised release.
The attestation is the part that establishes where the binary came from, and it
is only checked if `gh` is on the host. Verify one by hand with:

```sh
gh attestation verify webdesk-x86_64-rhel --repo HutsonLabs/WebDesk
```

That form fetches the attestation from GitHub and needs `gh` to be logged in.
The offline form, which is what a host actually runs, needs nothing but the two
files:

```sh
curl -fLO https://github.com/HutsonLabs/WebDesk/releases/download/latest-main/attestations.jsonl
gh attestation verify webdesk-x86_64-rhel --bundle attestations.jsonl --repo HutsonLabs/WebDesk
```

## What gets installed

```
/usr/local/bin/webdesk              the binary
/usr/local/bin/webdesk-update       symlink to the updater
/usr/sbin/webdesk-update            symlink again, on sudo's secure_path
/usr/local/libexec/webdesk-update   the updater
/usr/local/src/webdesk/             source, kept for incremental rebuilds
/etc/webdesk/install.conf           settings, so updates preserve them
/etc/pam.d/webdesk                  PAM service
/etc/systemd/system/webdesk.service the unit
/var/lib/webdesk/                   update log and status (root, 0700)
/var/lib/webdesk/tls/               the self-signed certificate and key (0700/0600)
/var/lib/webdesk/apps.json          which apps are installed
```

That is the whole of it, and installing an app barely adds to it: one line in
`links.json`. That is the whole of what an app costs on disk now: a name and a
URL. WebDesk installs no applications and keeps no application data — `apps.json`
was the list of installed ones and `appdata/` was a container's `/config`, and
both went with the things they described.

`/usr/local/src/webdesk/target` is a Rust build directory and is the large
one. Deleting it costs nothing but a cold build next update.

## What it does

- **Files** — browse, open, edit and save text files, upload, download, create
  folders, rename, delete. File types are shown with
  [Catppuccin Icons](https://github.com/catppuccin/vscode-icons) (MIT, see
  `ui/icons.LICENSE`), vendored as a single sprite.
- **Terminal** — a real login shell via `su - <user>`, xterm.js in the browser,
  resize-aware.
- **Windows** — draggable and resizable panes with focus, minimize and a dock.
  Not a compositor; they are positioned divs, which is all a shell like this
  needs.
- **Auto-hiding title bars** — a window can give its bar back to whatever is
  inside it. The circle in the title bar is the switch: a filled dot while the
  bar is staying put, an empty ring once it has gone. Thrown, it takes the bar
  off the layout and hangs it on the outside of the window's top edge instead;
  touching that edge brings it back, and it goes again when the pointer
  leaves. This is for a framed page that draws a bar of its own, which nearly
  every web application does: two bars stacked one on the other is what gives
  away that you are looking at a page inside a page. The bar arrives *above*
  the window rather than over its content, and the body keeps its size
  throughout, so the page inside is never asked to reflow just because the
  pointer crossed an edge. A
  window snapped against the top of the desktop has no room above it and steps
  down while the bar is out. The cost is the top five pixels of the window,
  which answer to the desktop rather than to the app while the bar is away. The
  setting is remembered per app in the browser's own storage — nothing about
  it reaches the host.
- **Dock** — a floating frosted bar rather than a panel. Windows grow out of
  their icon and shrink back into it, and an icon carries a dot while its app
  has a window open, minimised or not. Clicking an icon raises what is already
  there — the minimised window first, otherwise the next one, so a second click
  on Terminal walks through the terminals; alt- or middle-click opens another.
  Open editors get a dock item each, drawn with the file's own icon. The dock
  is applications and open windows and nothing else.
- **Links** — anything else this machine, or your network, already serves.
  Type `localhost:8096` or `https://gmail.com` and it gets a dock tile of its
  own, opening in a window like everything else — or in a browser tab, for the
  many sites that refuse to be framed. Nothing is installed and nothing runs on
  the host. See [Apps](#apps).
- **Apps** — one grid of everything this desk can open: Files, Terminal,
  Settings, System and every link, drawn as the tiles the dock draws, in the
  order the dock draws them. Behind each link's tile is a page of its own. See
  [Apps](#apps).
- **Settings** — what this desk is like, in one window: accent colour, backdrop,
  motion, whether dragging to an edge snaps, whether an app reopens at the size
  and place you left it, and what opens when you sign in. Nothing has a Save
  button; a setting is one control and takes effect as it is changed. Files and
  the terminal have settings of their own — dotfiles, sorting and what a click
  does; font, cursor, scrollback, colours and copy-on-select — asked for in a
  modal from the gear in that app's own title bar, and reachable from a row in
  this window as well. All of it lives in this browser's storage: another
  browser gets its own answers and the host is never told any of it. See
  [Settings](#settings).
- **Account** — one button in the upper left, carrying no text: the username is
  its tooltip. It drops a three-row menu — the username again, which opens
  System, Settings, and Sign out.
- **No browser dialogs** — nothing calls `prompt()`, `confirm()` or `alert()`.
  Questions are asked in an in-page modal, complaints arrive as a toast above
  the dock, and a file the editor will not take is downloaded rather than
  opened in a tab of its own. Every button names itself in a styled tooltip;
  there is not a `title` attribute in the UI.

## HTTPS by default

WebDesk asks for a system password and hands back a root-capable shell. A login
form on plain http puts that one passive listener away, so **https is the
default and plaintext is now the thing you have to ask for.**

| | |
| --- | --- |
| Port | **61443** — five digits, and above the 32768–60999 range Linux hands out for outbound sockets, so nothing else on the host is already holding it |
| Certificate | self-signed, written to `/var/lib/webdesk/tls/` on first start (`0700`, key `0600`) and reused after that |
| Names on it | the host's own hostname, `localhost`, `127.0.0.1`, `::1` |
| Stack | `rustls` with `ring`. No OpenSSL headers, no cmake, nothing new to install on the host |
| Protocol | HTTP/1.1 only. h2 is deliberately not offered: a websocket over h2 needs extended CONNECT, and the terminal is a websocket |
| Session cookie | now `HttpOnly; SameSite=Strict; Secure` |

**What the self-signed certificate does and does not buy.** It encrypts, which
is the part that matters for a password crossing a LAN. It authenticates
nothing — there is no authority to check it against — so the browser shows its
interstitial once per host, and a machine-in-the-middle is not detectable. That
is a real limit and it is the honest default: the alternative was http, which
is worse in every respect and has no warning at all.

Give it a real certificate and both the warning and the limit go away:

```sh
sudo curl -fsSL .../bootstrap.sh | sudo \
  WD_TLS_CERT=/etc/ssl/certs/desk.pem WD_TLS_KEY=/etc/ssl/private/desk.key sh
```

Both must be PEM, both must be set together, and the certificate file may be a
chain. Already installed? The same two names in `/etc/webdesk/install.conf` are
what the unit reads, and `systemctl restart webdesk` picks them up.

**Behind a reverse proxy that already terminates TLS**, set `WD_TLS=off` to go
back to plain http on the listen port, and `WD_SECURE=on` so the session cookie
is still marked `Secure` — the browser is on https even though this process is
not. The scheme is not inferred from a forwarded header, because a client can
send one of those too.

**Typing http:// by mistake** gets a `308` to the same URL over https rather
than a TLS parse error and a blank page. The port sniffs the first byte of each
connection — a TLS `ClientHello` starts `0x16`, an HTTP method does not.

**Upgrading an existing install** keeps the port it was installed with; only the
scheme changes. A host on 6767 stays on 6767 and starts answering https there.
Pass `PORT=` explicitly to move it -- `PORT=61443 ./deploy.sh host` -- which
rewrites the recorded port and reopens the firewall on the new one.

## Apps

There are two kinds and neither of them is installed.

**Built in.** *Files* and *Terminal* — a file manager and a real login shell,
both served by the binary itself. See [What it does](#what-it-does).

**Links.** Anything else you already run: a name, an address, an icon. Open
**Apps** from the dock, take the dashed *Add a link* tile, and type
`localhost:8096` or `https://gmail.com`. It gets a tile beside Files and
Terminal and opens in a window with that page in it — or in a browser tab, for
the many sites that refuse to be framed. See [docs/links.md](docs/links.md).

**Apps is the grid that shows all of them**, built-ins included, which is a
thing no window did before: Files and Terminal were reachable only from the
dock, System only from the account menu, and links only from a list of their
own. One grid, unheaded — a desk with eight tiles on it does not need a filing
system. Clicking a tile opens it; the chevron in its corner — or a right-click —
turns it over to a page carrying what a tile cannot say on its face: the whole
address, whether the frame is actually going to work, and who else on this host
is looking at the same tile.

It was a list of rows until it was a grid, and the rows were the catalog's rows.
A row is the right shape when every one of them carries a description and three
different verbs; a link carries a name and an address, so twenty of them were
twenty repetitions down a window that was half whitespace — beside a dock that
was already drawing the same things, better, as tiles.

**WebDesk used to run applications for you, twice, and both attempts are gone.**
First as containers from a fixed list of images, reverse-proxied onto this origin
at `/app/<slug>/`; then as Flatpaks installed host-wide and streamed into a
canvas over RFB from a headless compositor. The first needed a container engine,
a port allocator, bind mounts and an application that tolerated living under a
path prefix. The second needed `flatpak`, a compositor and `wayvnc` on the host —
and on Enterprise Linux 9 no compositor is packaged at any version, so on that
generation the entire catalog was uninstallable.

Both were this program running a smaller, worse copy of machinery the operator
already had. A machine that runs WebDesk very often already runs things, with
Compose or units or behind a proxy somebody configured years ago. So it stopped
competing with that and started pointing at it.

**The rule the whole thing rests on: WebDesk never fetches the URL.** Your
browser does. A server-side fetch would hand every signed-in session a
request-forgery primitive aimed at every service this host can reach and you
cannot — and it would drag the whole reverse proxy back. There is no HTTP client
in `src/links.rs`.

That is also why **anyone signed in may add a link for themselves**, where the
old catalog was fixed in the binary and closed to everybody. A container was a
way to run code as root; a link runs nothing. Personal links live in
`~/.config/webdesk/links.json` and are written through the same
privilege-dropping helper that writes every other file in your home, so they need
no new privileged code at all. **An administrator may publish one host-wide**,
which does, and is gated like every other administrative act here.

Four things are worth knowing before you add one:

- **`localhost` means the machine your browser is on**, not the machine WebDesk
  runs on. `http://localhost:8096` is right when you are sitting at the server
  and silently wrong from a laptop. The form says so as you type it.
- **An `http://` address cannot be framed on an https desk** — that is mixed
  content and the browser blocks it before any request. Such a link is set to
  open in a tab, with the reason on the form. `http://localhost` is the
  exception: browsers treat loopback as trustworthy, so it frames.
- **Many sites refuse to be framed** with `X-Frame-Options` or CSP
  `frame-ancestors`, and a parent page **cannot reliably detect that** — a
  blocked frame still fires `load`. So there is no auto-detection: the window
  carries *Open in a tab* from the first frame, and after three seconds a line
  appears explaining a blank one, with an *Always open in a tab* beside it.
- **A link does not arrive signed in.** It is a different origin; you sign in to
  the site itself and your browser keeps that cookie.

The frame is sandboxed, unlike the container frame that preceded it, and
`allow-top-navigation` is the flag it deliberately lacks: without a sandbox a
framed page can navigate the whole desk elsewhere, and this desk's login form
takes a system password that hands back a root-capable shell.

## Settings

Two windows' worth of questions, in three places, because they are three
different kinds of question.

**The desk**, in the Settings window — reached from the account menu or the Apps
grid, like System:

| | |
| --- | --- |
| **Accent** | five colours, shown as the colours themselves. Everything reads `--accent`, so this is one variable and the focus rings, the chosen menu row, the confirm button and the terminal caret all follow |
| **Desktop** | Aurora, Dusk or Plain — the same dark with different amounts of weather behind the windows |
| **Motion** | follow the system, or say it here: windows flying out of the dock and menus rising, or everything in place at once |
| **Snap to the edges** | whether dragging a title bar into an edge offers a half or a quarter. The layout button offers the same seven regions either way |
| **Remember size and position** | one shape per app, written down when a drag or a resize ends and clamped to the screen on the way back — the shape may have been recorded on a larger one |
| **Open on arrival** | Files, Terminal, Apps or nothing, when you sign in |

**Each app**, in a modal from the gear in its own title bar — and from a row in
the Settings window, which opens the same dialog rather than a copy of it:

- **Files** — show dotfiles, folders before files, sort by name / largest /
  newest, whether one click opens, and how large a text file may be before it
  downloads instead of opening in the editor.
- **Terminal** — font size, line spacing, cursor shape, whether it blinks,
  scrollback, colours (Desk, Black, Light) and copy-on-select. Every one of
  them is set on the terminals already open, not just the next one. Copy-on-
  select needs a page the browser trusts with the clipboard — https, or
  localhost — and quietly does nothing elsewhere.

**Per window**, from the title bar itself: the auto-hiding bar switch, which is
remembered per app.

None of it reaches the host. It is `localStorage` under one key, so a second
browser is entitled to its own answer, another person signing in here gets
their own, and a host that has never heard of any of it stays that way. *Reset
everything* in the Settings window puts all three groups, the remembered
windows and the title-bar switches back to how they arrived.

## Icons

File and folder icons are the `css-variables` build of
[Catppuccin Icons](https://github.com/catppuccin/vscode-icons) (MIT). A curated
subset — 53 symbols, 23 KB — is vendored into `ui/icons.svg` as one sprite and
committed, so neither a build nor an install ever fetches them.

The sprite is injected into the document at boot rather than referenced with
`<img>`. That is not incidental: those icons colour themselves from
`--vscode-ctp-*` custom properties, and an image is a separate document that
cannot see this page's variables. Injected, `<use>` resolves in the same
document and the Mocha palette in `style.css` applies.

To cover more file types or move to a newer upstream, edit `SHA` or `ICONS` in
`scripts/vendor-icons.py`, re-run it, and commit the result.

The desktop's own icons — the Files toolbar, the dock, the window controls —
are hand-drawn in `ui/ui-icons.svg`, hairline strokes in `currentColor` so a
button colours its own icon on hover.

There used to be a third set: brand marks from [Simple
Icons](https://simpleicons.org), one per catalog entry, vendored by a
`scripts/brand-icons.py` that is also gone. They went with the catalog.

A link draws one of ten neutral marks from the hand-drawn set — an allow-list in
`src/links.rs` rather than "any id in the sprite", because an id the sprite has
not got draws an empty square, silently, in somebody's dock — **or an icon you
pasted**. Copy one from [iconify.design](https://iconify.design) (362,000 across
238 sets, `selfh.st` among them) and paste its SVG into the form.

Nothing is fetched and no set is vendored: selfh.st alone is 13.2 MB against a
2.9 MB binary, and fetching at paint time would put a third-party request in
front of every dock. **The geometry is stored with the link**, so a pasted icon
works on an air-gapped host and is never fetched again.

What is stored is *not* the markup that was pasted — SVG carries `<script>`,
`onload=` and `javascript:` hrefs, and this desk's login form takes a system
password. The browser reduces the paste to shapes and numbers in an inert parse,
`src/links.rs` refuses anything that is not shapes and numbers, and the dock
rebuilds it with `createElementNS` rather than markup. Icons are monochrome, so
they take the colour of whatever they sit in. See
[docs/links.md](docs/links.md#icons).

## How privileges work

This is the part worth reading before trusting it.

The daemon runs as **root**, because authenticating against PAM requires it.
It never touches your files with those privileges. On a successful login it
forks a **helper** — the same binary, re-executed with `--helper` — which
permanently drops to the authenticated user via `setgid` → `initgroups` →
`setuid` before doing anything. Every filesystem operation for that session is
performed by that child.

```
browser → daemon (root) → PAM → uid/gid
                        ↘ helper (that user) → filesystem
```

The consequence is the point: **there is no permission logic in this program.**
The kernel decides what the session can read and write, exactly as it would for
that user at a shell. Nothing to get wrong, nothing to bypass. Terminals get the
same treatment for free — `su -` is the shortest correct path to a login shell.

Because it terminates at PAM, whatever the host already uses works unchanged:
local accounts, SSSD, LDAP, Kerberos. Nothing here knows or cares which.

`root` itself is refused a session on purpose.

### The two exceptions

Two things cannot work that way, and both are gated the same way.

**Updating** replaces a root-owned binary and restarts a root service, so it
runs with the daemon's privileges rather than the user's.

**Publishing a link for the whole host** writes root-owned state that changes
what every session sees, so it is gated too. Adding one *for yourself* is not:
a link in your own dock grants you nothing you did not already have — you have a
browser, and you could have typed the address into it. It is written into your
own home through the privilege-dropping helper, so there is no ownership check
in the code and none to get wrong: the process doing the writing is you, and
cannot reach another account's list. The kernel decides, as it does for the file
manager.

These are the only two places in the program where authorisation is a decision
in code instead of a question for the kernel — which is exactly why they are
worth naming rather than burying.

Both are gated on something the host already decided: membership of
`wheel` or `sudo`, resolved through `getgrouplist` exactly as `sudo` resolves
it. An SSSD- or LDAP-provided `wheel` works without being told about. A session
outside those groups gets `403` from every update route and from every
host-scoped link route, and the check is on each route rather than on the
button — the controls are hidden for non-admins, but hiding a button is not an
access control.

**Adding a link trusts nothing**, which is the whole reason it is not gated. No
software is fetched, no code runs on this host, and the address is never dialled
from the server. What a link can do is put a page in a frame in your browser,
which is why `src/links.rs` accepts only `http` and `https` addresses and why
that frame is sandboxed without `allow-top-navigation` — see
[docs/links.md](docs/links.md) for both halves of that argument.

Be clear-eyed about what updating trusts. Pressing update runs code fetched from
GitHub as root on your host. The trust anchor is TLS to `codeload.github.com`
plus whoever can push to the tracked ref — there is no signature check, and a
compromised upstream is a compromised host. That is the same bargain as any
`curl | sh` installer, which is what this is; it is just wearing a button. If
that is not a bargain you want on a particular box, `WD_UPDATE=off` removes it
and `sudo bash install.sh` from a checkout you control still works.

## Layout

```
src/main.rs     axum server, sessions, filesystem API
src/auth.rs     PAM authentication, NSS lookup, the admin-group check
src/helper.rs   the privilege-dropping child and its file operations
src/proto.rs    JSON-line + binary-payload framing for the helper channel
src/pty.rs      terminal sessions over WebSocket
src/update.rs   version reporting, update check, launching the updater
src/links.rs    links: the store, the routes, and what an address must be
src/tls.rs      WebDesk's own https listener and its self-signed certificate
ui/             the whole frontend — vanilla JS, no build step
ui/icons.svg    vendored Catppuccin icon sprite, injected at boot
ui/ui-icons.svg hand-drawn sprite for the Files toolbar's own actions
scripts/        vendor-icons.py, run by hand to refresh the vendored sprite
scripts/brand-icons.py  the application marks, from Simple Icons — see Icons
scripts/preview.py  serve ui/ on any machine with mocked data — see Previewing the UI
.github/        the release workflow: build, attest, publish
bootstrap.sh    curl | sh installer; also the engine behind an update
install.sh      runs on the target: deps, build, PAM, systemd, firewall
libexec/        the updater: lock, log, status, run bootstrap.sh
deploy.sh       rsync + remote install
```

The UI is compiled into the binary with `rust-embed`, so deployment is a single
file. Measured on Rocky Linux 10 (x86_64): **2.13 MB** including the frontend,
idling at **4.4 MB** resident.

PAM is bound through hand-written FFI rather than `bindgen`, and `build.rs`
links `libpam.so.0` directly when no `-devel` symlink is present. The practical
effect is that this builds on a stock host with **no packages beyond gcc** --
no clang, no pam-devel -- on either distro family.

## Previewing the UI

The app itself needs Linux, PAM and a login shell. A colour, a gap or a label
needs none of that, so there is a look-only preview that runs anywhere Python
does — macOS included:

```sh
scripts/preview.py            # http://127.0.0.1:6868, opens a browser
scripts/preview.py --port 7000 --no-open
```

It serves the same `ui/` files the binary embeds, with a shim that answers every
`/api` call and the terminal socket with canned data. Nothing is copied or
rewritten on disk: `index.html` is patched in flight to load the shim, so what
renders is the file that ships. No npm, no cargo, no venv.

The bar in the corner has four controls:

| | |
| --- | --- |
| **Scene** | jump to a state — sign-in, a failed sign-in, the file manager, the editor, the terminal, System with an update pending / running / failed, a non-admin session, a permission-denied listing, four windows at once, the rename and delete dialogs, Settings, the terminal's and Files' own settings dialogs, Apps with links / with none / as a non-admin, a link's own page |
| **Viewport** | render at phone, tablet or laptop size without resizing the window |
| **Inspect** (⌥I) | click any pixel; the `ui/` file and line that style and build it are copied to the clipboard |
| **↻** | reload — though saving anything under `ui/` already reloads the tab |

Inspect is the part that pays for itself when the next step is a prompt. Clicking
the Delete button in the file manager copies:

```
element: button.fbtn.danger
text: "Delete"
path: div.win-body > div.files > div.files-bar > button.fbtn.danger
styled by:
  ui/style.css:141  .fbtn
  ui/style.css:142  .fbtn:hover
  ui/style.css:143  .fbtn.danger:hover
built in:
  ui/app.js:275  <button class="fbtn" data-a="up">Up</button>
```

Scenes and viewports are URL-addressable — `?scene=terminal&device=390x844` —
and `&nowatch=1` turns off the reload poll, which is what makes the preview
screenshottable from a headless browser.

Writes are accepted and reported as successful so the "saved" and "renamed"
states are reachable, but the listing is static and nothing is kept. Anything the
shim does not recognise answers `501 no mock for ...` rather than failing
quietly, so a missed route looks like a missed route and not a UI bug. The whole
harness lives in `scripts/` and is never embedded: `rust-embed` only takes `ui/`.

The mock's links are simply four plausible ones. There used to be a drift check
here — `preview.py` read the app catalog out of `src/catalog.rs` on every load
and corrected the shim's stale copy of it — and it went with the catalog. A link
has no source of truth in `src/` to drift from: what one says is whatever
somebody typed into it.

## API

All endpoints require the session cookie set by `/api/login`.

| Method | Path | |
| --- | --- | --- |
| POST | `/api/login` | `{username, password}` → sets `wd_session` |
| POST | `/api/logout` | ends the session, kills its helper |
| GET | `/api/me` | current identity |
| GET | `/api/fs/list?path=` | directory listing |
| GET | `/api/fs/read?path=` | file contents (64 MB cap) |
| PUT | `/api/fs/write?path=` | body is written verbatim |
| POST | `/api/fs/mkdir` | `{path}` |
| POST | `/api/fs/remove` | `{path}` — directories must be empty |
| POST | `/api/fs/rename` | `{path, to}` |
| GET | `/ws/term` | WebSocket: binary = I/O, text = `{"t":"resize",...}` |
| GET | `/api/links` | `{links, admin, icons}` — host-wide first, then your own |
| POST | `/api/links` | `{name, url, icon, open, width, height, scope}` |
| PUT | `/api/links/<id>` | the same body |
| DELETE | `/api/links/<id>` | |

The link routes are open to any session **for its own links**. `scope: "host"`,
and any write to a host-wide link, need the administrative group and are refused
with `403` otherwise — checked on the route, not on the button.

These require the session to be in an admin group and return `403` otherwise:

| Method | Path | |
| --- | --- | --- |
| GET | `/api/system/info` | build, host, and whether this session may update |
| POST | `/api/update/check` | compare the running commit with the tracked ref |
| POST | `/api/update/apply` | start an update; returns as soon as it is handed off |
| GET | `/api/update/status` | state, phase and log tail of the current or last run |

`/api/system/info` is the exception — any session may read it, but it reports
`updates.allowed: false` for one that may not update.

**There is no install and no start**, and there used to be both. Nothing is
installed and nothing runs: a link is a name and an address, and the browser is
what fetches the page.

## Known limits

- **The certificate is self-signed unless you supply one.** That is a real
  limit, not a formality: it encrypts, but it authenticates nothing, so a
  machine-in-the-middle on the path is not detectable. On a LAN it is strictly
  better than the plaintext it replaced; across anything less trusted, give it
  a certificate with `WD_TLS_CERT` and `WD_TLS_KEY`. See
  [HTTPS by default](#https-by-default).
- **Sessions are in memory.** Restarting the service signs everyone out.
- **No static musl build.** PAM `dlopen`s its modules, so it cannot be
  statically linked. Build on each distro family, or build against the oldest
  glibc you intend to support.
- **Delete is non-recursive** — deliberately, for now.
- Files are read into memory rather than streamed, hence the 64 MB cap.
- **Compiling on the host is still the fallback**, and it is expensive when it
  happens. Measured on Rocky Linux 10.2 (x86_64, 32 cores): 42 s wall and
  **2.2 GB peak memory**, leaving **282 MB** in `target/` and **687 MB** of Rust
  toolchain in `/opt/rust`. A 1 GB VM will be OOM-killed mid-build. Release
  binaries avoid all of it, so the fallback should be rare — but a host on an
  architecture or family with no artifact, or one that cannot reach the release,
  pays this every update.
- **Provenance is only checked when `gh` 2.49 or newer is installed.** The
  attestation is always produced and can be verified out of band, but a host
  without a `gh` that can check installs on a checksum alone, which is not a
  provenance control. Debian and Ubuntu LTS archives still carry older `gh`
  builds, so this is the common case rather than the exotic one. Set
  `WD_REQUIRE_ATTESTATION=1` to make it mandatory. It no longer requires that
  `gh` be *logged in*: the check runs offline against the bundle published with
  the release.
- **No static musl build.** PAM `dlopen`s its modules, so the binary cannot be
  statically linked; that is why artifacts are per libc family rather than one
  universal build.
- **An update signs everyone out**, because sessions are in memory.
- **A link is a bookmark, not a session.** It does not arrive signed in, WebDesk
  holds no credentials for it, and nothing checks whether the site is up. See
  [Apps](#apps) for the four things worth knowing before adding one.
- **Many sites cannot be framed and this cannot be detected.** A blocked frame
  still fires `load`, so the window explains a blank rather than predicting one.
- **`localhost` in a link is the browser's machine**, not the server's.

## Not built yet

Deliberately out of scope for this pass: service and log viewers, storage,
networking, users, multi-window terminals per session, and drag-and-drop
upload.

For links specifically: **hiding a host-wide one you do not use**, **reordering
them**, and **importing a list** rather than filling the form once per tool. All
three are named in [docs/links.md](docs/links.md) with the shape each would take.

**Running applications is not on this list, and will not come back.** WebDesk
tried it twice — containers behind a reverse proxy, then Flatpaks streamed from
a headless compositor — and both are described in [Apps](#apps) and in
`docs/links.md`. A browser cannot host a native window, so showing one means
streaming its pixels; that is a different architecture and `docs/architecture.html`
is where it is thought through rather than in this program.

For the update path specifically: rollback to the previous build and a
scheduled check are both missing and both worth having.
