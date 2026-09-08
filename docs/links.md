# Links

*Applications this desk points at rather than ones it runs.*

**Built.** `src/links.rs` is the server, `ui/app.js` the window and the tile.
This document is what the design was and what it became; where the two differ,
the code is right and this says which decisions moved.

---

## What this replaces

WebDesk tried twice to run applications for you, and both attempts are gone.

**Containers.** A fixed list of images in the binary. Install pulled one, created
a container, published it on a loopback port and reverse-proxied it onto
WebDesk's own origin at `/app/<slug>/`. That cost a container engine to detect
and drive, a port allocator, a bind-mount validator, `PUID`/`PGID`, a shared
memory size, a render-node passthrough, a cookie rewriter and an
`X-Forwarded-Prefix` convention. And it cost a rule: an entry had to work when
served under a path prefix, so "can this app live at `/app/<slug>/`" decided what
could be in the catalog at all -- a question about WebDesk's proxy rather than
about the application.

**Streamed Flatpaks.** A fixed list of application ids. Install put one on the
host with `flatpak --system`; opening it started a headless compositor and an RFB
server in the opener's own systemd user session and carried the pixels to a
canvas. Simpler than a container in every way that mattered, and it needed
`flatpak`, `sway` or `cage`, and `wayvnc` on the host. On Enterprise Linux 9 --
which is what the deployment host turned out to be -- no compositor is packaged
in any repository at any version, so the entire catalog was uninstallable there.
An arrangement that depends on three host packages is an arrangement that does
not work on the host you have.

Both were WebDesk running a smaller, worse copy of machinery the operator already
had. A machine that runs this very often already runs things: with Compose, with
units, behind a reverse proxy somebody configured years ago.

**So the answer is: run it yourself, and tell the desk where it is.**

| | the containers | the streamed apps | a link |
| --- | --- | --- | --- |
| what WebDesk runs | a container, as root | a compositor and a Flatpak, as you | nothing |
| what it fetches | a multi-gigabyte image | a few hundred megabytes | nothing |
| what it needs on the host | a container engine | flatpak, a compositor, wayvnc | nothing |
| what it proxies | every request and websocket | RFB over a unix socket | nothing |
| who may add one | nobody; fixed in the binary | nobody; fixed in the binary | you |
| what a bad entry does to the host | runs code as root | runs code as you | nothing |

The last row is the one that changes the design. The catalog lived in the binary
because running a container is a way to run arbitrary code as whoever owns the
engine, so the set of them had to be a property of the build. **A link runs
nothing.** Your browser fetches a page from a server you named. WebDesk executes
no code, installs no software and opens no port. The argument that kept the
catalog closed does not reach it, which is exactly why this is the thing users
may add and the catalog was not.

## The rule everything else hangs on

**WebDesk never fetches the URL.** Not to check it is alive, not to fetch a
favicon, not to proxy it. There is no HTTP client in `src/links.rs` and there
must never be one.

A server-side fetch would be a request-forgery primitive handed to every
signed-in session: `http://169.254.169.254/`, `http://127.0.0.1:2375/`, every
service on every network this host can see and the person at the keyboard cannot.
It would also drag back the whole proxy that was just deleted -- cookie
rewriting, header rewriting, prefix negotiation, TLS to an upstream. The browser
fetches it. That is not a compromise; it is why this feature is one file.

## The shape of one

```json
{
  "id": "k3f9q2",
  "name": "Grafana",
  "url": "https://grafana.internal.example/",
  "icon": "a-globe",
  "glyph": { "w": 24, "h": 24, "shapes": [ { "t": "path", "a": { "fill": "currentColor", "d": "M12 …" } } ] },
  "open": "frame",
  "width": 1400,
  "height": 900,
  "scope": "host",
  "added": 1757203200,
  "actor": "hutson"
}
```

`id` is **generated, never typed**. Six characters from the system generator,
and its own key space: a URL app is addressed as `url:<id>` everywhere the desk
addresses an app, so it can never collide with a catalog slug, and adding a
catalog entry later can never shadow somebody's tile. This is worth doing even
though a typed slug would be prettier. The alternative is a validation rule, a
collision refusal, and a failure mode where an upgrade quietly repoints an icon.

