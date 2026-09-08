# Apps you supply a URL for

**Status: designed, not built.** This file is the design. Nothing in
`src/` implements it yet, and the README says so in [Not built
yet](../README.md#not-built-yet).

---

## What this replaces, and why it is not the same thing

WebDesk used to be able to run a web application for you. It kept a fixed list
of container images, pulled one, created a container, published it on a loopback
port, and reverse-proxied it onto its own origin at `/app/<slug>/`. That is gone
— the engine, the proxy, the port allocator, the bind mounts and the catalog
entries that described them.

The proximate reason it went is that it cost far more than it looked like it
cost. A container entry had to answer for a published port, a state directory,
`PUID`/`PGID`, a shared-memory size, a clock, a render node, and whether the
application would survive being served under a path prefix. The last of those
decided membership: an application that assumed it owned `/` emitted
root-absolute links, escaped its prefix, and rendered as a blank frame. So the
question "should this be in the catalog" was mostly the question "can this be
made to work behind our proxy", which is a question about our proxy and not
about the application.

The deeper reason is that it was the wrong job. A host that runs WebDesk very
often already runs things — with Compose, with systemd units, with a package
manager, behind a reverse proxy somebody already configured. WebDesk running a
*second*, smaller, less capable copy of that machinery, for a hardcoded list of
nine images, was competing with the operator's own tooling and losing.

**So the replacement is not "run my container". It is "I already run this
somewhere; put it on the desk."** You supply a URL. WebDesk stores it, draws a
tile for it, and opens a window with that page in it.

That is a much smaller thing than what it replaces, and the smallness is the
feature. It is worth being precise about how much smaller:

| | the container apps | a URL app |
| --- | --- | --- |
| what WebDesk runs | a container, as root | nothing |
| what WebDesk fetches | a multi-gigabyte image | nothing |
| what WebDesk proxies | every request and websocket | nothing |
| what the catalog constrains | which images may run | nothing — it is not a catalog entry |
| who may add one | nobody; it was fixed in the binary | you, for yourself |
| what a bad entry can do to the host | run arbitrary code as root | nothing |

The last row is the one that changes the design. The catalog is fixed in the
binary because "the set of things that may be run on this host is a property of
the build" — a container is a way to run arbitrary code as whoever owns the
engine, so the list of them is reviewed like code. **A URL app runs nothing.**
Your browser fetches a page from a server you named. WebDesk executes no code,
installs no software, opens no port and makes no outbound request. The whole
argument that keeps the catalog closed simply does not reach this, which is
exactly why this can be the thing users add and the catalog cannot.

**WebDesk must never fetch the URL.** That is the single most important line in
this design and it is worth stating as a rule rather than leaving as an
implementation detail. A server-side fetch would be a server-side request
forgery primitive handed to every signed-in user: `http://169.254.169.254/`,
`http://127.0.0.1:2375/`, every service on every network this host can see but
you cannot. It would also drag back the whole proxy — cookie rewriting, header
rewriting, prefix negotiation, TLS to an upstream — which is what was just
deleted. The browser fetches it. That is not a compromise; it is the reason this
design is three hundred lines and the thing it replaces was three thousand.

---

## The shape of one

```json
{
  "id": "k3f9q2",
  "name": "Grafana",
  "url": "https://grafana.internal.example/",
  "icon": "a-globe",
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

`open` is `frame` or `tab`, and see [How it renders](#how-it-renders) for why it
is a stored per-app choice rather than a global preference or a guess.

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
~/.config/webdesk/urlapps.json
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
/var/lib/webdesk/urlapps.json
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
Apps window's own prose, not only here. It is also the correct behaviour: the
alternative — WebDesk holding credentials for third-party services — is a
password manager, and there is [one of those in the
catalog](../README.md#apps-drawn-on-this-host).

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

A URL app picks a mark from `ui/ui-icons.svg`, the same sprite the catalog draws
from, defaulting to a new `a-globe`. The picker shows the handful that make sense
for a link — a globe, a chart, a terminal, a book, a gear.

**Fetching the site's favicon is deliberately not the default**, and the reason
is not difficulty. It would mean every paint of the dock making a request from
the desk's page to a third-party server, announcing to that server when somebody
opened their desk and from where. For an internal Grafana that is nothing; for a
tile pointing at a public site it is a beacon nobody asked for. It is also
unreliable in exactly the case this feature is for: internal tools frequently
have no favicon, and the fallback would be a broken image in a dock.

If it is wanted later, the shape is an opt-in checkbox on the entry, an
`<img>` the browser loads directly, and the sprite mark underneath it as the
fallback. Not a host-side fetch — see the rule above.

---

## Routes

```
GET    /api/urlapps            merged list: host-wide, then this session's own
POST   /api/urlapps            {name, url, icon, open, width, height, scope}
PUT    /api/urlapps/{id}       same body; only the fields sent are changed
DELETE /api/urlapps/{id}
```

Any session may call all four for `scope: "me"`. `scope: "host"` requires the
administrative group, checked on the route.

`GET /api/apps/list` does **not** grow a second kind. URL apps are not installed
apps: nothing about them involves Flatpak, a user unit, an RFB socket or a state
word, and folding them into that response would mean every consumer of it
branching on a kind again — which is the shape this whole change removed. They
are a separate list, and the dock concatenates two lists, which is a line of
JavaScript.

---

## What the UI gains

- **Apps window**: a third group, *Links*, under *Installed* and *Available*,
  with an **Add a link** button. Rows show the name, the host part of the URL,
  and whether it opens framed or in a tab. Host-wide entries are marked as such
  and are only editable by an administrator.
- **Dock**: a tile each, after the installed apps.
- **Window**: for `frame`, an iframe with a Reload and an Open-in-a-tab button
  in the title bar — which is what `frameApp` did before it was deleted, and
  `git show HEAD~1:ui/app.js` is where to read it. For `tab`, clicking the tile
  opens the tab and no window is created at all.

---

## Open questions

Worth deciding before building rather than during.

1. **Should a host-wide entry be hideable per user?** An operator publishes six
   tiles; somebody uses two. A per-user hidden list in the personal store is four
   lines and prevents the dock becoming somebody else's opinion. Probably yes.
2. **Ordering.** Alphabetical is predictable; drag-to-reorder is what people
   want the moment there are more than about five. Alphabetical first.
3. **Import.** An operator with twenty internal tools will want to paste a list
   rather than fill a form twenty times. A textarea of `name<tab>url` lines is
   the cheap version and can wait for somebody to ask.
4. **What happens to a URL app whose site has gone.** Nothing detectable from
   here, by the same argument as the frame refusal. Probably nothing should
   happen: a bookmark to a dead site is an ordinary thing to have.
