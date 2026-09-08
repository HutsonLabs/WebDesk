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
| `WD_APPS` | provision what the app kinds need — `streamed`, `host`, or `all`. **Empty by default**, so a plain install still adds nothing but WebDesk |

### What apps need, and when to install it

Neither half works on a bare host. [Apps drawn on this
host](#apps-drawn-on-this-host) need `flatpak`, a compositor and `wayvnc`, and
the [host panels](#host-panels-without-the-cockpit-interface) need
`cockpit-bridge`. **Neither is installed by default**, because a host that will
only ever run the file manager and the terminal should not be made to carry a
compositor — what gets written is still exactly [what is listed
below](#what-gets-installed).

There are two moments to fix that, and the second one is the good one.

**At install time**, if you already know what the machine is for:

```sh
curl -fsSL .../bootstrap.sh | sudo WD_APPS=streamed,host sh
```

`all` and `none` also work, the value is validated before anything is
downloaded, and it is recorded in `/etc/webdesk/install.conf` so an update does
not quietly revert it. Whatever is still missing is named in the closing
summary, along with what it costs and the exact `WD_APPS=` to re-run with.

**Or from the browser, later.** Open **Apps** and anything missing is listed
above the catalog: what it is, one sentence on what stops working without it,
the package that would provide it, and a button that installs the lot. A blocked
install offers the same button in place rather than sending you elsewhere —
pressing it installs the dependency and then continues the install you wanted.
It is gated on the same administrative group as every other install, and a
non-admin is told which groups can rather than shown a button that will fail.

Three answers are refusals rather than buttons, and they say so: you are not in
the group; this host has no package manager WebDesk knows; or the package does
not exist here. The last one is real and specific — **`cage` is in EPEL 10 and
was never built for Enterprise Linux 9**, so an EL9 host is told that streamed
apps are not available on that release rather than being sent to install
something that does not exist. On EL10 it names EPEL and stops: enabling a
third-party repository is not something WebDesk will do to your machine on your
behalf.

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
`apps.json`. The application itself is a Flatpak and lives where Flatpak puts
it, under `/var/lib/flatpak`; its *state* is in `~/.var/app/<id>` in each
user's own home. WebDesk has no app data directory of its own any more —
`appdata/` was a container's `/config` and went when the containers did.

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
  leaves. This is for the streamed desktops, which draw a bar of their own at
  the top of the screen they are streaming: two bars stacked one on the other
  is what gives away that you are looking at a desktop inside a desktop. The
  bar arrives *above* the window rather than over its content, and the body
  keeps its size throughout, so a streamed canvas is never asked to
  renegotiate its resolution just because the pointer crossed an edge. A
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
- **Apps** — a short catalog of desktop applications that run on this machine
  as you and are drawn into a window here. Pick one and press Install; there is
  nothing to fill in. It gets a dock icon of its own and opens in a window like
  everything else. See [Apps drawn on this host](#apps-drawn-on-this-host).
- **Account** — one button in the upper left, carrying no text: the username is
  its tooltip. It drops a two-row menu — the username again, which opens
  System, and Sign out.
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

## Apps drawn on this host

Every entry in the catalog is one thing: a **Flatpak that runs on this machine
as you**, under a headless compositor, with its pixels carried into a WebDesk
window. Open it from the dock and you are running the program locally in every
sense that matters — it has your home directory, your files, your fonts, your
theme, the machine's GPU and a working `xdg-desktop-portal`, because it is an
ordinary application in an ordinary session.

Open **Apps** from the dock, choose something from *Available*, and press
Install. There is nothing to fill in. The download streams into the window as it
goes, and the app appears in the dock with its own icon.

**There used to be two other kinds**, and both were HTTP services WebDesk
reverse-proxied at `/app/<slug>/`: containers it created from a fixed list of
images, and one service adopted from a systemd unit on the host. Both are gone,
along with the proxy, the container engine, the port allocator, the bind mounts
and the path-prefix question that decided what could be in the catalog at all.
If you want a web application on this desk, you run it yourself — wherever you
already run things — and give WebDesk a URL. See [Apps you supply a URL
for](#apps-you-supply-a-url-for).

What is left is easiest to see in what an entry does *not* have to answer for:

| | the container entries | what replaced them |
| --- | --- | --- |
| published port | assigned from 47000–47999 | none |
| state directory | `/var/lib/webdesk/appdata/<slug>`, mounted at `/config` | none — Flatpak already keeps it in `~/.var/app/<id>`, per user |
| `PUID`/`PGID` | the installer's | none — it runs *as* you |
| `TZ` | read off the host and passed in | none — it is on the host |
| `--shm-size` | `1g`, or a browser dies | none |
| render node and fonts | bound in, when the host has them | none — every device is simply present |
| path prefix | must survive `/app/<slug>/` | none — there is no proxy |

Seven absences, but they are one simplification said seven times: none of those
questions exist when the application is already running as the right user on the
right machine. It is also why there is no install form — every blank a container
entry asked you to fill had exactly one right answer here.

**It answered four of the old [Known limits](#known-limits) outright.** A
download lands in `~/Downloads` rather than in an app directory. There is no
passwordless root shell behind it. `/home` is not mounted read-write for every
app on the box, because nothing is mounted anywhere. And it does not hold a
gigabyte of shared memory to run a browser.

**The catalog is still fixed, and still in the binary.** Installing chooses what
software goes on the machine, and that is not a decision to take from a request
body — so the set of entries is a property of the build, reviewed like any other
code. `src/catalog.rs` is the file; `scripts/flathub-entry.py` writes an entry
from an application id, and prints the permissions that id asks for before you
add it. **You cannot define your own entry from the browser**, by design.

**What it costs.** The container boundary is gone. A drawn app is confined by
Flatpak's own sandbox, which is finer-grained than "this container can read
every home directory on the machine" but is the *app's* sandbox rather than
ours — and an app that asks for the whole of `home` in its manifest has one no
narrower than a container's on that question. It is still the right user's
files, which is the point, but do not read this as tighter confinement across
the board. That is why installing one is still gated on the administrative
group, and why `scripts/flathub-entry.py` prints an app's requested permissions
before you add it.

### Installed once, run per user

The two halves are deliberately different, and they line up with the split this
project already had between who may install and who may open.

**Installing is host-wide.** `flatpak install --system`, one copy on disk, part
of the machine like a package, and gated on `wheel`/`sudo` exactly like every
other install here. **Running is per user.** Opening one starts a systemd *user*
unit in your own session, because an application whose subject is your home
directory is worth nothing pointed at somebody else's.

So two people can have the same app open at once and they are two compositors,
two sockets and two sets of files, neither able to reach the other's. The socket
is a unix socket under `/run/webdesk/rfb/<uid>/`, mode `0700`: unreachable from
the network as a property of the filesystem rather than of a `--bind` somebody
had to remember to write.

### How the pixels get out

**Sway** run headless — with no bar, no borders and one application in it — or
`cage` where Sway is not packaged, with `wayvnc` serving RFB on that socket and
noVNC drawing it on a canvas in the browser.

Sway is preferred for one reason and it is not tidiness: its output can be
resized *and scaled* while it runs, so an application's resolution follows the
WebDesk window and its interface can be made bigger without being made blurrier.

**Zoom is in the window menu**, from 100% to 300%, and it is remembered per app
in your own browser rather than on the host — two people opening the same app
want different answers, and a value stored beside the install would give them
one between them. It defaults to your browser's `devicePixelRatio`, which is the
right answer without anybody choosing: the framebuffer is asked for in device
pixels, so on a HiDPI display an unscaled session would arrive sharp and half
the size it should be.

The distinction that makes this work is between the framebuffer and the logical
size. The framebuffer is what the browser paints one pixel for one pixel; the
scale divides it into the space the application lays itself out in. At 200% in a
1920×1200 window the application is told it has 960×600 to work with and draws
every pixel twice — the same sharpness, everything twice the size. That is not
the same as showing a smaller picture stretched, which is what a viewer-side
zoom does and what this replaces.

It only goes so far, and WebDesk says so rather than letting it fail quietly:
200% in a small window leaves an application a few hundred points wide, at which
point it hides toolbars and clips dialogs instead of complaining. A request that
would do that is refused with the numbers in the sentence. It also has somewhere sensible to put a second window, which is what an
application opening a file dialog needs and what a one-surface kiosk compositor
does not have. WebDesk terminates the WebSocket itself, so there is no
`websockify` and no second daemon per app.

Two consequences worth knowing:

- **The unit is one template for every app, and its instance name is a slug.**
  A unit whose `ExecStart` interpolated an application id would be a way to run
  any Flatpak on this host as anyone. Instead the unit runs
  `webdesk app-session <slug>`, which resolves the slug against the catalog
  compiled into the binary and refuses anything that is not in it. A request
  still decides only *whether* something the build already contains runs.
- **Stopping a session kills the Flatpak by application id**, because the scope
  `flatpak run` escapes into is otherwise outside the unit's cgroup and that is
  the only handle that reaches into it. It follows that quitting an app here also
  closes that app if you happen to have it open on the machine's own screen.
  That is announced in the unit's journal rather than done silently.

### Adding one

`scripts/flathub-entry.py <application-id>` prints a ready-to-paste catalog
entry, along with the things that actually decide whether it belongs: installed
size, licence, whether Flathub has verified the publisher, and which sandbox
permissions it asks for. It also says plainly what it cannot answer — whether
the app is usable at a fixed initial resolution, whether its dialogs behave
under a one-surface compositor, and whether it wants audio it will not get.

Entries live in the binary and are reviewed like code. What changed when the
containers went is what the review is *about*: the deciding question used to be
mechanical — does this application tolerate living under `/app/<slug>/` — and
that question no longer exists. The gate is judgement rather than compatibility.
See [docs/flathub.md](docs/flathub.md).

## Apps you supply a URL for

**Designed, not built.** [docs/url-apps.md](docs/url-apps.md) is the whole of it
today; this is the summary.

WebDesk used to be able to *run* a web application for you — a container from a
fixed list, published on loopback and reverse-proxied onto this origin at
`/app/<slug>/`. That is gone. The replacement is not a smaller version of it: you
run the application yourself, wherever you already run things, and give WebDesk
a URL. It stores it, draws a tile, and opens a window with that page in it.

The distinction that makes this safe to hand to users, when the catalog is
deliberately closed to them, is that **a URL app runs nothing.** Your browser
fetches a page from a server you named. WebDesk executes no code, installs no
software, opens no port and — this is the load-bearing rule — **never fetches
the URL itself**. A server-side fetch would hand every signed-in session a
request-forgery primitive pointed at every service this host can reach and you
cannot.

So the gate is different from the one on installing an app. A link in your own
dock grants you nothing you did not already have, so **anyone may add one for
themselves**; it is stored in `~/.config/webdesk/urlapps.json` and written
through the same privilege-dropping helper that writes every other file in your
home, which means it needs no new privileged code at all. **An administrator may
publish one host-wide**, which does, and is gated like every other
administrative act here.

Three things about it are worth knowing before it exists, because they are
constraints rather than choices:

- **Many sites refuse to be framed**, with `X-Frame-Options` or CSP
  `frame-ancestors`, and a parent page **cannot reliably detect that** — a
  blocked frame still fires `load`. So there is no auto-detection: the window
  always carries an *Open in a tab* button, and a line appears after three
  seconds explaining a blank one. Taking it offers to make that the default for
  that app, once.
- **An `http://` URL cannot be framed on an https desk at all.** That is mixed
  content and the browser blocks it before any request. Such an entry is set to
  open in a tab at save time, with the reason on the form.
- **It does not arrive signed in.** A container app shared this origin and its
  session cookie; a URL app is a different origin and you sign in to it yourself.

The frame is sandboxed, unlike the container frame that preceded it, and
`allow-top-navigation` is the flag it deliberately lacks: without a sandbox a
framed page can navigate the whole desk elsewhere, and this desk's login form
takes a system password that hands back a root-capable shell.

## Host panels, without the Cockpit interface

Service, log and metric views talk to **`cockpit-bridge`**, and to nothing else
of Cockpit. The bridge is a separate package from `cockpit-ws`: one binary that
speaks a JSON channel protocol over stdio, with no port, no web server, no login
page and no interface. Installing it surfaces none of Cockpit, which is why this
was chosen over putting the web console in an iframe — that would have meant a
second PAM sign-in and a second visual language inside the desk.

It runs **as the signed-in user**, spawned the way `pty.rs` spawns a shell, so
the kernel enforces what a session may read and write. That is the property that
makes it safe, and it is why none of this needed a privileged daemon of its own —
[architecture.html](docs/architecture.html) prices the alternative at Umbrel's
~35,000-line root daemon.

**The protocol is terminated in WebDesk and never handed to the browser.**
Cockpit's own client opens channels from JavaScript because Cockpit's interface
*is* JavaScript; doing the same here would hand a browser a `stream` channel,
which is a shell by another name. Every endpoint is instead a named operation
with a validated parameter — `/api/host/services`, `/api/host/journal`,
`/api/host/metrics`, and a start/stop/restart whose verb is matched against a
fixed table and whose unit must be one the desk has already listed.

On a host without the bridge every one of those answers `503` naming the package
to install rather than failing, which is what the dependency check reads. On
Arch there is no split package, so installing the bridge there installs the web
console too — that is said out loud before the button rather than discovered
afterwards.

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

**Application marks are the exception**, and they come from
[Simple Icons](https://simpleicons.org) (CC0 1.0), vendored into the same
sprite by `scripts/brand-icons.py`. They are solid glyphs, not hairline
strokes, because that is what a brand mark is; what makes them belong is that
they share the 24 grid, take their colour from `currentColor` like everything
else, and are scaled a little under full size so a filled shape does not
outweigh a stroked one beside it in the dock. Brand colours are deliberately
dropped. The marks remain their owners' trademarks; they are used here to label
the application they belong to and nothing else.

```sh
scripts/brand-icons.py            # refresh the marks from upstream
scripts/brand-icons.py --check    # fail if the committed sprite is stale
```

Two things that catch people out. **Some upstream marks are a filled square with
the shape knocked out of them**, which at dock size is a black block rather than
an icon — `STRIP_LEADING` names those, so the script drops the square and keeps
the shape, and refuses to run if upstream changes that path out from under it.
It is empty right now, since the one entry that needed it left the catalog. And
**an icon id the catalog names but the sprite does not have is a blank square in
the Apps window**, so a test asserts every one of them resolves.

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

**Installing an app** puts software on the machine for everybody, with
`flatpak install --system`; there is no version of that which the kernel can
decide on one session's behalf. Only install and remove are gated. Listing the
installed apps and opening them is open to any session, because an installed app
is part of the host rather than the property of whoever installed it — and
because opening one runs a program as *you*, under your own uid, with no
privilege anywhere in it. That is something having an account on this machine
already means.

These are the only two places in the program where authorisation is a decision
in code instead of a question for the kernel — which is exactly why they are
worth naming rather than burying.

Both are gated on something the host already decided: membership of
`wheel` or `sudo`, resolved through `getgrouplist` exactly as `sudo` resolves
it. An SSSD- or LDAP-provided `wheel` works without being told about. A session
outside those groups gets `403` from every update and app-management route, and
the check is on each route rather than on the button — the controls are hidden
for non-admins, but hiding a button is not an access control.

Installing an app trusts Flathub and whoever publishes that application, for as
long as it is on the machine. Flatpak verifies its own repository signatures, so
this is better than the container registries this used to reach — but a
verified download of somebody else's code is still somebody else's code, and it
then runs in *your* session with whatever its manifest asked for. What limits
the blast radius is that the set of application ids is fixed in the binary, and
that `scripts/flathub-entry.py` prints the permissions an id asks for at the
moment somebody is deciding whether to add it.

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
src/catalog.rs  the fixed list of installable apps
src/apps.rs     installing, running and removing apps; the state file
src/flatpak.rs  installing an application, and what it needs on the host first
src/deps.rs     what this host is missing, and installing it on one press
src/session.rs  the compositor an app is drawn into, started as the user
src/rfb.rs      the app's pixels, from a unix socket to a browser WebSocket
src/systemd.rs  the user unit an app runs in, and its template
src/which.rs    finding a program on PATH
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
| **Scene** | jump to a state — sign-in, a failed sign-in, the file manager, the editor, the terminal, System with an update pending / running / failed, a non-admin session, a permission-denied listing, four windows at once, the rename and delete dialogs, Apps with something installed / with nothing / on a host missing a compositor / after a failed install |
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

The app entries are the one piece of mock data with a source of truth elsewhere,
so they are not trusted to stay in step on their own: `scripts/preview.py` reads
the real ones out of `src/catalog.rs` on each load and the shim takes each
entry's name, icon, application id and prose from there, keeping only the window
sizes of its own. Drift is corrected rather than drawn, and named in the console
so the stale copy in `mock.js` can be fixed. Editing an entry reloads the tab the
same way editing `ui/` does.

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
| GET | `/api/apps/catalog` | what may be installed, and whether this session may |
| GET | `/api/apps/list` | installed apps, each with its state *in this session* |
| POST | `/api/apps/open` | `{slug}` — starts your own session, returns its `ws` |
| POST | `/api/apps/close` | `{slug}` — quits your own session |
| POST | `/api/apps/resize` | `{slug, width, height, scale}` — your session's output |
| GET | `/ws/rfb/<slug>` | WebSocket: RFB, from your own session's unix socket |
| GET | `/api/deps` | what this host is missing before an app will run |

These additionally require the session to be in an admin group, and return
`403` otherwise:

| Method | Path | |
| --- | --- | --- |
| GET | `/api/system/info` | build, host, and whether this session may update |
| POST | `/api/apps/install` | `{slug}` — returns once the download is handed off |
| POST | `/api/apps/remove` | `{slug, accept_uninstall}` — refuses once, says what it will do |
| GET | `/api/apps/status` | state, phase and log tail of the current or last install |
| POST | `/api/deps/install` | `{keys}` — install what the host is missing |
| POST | `/api/update/check` | compare the running commit with the tracked ref |
| POST | `/api/update/apply` | start an update; returns as soon as it is handed off |
| GET | `/api/update/status` | state, phase and log tail of the current or last run |

`/api/system/info` is the exception — any session may read it, but it reports
`updates.allowed: false` for one that may not update.

**There is no start and no stop**, and there used to be both. They were the
container engine's verbs. An app installed once for the machine and run once per
person has nothing host-wide to put into either state: opening starts it and
quitting ends it, both are per session, and both are open to anyone signed in.

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
- **Installs are one at a time**, host-wide. A second one is refused while the
  first is running rather than queued.
- **An app drawn on this host has no sound.** RFB carries none. That is fine
  for an editor and disqualifying for a media player, and
  `scripts/flathub-entry.py` flags an application that asks for PulseAudio
  before you add it.
- **Its clipboard is text only**, both directions. Images and file lists do not
  cross.
- **Resolution follows the window only under Sway.** wlroots brings a headless
  output up at a hardcoded 1280×720 and no compositor has a flag for anything
  else, so the size has to be applied afterwards. Two things stop the obvious
  route: `wayvnc` through 0.7.2 — Debian, Ubuntu and EPEL 9 — never registers a
  handler for a client's `SetDesktopSize`, so noVNC's request is received and
  dropped with nothing logged; and `cage` cannot resize at all, because asking
  it to trips an assertion in wlroots' scene layout and the compositor dumps
  core. So WebDesk asks the compositor directly, out of band, through
  `swaymsg` — which means **an app under Sway follows the window, and an app
  under cage is fixed at 1280×720 and scaled by the browser.** Sway is offered
  first for exactly this reason; cage stays because it is the only compositor
  packaged for Enterprise Linux 10, where Sway is not.
- **Sway is in no EPEL generation**, so Enterprise Linux gets the fixed-size
  behaviour even on 10, where cage is available. On 9 there is neither.
- **Latency is VNC latency.** Fine on a LAN; worse than a purpose-built encoder
  across a WAN.
- **Drawn apps do not work on the EL9 generation at all.** Neither compositor is
  packaged there: `cage` starts at EPEL 10 and Sway is in no EPEL at all.
  `wayvnc` is in both, but the compositor is the half that is missing. Rocky and Alma 10 are one
  `dnf` away once EPEL is enabled; enabling EPEL is a third-party repository and
  WebDesk will not do it behind your back.
- **A drawn app is confined by its own Flatpak manifest, not by ours.** An
  application that asks for the whole of `home` has a filesystem sandbox no
  narrower than a container's. It is still *your* files rather than everyone's,
  which is the improvement, but it is not blanket confinement.
- **`cockpit-bridge` is not split from the web console on Arch**, so installing
  it there installs Cockpit's interface too — the one thing the host panels are
  arranged to avoid.

## Not built yet

Deliberately out of scope for this pass: service and log viewers, storage,
networking, users, multi-window terminals per session, and drag-and-drop
upload.

For apps specifically: **updating an installed one.** The mechanism is there
and works — `flatpak::update` in the binary — with nothing calling it: no
button, no route, no scheduled check. That is a wire to run rather than a
mechanism to invent, and it is the largest remaining gap in this half of the
program.

Also missing: **defining your own catalog entry**, which is the deliberate
omission described in [Apps drawn on this
host](#apps-drawn-on-this-host); pinning an application to a version rather than
taking whatever Flathub has; and reconciling the app list against Flatpaks
installed or removed behind WebDesk's back.

And **[URL apps](#apps-you-supply-a-url-for) are designed and not built.**
`docs/url-apps.md` is the whole of it today.

Native desktop applications are not on this list because they are not on this
path at all. A browser cannot host a native window, so the only way to show one
is to stream its pixels — a different architecture, described in
`docs/architecture.html`.

For the update path specifically: rollback to the previous build and a
scheduled check are both missing and both worth having.