`icon` is always set and is the fallback; `glyph` is a pasted icon and is absent
when there is not one. See [Icons](#icons).

`open` is `frame` or `tab`, and see [How it renders](#how-it-renders) for why it
is a stored per-link choice rather than a global preference or a guess.

`scope` is `me` or `host`, and see [Two scopes](#two-scopes-and-only-one-of-them-needs-new-privilege).

`width` and `height` are the window's first size, for the same reason a catalog
entry carries one: a window created at a guessed size means the first thing
anybody sees is the window resizing itself.

---

## Two scopes, and only one of them needs new privilege

### Personal — any user, no gate

A link in your own dock grants you nothing you did not already have. You have a
browser; you could have typed the URL into it. So a personal URL app is not an
administrative act and is not gated on one.

It also needs **no new privileged code in WebDesk**, which is the part of this
design worth arguing for. The store is a file in your own home:

```
~/.config/webdesk/links.json
```

and it is written the way every other file in your home is written — through the
helper described in [How privileges work](../README.md#how-privileges-work). The
daemon runs as root; on login it forks a child that permanently drops to your
uid before touching anything; every filesystem operation for your session goes
through that child. Writing this file is one more `write` down that channel.

The consequence is that **the kernel decides**, exactly as it does for the file
manager. There is no ownership check to write, no "is this your app" test to get
wrong, and no way to reach another account's list, because the process doing the
reading and writing is running as you and cannot. That is the same property the
rest of this program is built on, arrived at for free.

### Host-wide — admin, for the whole machine

An operator who runs six internal tools does not want six people each adding six
tiles. So an administrator may publish one, and it appears for everybody:

```
/var/lib/webdesk/links.json
```

This one *is* a new privileged write — it is root-owned state that changes what
every session sees — so it is gated on the same administrative group as
installing an app and updating the binary, checked on the route rather than on
the button, exactly as those are. It is a third place where authorisation lives
in code rather than in the kernel, and it should be named in the README's [two
exceptions](../README.md#the-two-exceptions) when it is built, which will then
be three.

### Merging them

The dock shows catalog apps, then host-wide URL apps, then personal ones. Two
entries may share a *name* and that is cosmetic and allowed; they cannot share an
id. A personal entry never shadows a host one and is never shadowed by it,
because both are simply in the list.

---

## What the server checks

WebDesk never fetches the URL, so none of this is about protecting the host.
Every rule here protects the *browser session* — and one of them is not
obvious.

**The scheme is `http` or `https`, and nothing else.** This is the rule that
matters most and it is the cheapest to get wrong by omission. A stored string
that reaches an `iframe.src` or a `window.open` is script execution if it starts
with `javascript:`; `data:` and `blob:` are the same hole wearing a different
hat, and `file:` reads the host's disk into a frame. An allow-list of two
schemes closes all of it. A deny-list would not: there are more URL schemes than
anybody's deny-list, and browsers add them.

**It must parse as an absolute URL with a host.** Rejected before storing, so a
malformed entry cannot sit in the list waiting to be clicked.

**It must not be WebDesk's own origin.** See [the sandbox](#the-sandbox-and-the-one-attribute-that-must-not-be-there)
— the sandbox that protects the desk from a framed page depends on the frame
being cross-origin, and this is the check that guarantees it.

**No credentials in the URL.** `https://user:hunter2@host/` would put a password
in a JSON file in cleartext, echo it back into an edit form, and print it in a
tooltip. Refused, with a sentence saying why rather than a validation error that
reads like a typo.

**A length cap**, in the low thousands. Nothing legitimate is longer and it keeps
a pathological value out of a file the dock parses on every load.

And on the client: the URL is assigned to `iframe.src` or passed to
`window.open`, and is **never interpolated into HTML**. The desk builds its DOM
with `createElement` and `textContent` almost everywhere already; this is one
more place where that has to stay true.

---

## How it renders

This is the part that needs the most honesty, because the obvious design does
not work and the reason is not WebDesk's fault.

### An iframe, when the site allows it

A window with one iframe in it, the way the container apps were shown. Most
internal tools — Grafana, Home Assistant, a router's interface, anything on your
own network — frame perfectly well.

### Many sites refuse to be framed, and you cannot reliably ask first

`X-Frame-Options: DENY` and CSP `frame-ancestors 'none'` are how a site says it
will not be embedded, and nearly every large public site sends one. The browser
enforces it by refusing to render the frame. What lands in the window is
nothing.

The tempting move is to detect this and switch to a tab automatically. **It
cannot be done reliably from the parent page**, and a design that pretends
otherwise produces a UI that is wrong in a way nobody can debug:

- The parent cannot read the child's document, headers or status. That is the
  same-origin policy doing its job.
- A frame blocked by `X-Frame-Options` still fires `load` in Chromium — on the
  error page — so `load` proves nothing.
- A frame that has not fired `load` after N seconds might be blocked, or might be
  a slow internal service on a cold start.

So: **do not detect. Make the recourse permanent and explain the blank.**

- The window always carries an **Open in a tab** button, from the first frame,
  not conditionally.
- If `load` has not fired after three seconds, a line appears over the frame:
  *"If this window is blank, `grafana.internal.example` does not allow being
  shown inside another page. Open it in a tab."*
- Taking that offer asks once: **"Always open Grafana in a tab?"** Yes writes
  `"open": "tab"` on the entry and the question never comes back.

That turns an undetectable condition into a one-time click, which is the best
available outcome and is better than a heuristic that guesses wrong on a slow
service.

### An http URL on an https desk cannot be framed at all

This one *is* deterministic and should be decided at save time rather than
discovered at open time.

WebDesk serves https by default. A browser will not load an `http://`
subresource into an https page — it is mixed content, and it is blocked before
any request is made. So an `http://` URL on a TLS desk has `"open": "tab"`
forced, not offered, and the form says so as you type it:

> `http://` pages cannot be shown inside this desk, because this desk is on
> https and your browser will not mix the two. This one will open in a tab.
> Give it an `https://` address, or set `WD_TLS=off` if this host is on a
> network where that is appropriate.

The related trap is worth a line in the same place: Chromium's private-network
access rules also restrict requests from a public or secure page to a private
address, so `https://desk.example.com` framing `http://10.1.2.40:8080` fails for
two reasons rather than one.

### It does not arrive signed in

A container app shared WebDesk's origin and therefore its session cookie, and
arrived already authenticated. **A URL app does not.** It is a different origin;
WebDesk's cookie is `SameSite=Strict` and scoped to the desk. You sign in to the
site the way you would in any tab, and your browser keeps that cookie for it
afterwards.

This is a real behavioural difference from what it replaces and belongs in the
form's own prose, not only here. It is also the correct behaviour: the
alternative — WebDesk holding credentials for third-party services — is a
password manager, and writing one of those was never on offer.

---

## The sandbox, and the one attribute that must not be there

The container-app frame was deliberately **not** sandboxed, and the reasoning
was sound for what it was: the app was on WebDesk's own origin, needed the
session cookie and its own storage, and could not be reached at all without
getting past WebDesk's login first.

None of that holds for a URL you typed. A URL app frame **is** sandboxed:

```html
<iframe sandbox="allow-scripts allow-same-origin allow-forms
                 allow-popups allow-popups-to-escape-sandbox allow-downloads">
```

Read that list by what is missing from it.

**`allow-top-navigation` is absent, and that is the point of the whole
attribute.** Without a sandbox, a framed page can set `window.top.location` and
navigate the entire desk somewhere else. Consider what that is worth to an
attacker: WebDesk's login form is a username and a system password that hands
back a root-capable shell. A tile in somebody's dock that quietly replaces the
desk with a convincing copy of that form is the best phishing position on the
machine. Omitting `allow-top-navigation` makes the attempt a no-op with a console
message.

`allow-same-origin` is present, and it is the flag to think about twice, because
`allow-scripts` plus `allow-same-origin` lets a document remove its own sandbox
— **when it is same-origin with the embedder.** The framed page could reach into
its own `<iframe>` element and strip the attribute. That is precisely why the
server refuses a URL on WebDesk's own origin: the two rules are one rule, written
in two files, and neither is safe without the other. Whichever gets implemented
first should carry a comment naming the other.

It is present rather than dropped because dropping it puts the framed page in an
opaque origin with no cookies, no `localStorage` and no `IndexedDB`, which breaks
the login of very nearly every application anybody would want to add. A tile that
signs you out every time you click it is not a working tile.

`allow-popups-to-escape-sandbox` is there so that a link the app opens in a new
tab is a normal tab rather than another sandboxed document, which is what a
"download this report" or an OAuth hand-off needs.

Two things deliberately not in the list, so that a future reader knows they were
considered: `allow-modals`, because the desk's own rule is that nothing calls
`alert()` or `confirm()` and a framed page blocking the whole browser on a modal
is exactly the failure that rule exists to prevent; and `allow-storage-access-by-user-activation`,
which is about third-party cookie access and is a question to answer when
somebody has a site that needs it, not before.

---

## Icons

Ten built-in marks, and a paste box past them.

The ten come from `ui/ui-icons.svg` and are an allow-list in `src/links.rs`
rather than "any id in the sprite", because an id the sprite has not got draws an
empty square, silently, in somebody's dock. They are enough to tell a router from
a media server and no more.

**Past them: paste an icon's SVG from [iconify.design](https://iconify.design).**
362,000 icons across 238 sets, including `selfh.st`, which has a mark for very
nearly every application anybody self-hosts.

### Why paste, and not a picker or a vendored set

Both alternatives were measured before this was written.

| | icons | raw | gzipped |
| --- | --- | --- | --- |
| Lucide | 1,816 | 595 KB | 89 KB |
| Simple Icons | 3,459 | 4.8 MB | 1.9 MB |
| selfh.st | 7,124 | **13.2 MB** | 2.6 MB |

The binary is 2.9 MB. Vendoring the one set a homelab actually wants is more than
four times the whole program — and it would still have been a fixed list, which
is the thing being escaped.

A picker means a search box, a results grid, a debounce and a request to
`api.iconify.design` on every keystroke. That is a third-party dependency on the
one screen where this desk is otherwise self-contained, and it is useless on a
host with no route out. Fetching *at paint time* would be worse still: a
third-party request in front of every dock, which is the same beacon this project
already refused for favicons.

**Pasting has neither cost.** You already have the icon on your clipboard by the
time you reach the form. The geometry is stored in `links.json` beside the name
and the URL, so from the moment it is saved the icon is as local as the rest of
the desk — an air-gapped host draws it exactly as well as a connected one, and
nothing is ever fetched again.

Measured on five real icons: **307–4,014 bytes** each after reduction. The 4 KB
one is a 512-grid app logo; the 24-grid line icons are 307–437.

### What is stored is not what was pasted

This is the security argument and it is the reason the feature is shaped this
way.

SVG is a document format. It carries `<script>`, `onload=`, `<foreignObject>`
full of HTML, `href="javascript:"`. A stored string that ever reached
`innerHTML` would be script execution in the desk's own origin — the origin whose
login form takes a system password and hands back a root-capable shell. And the
string would be reachable by anyone who could write `links.json`, not only by
whoever pasted it.

So there are three barriers and no string survives any of them:

1. **The browser parses once, inertly, and throws the markup away.**
   `DOMParser` with `image/svg+xml` builds a document that runs no scripts and
   resolves no external references. What comes out is read attribute by
   attribute against a table of seven tags and their geometry.
2. **`src/links.rs` checks the structure again**, because a server that trusts
   its client has no rule at all. A `d` must be the path alphabet and numbers; a
   `transform` may name six functions and nothing else; `fill` and `stroke` may
   be `currentColor` or `none`. That last one is what makes this monochrome, and
   it disposes of `fill="url(#grad)"` without a rule of its own — gradients are
   not among the kept elements, so a reference to one could only dangle.
3. **The dock rebuilds by construction** — `createElementNS` and `setAttribute`,
   never markup, never a second parse.

There is no path from a stored byte to an executed one. The `d` attribute is
where that claim rests: it is a command alphabet and a set of numbers, with no
syntax for a URL, a script, an entity or an element. A string containing only
those characters *cannot express* anything but a shape — which is a stronger
statement than "nothing bad was found in it".

Verified against 16 hostile inputs — `<script>` children, `onload`/`onclick`,
`<foreignObject>`, external `<use>`, `javascript:` anchors, `<image href>`,
`<style>` blocks and attributes, `<set attributeName="href">`, quote-breakout
inside a `d`, `url()` in a fill and in a transform, and a missing viewBox. Every
one is dropped or refused; the output is geometry in all cases.

### Monochrome, deliberately

An icon takes the colour of the control it sits in, like every other icon here.
That keeps a pasted mark from shouting over the hand-drawn ones beside it in the
dock — the same reason the old brand-mark script dropped brand colours — and it
makes the paint allow-list two values long instead of a colour parser.

A colour logo pastes fine; it arrives as a monochrome silhouette, and the form
says so: it counts what was left out rather than quietly drawing less.

## Routes

```
GET    /api/links            merged list: host-wide first, then this session's own
POST   /api/links            {name, url, icon, open, width, height, scope}
PUT    /api/links/{id}       the same body; the id, the added time and the
                             original author are kept
DELETE /api/links/{id}
```

Any session may call all four for its own links. `scope: "host"` on create, and
any write to a host-wide link, require the administrative group -- checked on the
route, not on the button.

`GET /api/links` answers `{links, admin, icons}`. `icons` is the allow-list the
form draws its picker from, sent rather than hardcoded in the browser so the two
cannot disagree about what the sprite has. Each link carries `scope` and
`editable`; `editable` is a property of the *pair* -- this link, this session --
and is computed on the server so the browser is not left inferring it from two
other fields.

There is no second list and no kind flag anywhere. The dock draws Files,
Terminal and then these.

---

## What the UI does

- **Apps window** (the dock's first built-in): one grid of tiles in the order
  the dock is in -- Files, Terminal and System, then the host-wide links, then
  your own, then a dashed **Add a link** tile. Clicking a tile opens it; alt- or
  middle-click opens another, the same gesture the dock takes. No headings: a
  desk with eight tiles on it does not need a filing system, and scope is on a
  link's own page, where somebody goes when they want to know it.
- **A page behind each link**, reached by the chevron in a tile's corner or by
  right-clicking it: the address in full, how it will open, who else can see it,
  the window size, and then Open / Open in a tab / Edit / Remove. `editable`
  decides whether Edit and Remove are drawn, and a link somebody else published
  says so in a line instead of simply losing two buttons.

  This was a list of rows, and they were the rows the app catalog left behind. A
  row earns its width when each one carries a description and Install, Start and
  Stop; a link is a name and an address, so the rows were a column of identical
  shapes wearing identical buttons. Everything the sub-line carried is on the
  page now, in sentences, where there is room to say why an `http://` tile will
  not frame rather than to leave it as three words after a dot.
- **The form** is one dialog for adding and editing, with the ten built-in marks
  as a row and a dashed eleventh button that opens the paste box. Under the
  address is a live line that says the two things a person cannot see for
  themselves: what a
  scheme-less entry will be turned into, and that an `http://` page cannot be
  framed on an https desk. When the address is loopback it adds the warning
  below, which is the one that catches people out.
- **Dock**: a tile each, after Files and Terminal. Alt- or middle-click opens it
  in a tab whatever the link says, which is the ordinary browser gesture and is
  also the fastest way out of a page that will not frame.
- **Window**: one iframe, with Reload and Open-in-a-tab in the title bar from the
  first frame rather than appearing with the failure message.

### `localhost` means the browser's machine

The sharpest thing about this feature and the least obvious. WebDesk stores the
address; *your browser* fetches it. So `http://localhost:8096` is Jellyfin on the
machine you are sitting at, not on the machine WebDesk runs on. That is right
when you are at the server and silently wrong from anywhere else, and the symptom
is a tile that works for one person and not for another.

The form says so as you type it. It is not refused, because browsing from the
host itself is a real thing to do.

---

## What moved between the design and the code

- **The store is `links.json`, not `urlapps.json`**, and the routes are
  `/api/links`. "URL app" was a phrase for a thing that did not exist yet; once
  it existed it was a link.
- **Removal asks once, without ceremony**, where the design left it open.
  Removing a link deletes a name and an address: nothing is uninstalled, nothing
  is lost, and putting it back is retyping one line.
- **Scheme detection is not the obvious regex.** `/^[a-z][a-z0-9+.-]*:/` matches
  `localhost:8096` -- the commonest input this feature has -- and reads
  `localhost` as the scheme. A bare `word:` is a scheme only when what follows is
  not a port number. Getting this wrong meant nothing was prepended and the
  server refused the address the user most wanted to add.
- **A hostile scheme is passed through unchanged rather than prefixed.**
  `javascript:alert(1)` could have had `https://` put in front of it, which would
  have made it harmless and also incomprehensible. It goes to the server as
  typed, and the server refuses it by name.

---

## Still open

1. **Should a host-wide link be hideable per user?** An operator publishes six
   tiles; somebody uses two. A per-user hidden list in the personal store is a
   few lines and stops the dock becoming somebody else's opinion.
2. **Ordering.** Host-wide first, then personal, each in the order added.
   Alphabetical is predictable; drag-to-reorder is what people want the moment
   there are more than about five.
3. **Import.** Twenty internal tools is twenty passes through the form. A
   textarea of `name<tab>url` lines is the cheap version.
4. **A link whose site has gone.** Not detectable from here, by the same argument
   as the frame refusal, and probably nothing should happen: a bookmark to a dead
   site is an ordinary thing to have.
