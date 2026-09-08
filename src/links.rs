//! Links: applications you point WebDesk at rather than ones it runs.
//!
//! A link is a name, a URL and an icon. It draws a tile in the dock and opens a
//! window with that page in it. That is the whole feature, and the smallness is
//! the point -- it is what is left after three larger attempts at the same
//! problem were removed.
//!
//! **WebDesk used to run the application for you.** It kept a fixed list of
//! container images, pulled one, created a container, published it on a loopback
//! port and reverse-proxied it onto its own origin at `/app/<slug>/`. Then it
//! kept a fixed list of Flatpaks, installed one host-wide, ran it under a
//! headless compositor and streamed the pixels to a canvas. Both are gone. The
//! first cost an engine, a port allocator, a bind-mount validator and a
//! path-prefix question that decided what could be in the catalog at all; the
//! second cost a compositor, an RFB server, a per-user systemd unit and a video
//! encoder, and did not work at all on a distribution generation with no
//! compositor packaged.
//!
//! Both were WebDesk running a smaller, worse copy of machinery the operator
//! already had. A host that runs this very often already runs things -- with
//! Compose, with units, behind a proxy somebody configured years ago. So the
//! answer now is: run it yourself, wherever you already run things, and tell the
//! desk where it is.
//!
//! **The rule this file is built on: WebDesk never fetches the URL.** Not once,
//! not to check it is alive, not to fetch a favicon, not to proxy it. The
//! browser fetches it. A server-side fetch would be a request-forgery primitive
//! handed to every signed-in session -- `http://169.254.169.254/`,
//! `http://127.0.0.1:2375/`, every service on every network this host can see
//! and the person at the keyboard cannot. It would also drag back the whole
//! proxy that was just deleted: cookie rewriting, header rewriting, prefix
//! negotiation, TLS to an upstream. There is no HTTP client in this file and
//! there must never be one.
//!
//! **That is also why a link may be user-supplied where a catalog entry could
//! not.** The old catalog lived in the binary because a container is a way to
//! run arbitrary code as whoever owns the engine, so the set of them was a
//! property of the build. A link runs nothing. It installs nothing, opens no
//! port and executes no code on this host. The argument that kept the catalog
//! closed does not reach it.
//!
//! **Two scopes, and only one of them is privileged.**
//!
//! - **Personal**, in `~/.config/webdesk/links.json`, written through the same
//!   privilege-dropping helper that writes every other file in your home. Any
//!   session may add one, because a link in your own dock grants you nothing you
//!   did not already have -- you have a browser; you could have typed the URL.
//!   There is no ownership check in this file for these, and there is nothing to
//!   get wrong: the process doing the reading and writing *is* you and cannot
//!   reach another account's list. The kernel decides, which is the property the
//!   rest of this program is built on.
//! - **Host-wide**, in `/var/lib/webdesk/links.json`, root-owned, written by
//!   this process. That one changes what every session sees, so it is gated on
//!   the administrative group, checked on the route.
//!
//! Everything the server checks is below in `validate`. None of it protects the
//! host, because nothing here touches the host; all of it protects the browser
//! session, and the sharpest one is not obvious. See the comments there.

use crate::{auth, session_of, unauthorized, AppState, Session};
use axum::extract::{Path as AxPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// Longest URL accepted. Nothing legitimate is longer, and it keeps a
/// pathological value out of a file the dock parses on every load.
const MAX_URL: usize = 2048;
const MAX_NAME: usize = 120;

/// The icons a link may name.
///
/// An allow-list rather than a free string, and the reason is not injection --
/// the value reaches `<use href="#...">`, which is a same-document reference and
/// cannot leave the page. It is that an id the sprite has not got draws an empty
/// square, silently, in somebody's dock. A name that is not here is refused at
/// the moment it is chosen instead.
const ICONS: &[&str] = &[
    "a-globe", "a-box", "a-apps", "a-terminal", "a-files", "a-home", "a-user",
    "a-layout", "a-external", "a-refresh",
];

pub const DEFAULT_ICON: &str = "a-globe";

/// How a link opens: framed in a WebDesk window, or in a browser tab.
///
/// Stored per link rather than guessed each time, because the guess cannot be
/// made reliably -- see the note on framing in `docs/links.md`. A page that
/// refuses to be framed still fires `load`, so the browser cannot tell the desk
/// which happened; what it can do is remember the answer a person gave once.
fn open_mode(v: &str) -> Option<&'static str> {
    match v {
        "frame" => Some("frame"),
        "tab" => Some("tab"),
        _ => None,
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Link {
    /// Generated, never typed, and its own key space.
    ///
    /// The dock addresses a link as `link:<id>`. A typed slug would have been
    /// prettier and would have needed a uniqueness rule, a collision refusal and
    /// a story about what happens when two scopes pick the same word. Six
    /// characters from the system generator have none of that.
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: String,
    pub open: String,
    pub width: u16,
    pub height: u16,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub actor: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Book {
    #[serde(default)]
    links: Vec<Link>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn bad(status: StatusCode, msg: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": msg.to_string() }))).into_response()
}

fn fresh_id() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..6).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect()
}

// ------------------------------------------------------------------ the host

fn host_file() -> std::path::PathBuf {
    let dir = std::env::var("WD_STATE_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/var/lib/webdesk".into());
    std::path::Path::new(&dir).join("links.json")
}

fn read_host() -> Book {
    match std::fs::read_to_string(host_file()) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_default(),
        Err(_) => Book::default(),
    }
}

/// Replace the host list in one step, so a concurrent read never sees half of it.
fn write_host(b: &Book) -> std::io::Result<()> {
    let path = host_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(b)?)?;
    std::fs::rename(tmp, path)
}

// ------------------------------------------------------------------ the user

/// Where one person's own links live.
///
/// Under `~/.config`, which is where an application's per-user settings belong
/// on every target distribution, and reached only through the helper -- so the
/// path is built from the session's own home and the file is opened by a process
/// running as them.
fn user_dir(ident: &auth::Identity) -> String {
    format!("{}/.config/webdesk", ident.home.trim_end_matches('/'))
}

fn user_file(ident: &auth::Identity) -> String {
    format!("{}/links.json", user_dir(ident))
}

/// Read this session's own links, as them.
///
/// A missing file is an empty list rather than an error: somebody who has never
/// added one has no file, and that is the ordinary state rather than a fault.
/// Unparseable is *also* an empty list, for the same reason the app book was --
/// a hand-edited file with a stray comma should cost the dock, not the session.
async fn read_user(session: Arc<Session>) -> Book {
    let path = user_file(&session.ident);
    match crate::ask(session, crate::hreq("read", &path, "", 0), Vec::new()).await {
        Ok((r, bytes)) if r.ok => serde_json::from_slice(&bytes).unwrap_or_default(),
        _ => Book::default(),
    }
}

async fn write_user(session: Arc<Session>, b: &Book) -> Result<(), String> {
    let dir = user_dir(&session.ident);
    let path = user_file(&session.ident);
    let body = serde_json::to_vec_pretty(b).map_err(|e| e.to_string())?;

    // `mkdirp`, not `mkdir`: `~/.config` may not exist on a fresh account and
    // `~/.config/webdesk` will not exist the first time. Already there is
    // success -- see `helper.rs` for why that is a second op rather than a
    // change to the one the Files window uses.
    let made = crate::ask(session.clone(), crate::hreq("mkdirp", &dir, "", 0), Vec::new()).await;
    if let Ok((r, _)) = made {
        if !r.ok {
            return Err(r.error.unwrap_or_else(|| format!("could not create {dir}")));
        }
    }

    let len = body.len();
    match crate::ask(session, crate::hreq("write", &path, "", len), body).await {
        Ok((r, _)) if r.ok => Ok(()),
        Ok((r, _)) => Err(r.error.unwrap_or_else(|| "could not save".into())),
        Err(e) => Err(e),
    }
}

// --------------------------------------------------------------- validation

#[derive(Deserialize)]
pub struct LinkBody {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    open: String,
    #[serde(default)]
    width: u16,
    #[serde(default)]
    height: u16,
    /// `"me"` or `"host"`. Only read when creating; a link does not move between
    /// scopes, because moving one means an admin check on the destination and a
    /// delete on the source, and "edit" is not the place to hide that.
    #[serde(default)]
    scope: String,
}

/// Everything the server insists on, and why.
///
/// None of this protects the host. WebDesk never fetches the URL, so there is no
/// request to forge and no port to reach: every rule here protects the *browser
/// session* that will one day click the tile.
fn validate(body: &LinkBody, own_host: Option<&str>) -> Result<Link, String> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err("a name is needed".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("that name is longer than {MAX_NAME} characters"));
    }
    if name.chars().any(|c| c == '\0' || (c.is_control() && c != '\t')) {
        return Err("that name contains a control character".into());
    }

    let url = body.url.trim();
    if url.len() > MAX_URL {
        return Err(format!("that address is longer than {MAX_URL} characters"));
    }

    // **The rule that matters most, and the cheapest to get wrong by omission.**
    // A stored string that reaches an `iframe.src` or a `window.open` is script
    // execution if it begins `javascript:`; `data:` and `blob:` are the same
    // hole wearing a different hat, and `file:` reads this host's disk into a
    // frame. An allow-list of two schemes closes all of it at once. A deny-list
    // could not: there are more URL schemes than anybody's deny-list, and
    // browsers add them.
    //
    // Matched case-insensitively because `JavaScript:` is the same scheme to a
    // browser and a different string to a naive `starts_with`.
    let lower = url.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("http://") {
        r
    } else if let Some(r) = lower.strip_prefix("https://") {
        r
    } else {
        return Err(
            "an address has to start with http:// or https://. Nothing else is accepted here \
             -- a javascript:, data: or file: address in a dock tile would run in this page \
             rather than open a site."
                .into(),
        );
    };

    // A scheme and nothing after it is not an address.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err("that address has no host in it".into());
    }

    // **No credentials in the address.** `https://user:hunter2@host/` would put a
    // password in a JSON file in cleartext, echo it back into the edit form, and
    // print it in a tooltip under somebody's dock. Refused with a sentence
    // rather than a validation error that reads like a typo.
    if authority.contains('@') {
        return Err(
            "take the username and password out of the address. They would be stored here in \
             plain text and shown back to you in the form; sign in to the site itself instead."
                .into(),
        );
    }

    if url.chars().any(|c| c == '\0' || c.is_control() || c == ' ') {
        return Err("that address contains a space or a control character".into());
    }

    // **Not WebDesk's own origin**, and this is one half of a rule whose other
    // half is in the browser. The frame a link opens in is sandboxed with
    // `allow-scripts allow-same-origin`, which is safe for a cross-origin page
    // and is *not* safe for a same-origin one: a document that is same-origin
    // with its embedder can reach its own `<iframe>` element and strip the
    // sandbox attribute, and the attribute is what stops it navigating the whole
    // desk somewhere else. Refusing the desk's own origin here is what
    // guarantees the frame is cross-origin. Neither half is safe without the
    // other; whichever is read first should lead to the other.
    //
    // The desk's own name comes from the `Host` header of the request asking,
    // because WebDesk is never told its public name and guessing one from an
    // interface address would be wrong for every operator who reaches it by a
    // domain. That makes this a best-effort check rather than an airtight one --
    // it catches the accident, and the sandbox is what handles the rest.
    if let Some(mine) = own_host {
        let mine = mine.trim().to_ascii_lowercase();
        if !mine.is_empty() && (authority == mine || authority == host_only(&mine)) {
            return Err(
                "that is WebDesk's own address. A page from this origin cannot be framed \
                 safely here -- point this at the application you want instead."
                    .into(),
            );
        }
    }

    let icon = if body.icon.is_empty() { DEFAULT_ICON } else { &body.icon };
    let Some(icon) = ICONS.iter().find(|i| **i == icon) else {
        return Err("that is not one of the icons this build has".into());
    };

    let open = open_mode(if body.open.is_empty() { "frame" } else { &body.open })
        .ok_or("a link opens either framed or in a tab")?;

    // A window with no size is a canvas created at a guess and then resized in
    // front of the person who opened it. Zero is what a client gets by
    // forgetting the field, so it is the one worth replacing rather than
    // refusing.
    let width = if body.width == 0 { 1200 } else { body.width.clamp(320, 8192) };
    let height = if body.height == 0 { 800 } else { body.height.clamp(240, 8192) };

    Ok(Link {
        id: String::new(),
        name: name.to_string(),
        url: url.to_string(),
        icon: (*icon).to_string(),
        open: open.to_string(),
        width,
        height,
        added: now(),
        actor: String::new(),
    })
}

/// Strip `:port` from a host, leaving an IPv6 literal's brackets intact.
///
/// `[::1]:443` and `[::1]` both have colons in the host part, so the port can
/// only be the tail after the closing bracket.
fn host_only(host: &str) -> &str {
    match host.rfind(']') {
        Some(close) => &host[..=close],
        None => host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host),
    }
}

fn own_host(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::HOST).and_then(|v| v.to_str().ok())
}

fn as_json(l: &Link, scope: &str, editable: bool) -> Value {
    json!({
        "id": l.id,
        "name": l.name,
        "url": l.url,
        "icon": l.icon,
        "open": l.open,
        "width": l.width,
        "height": l.height,
        "added": l.added,
        "actor": l.actor,
        "scope": scope,
        // Whether *this* session may change it. A host-wide link is readable by
        // everyone and editable by an administrator, so the answer is a property
        // of the pair and not of the link -- which is why it is computed here
        // rather than left to the browser to work out from two other fields.
        "editable": editable,
    })
}

// ------------------------------------------------------------------ handlers

/// `GET /api/links` -- everything this session should see, host-wide first.
///
/// One list rather than two, because the dock draws one row of tiles and asking
/// it to concatenate two responses would only move the join. `scope` says which
/// each came from and `editable` says what may be done to it.
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };
    let admin = session.ident.admin;

    let mut out: Vec<Value> = read_host().links.iter().map(|l| as_json(l, "host", admin)).collect();
    for l in read_user(session.clone()).await.links.iter() {
        out.push(as_json(l, "me", true));
    }

    Json(json!({ "links": out, "admin": admin, "icons": ICONS })).into_response()
}

/// `POST /api/links` -- add one.
///
/// Open to any session for `scope: "me"`. A link in your own dock is not an
/// administrative act: you have a browser, and you could have typed the URL into
/// it. `scope: "host"` changes what everybody sees and is gated.
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LinkBody>,
) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };
    let host_wide = body.scope == "host";
    if host_wide && !session.ident.admin {
        return refuse_host_scope();
    }

    let mut link = match validate(&body, own_host(&headers)) {
        Ok(l) => l,
        Err(e) => return bad(StatusCode::BAD_REQUEST, e),
    };
    link.id = fresh_id();
    link.actor = session.ident.username.clone();

    if host_wide {
        let mut book = read_host();
        if book.links.len() >= 200 {
            return bad(StatusCode::CONFLICT, "that is as many host links as this build keeps");
        }
        book.links.push(link.clone());
        if let Err(e) = write_host(&book) {
            return bad(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save: {e}"));
        }
        tracing::warn!(user = %session.ident.username, id = %link.id, url = %link.url,
                       "added a host-wide link");
        return Json(as_json(&link, "host", true)).into_response();
    }

    let mut book = read_user(session.clone()).await;
    if book.links.len() >= 200 {
        return bad(StatusCode::CONFLICT, "that is as many links as one account keeps");
    }
    book.links.push(link.clone());
    if let Err(e) = write_user(session, &book).await {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    Json(as_json(&link, "me", true)).into_response()
}

/// `PUT /api/links/{id}` -- change one, in whichever scope holds it.
///
/// Your own first. A personal link and a host link can never share an id in
/// practice, but looking in your own list first is the order that cannot
/// surprise anybody: nothing an administrator adds can shadow something of
/// yours.
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(id): AxPath<String>,
    Json(body): Json<LinkBody>,
) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };

    let mut mine = read_user(session.clone()).await;
    if let Some(pos) = mine.links.iter().position(|l| l.id == id) {
        let mut link = match validate(&body, own_host(&headers)) {
            Ok(l) => l,
            Err(e) => return bad(StatusCode::BAD_REQUEST, e),
        };
        link.id = id;
        link.added = mine.links[pos].added;
        link.actor = mine.links[pos].actor.clone();
        mine.links[pos] = link.clone();
        if let Err(e) = write_user(session, &mine).await {
            return bad(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
        return Json(as_json(&link, "me", true)).into_response();
    }

    let mut book = read_host();
    let Some(pos) = book.links.iter().position(|l| l.id == id) else {
        return bad(StatusCode::NOT_FOUND, "there is no link with that id");
    };
    if !session.ident.admin {
        return refuse_host_scope();
    }
    let mut link = match validate(&body, own_host(&headers)) {
        Ok(l) => l,
        Err(e) => return bad(StatusCode::BAD_REQUEST, e),
    };
    link.id = id;
    link.added = book.links[pos].added;
    link.actor = book.links[pos].actor.clone();
    book.links[pos] = link.clone();
    if let Err(e) = write_host(&book) {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save: {e}"));
    }
    tracing::warn!(user = %session.ident.username, id = %link.id, "changed a host-wide link");
    Json(as_json(&link, "host", true)).into_response()
}

/// `DELETE /api/links/{id}` -- take one out.
///
/// No confirmation is asked for here, and that is deliberate rather than an
/// omission: removing a link deletes a name and a URL. Nothing is uninstalled,
/// no data is lost and adding it again is retyping one line. The Apps window
/// this replaced had to ask before removing, because removing there uninstalled
/// software for every account on the machine.
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(id): AxPath<String>,
) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };

    let mut mine = read_user(session.clone()).await;
    if let Some(pos) = mine.links.iter().position(|l| l.id == id) {
        mine.links.remove(pos);
        if let Err(e) = write_user(session, &mine).await {
            return bad(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
        return Json(json!({ "ok": true })).into_response();
    }

    let mut book = read_host();
    let Some(pos) = book.links.iter().position(|l| l.id == id) else {
        return bad(StatusCode::NOT_FOUND, "there is no link with that id");
    };
    if !session.ident.admin {
        return refuse_host_scope();
    }
    let gone = book.links.remove(pos);
    if let Err(e) = write_host(&book) {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save: {e}"));
    }
    tracing::warn!(user = %session.ident.username, id = %gone.id, "removed a host-wide link");
    Json(json!({ "ok": true })).into_response()
}

fn refuse_host_scope() -> Response {
    bad(
        StatusCode::FORBIDDEN,
        format!(
            "a link for everyone on this host requires membership of {}. You can add one \
             for yourself instead.",
            auth::admin_groups().join(" or ")
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(url: &str) -> LinkBody {
        LinkBody {
            name: "Jellyfin".into(),
            url: url.into(),
            icon: String::new(),
            open: String::new(),
            width: 0,
            height: 0,
            scope: String::new(),
        }
    }

    /// The allow-list is the whole of the defence, so it is asserted from the
    /// attacking side rather than by checking that http works. Every one of
    /// these reaches an `iframe.src` if it is stored, and every one of them then
    /// runs in the desk's own page rather than opening a site.
    #[test]
    fn only_http_and_https_are_addresses() {
        for hostile in [
            "javascript:alert(document.cookie)",
            "JavaScript:alert(1)",
            "  javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "blob:https://example.com/1234",
            "file:///etc/shadow",
            "vbscript:msgbox(1)",
            "ftp://example.com/",
            "//example.com/",
            "example.com",
            "",
        ] {
            assert!(
                validate(&body(hostile), None).is_err(),
                "{hostile:?} was accepted as an address"
            );
        }
        assert!(validate(&body("http://localhost:8096"), None).is_ok());
        assert!(validate(&body("https://mail.example.com/"), None).is_ok());
        assert!(validate(&body("HTTPS://EXAMPLE.COM/"), None).is_ok());
    }

    /// A scheme with nothing behind it is not an address, and neither is one
    /// whose authority is empty because a path started immediately.
    #[test]
    fn an_address_has_a_host_in_it() {
        for empty in ["http://", "https://", "https:///path", "http://?q=1", "http://#x"] {
            assert!(validate(&body(empty), None).is_err(), "{empty:?} was accepted");
        }
    }

    /// Credentials in a URL would be stored in cleartext and echoed back into
    /// the form that edits them. Refused rather than stripped: silently changing
    /// somebody's address is worse than telling them why it will not do.
    #[test]
    fn credentials_in_the_address_are_refused() {
        // Matched rather than unwrap_err'd, so `Link` never needs a Debug impl
        // it has no other use for -- a URL in a panic message is a URL in a log.
        let e = match validate(&body("https://admin:hunter2@box.example/"), None) {
            Ok(_) => panic!("an address with credentials in it was accepted"),
            Err(e) => e,
        };
        assert!(e.contains("plain text"), "{e}");
        // And the check is on the authority, not on the whole string -- an `@`
        // in a query or a fragment is ordinary and must still be accepted.
        assert!(validate(&body("https://box.example/u?to=a@b.com"), None).is_ok());
        assert!(validate(&body("https://box.example/#a@b"), None).is_ok());
    }

    /// The desk's own origin is refused, because the frame's sandbox carries
    /// `allow-same-origin` and a same-origin document can strip its own
    /// sandbox -- which is what stops it navigating the whole desk elsewhere.
    /// With and without the port, since the Host header may carry either.
    #[test]
    fn the_desks_own_origin_is_refused() {
        let mine = Some("desk.example.net:61443");
        assert!(validate(&body("https://desk.example.net:61443/"), mine).is_err());
        assert!(validate(&body("https://desk.example.net/"), mine).is_err());
        // A different host on the same domain is somebody else's application and
        // is fine.
        assert!(validate(&body("https://other.example.net/"), mine).is_ok());
        // And with no Host header to compare against, nothing is refused on
        // this ground -- the sandbox is the half that always holds.
        assert!(validate(&body("https://desk.example.net/"), None).is_ok());
    }

    /// An icon the sprite has not got draws an empty square in a dock, silently.
    /// Refused where somebody is looking instead.
    #[test]
    fn an_icon_has_to_be_one_this_build_draws() {
        let mut b = body("https://example.com/");
        b.icon = "a-not-a-real-icon".into();
        assert!(validate(&b, None).is_err());
        b.icon = "a-globe".into();
        assert!(validate(&b, None).is_ok());
        // Unset is the default rather than an error, so a client that sends no
        // icon still gets a working tile.
        b.icon = String::new();
        assert_eq!(validate(&b, None).unwrap().icon, DEFAULT_ICON);
    }

    /// Defaults exist so that a client which sends the minimum still produces a
    /// window somebody can use. Zero is what a forgotten field looks like.
    #[test]
    fn a_link_that_answers_the_minimum_still_opens_somewhere_sensible() {
        let l = validate(&body("http://localhost:8096"), None).unwrap();
        assert_eq!(l.open, "frame");
        assert_eq!(l.icon, DEFAULT_ICON);
        assert!(l.width >= 320 && l.height >= 240);
    }

    /// An id is generated and is safe in the two places it is used: a JSON key
    /// and a `data-app` attribute the dock selects on. No hyphens, no quotes,
    /// nothing that needs escaping differently in either.
    #[test]
    fn a_generated_id_is_safe_everywhere_it_is_used() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            let id = fresh_id();
            assert_eq!(id.len(), 6);
            assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
            seen.insert(id);
        }
        // Not a uniqueness guarantee -- it is a birthday check that the
        // generator is not returning a constant, which is the failure that would
        // otherwise show up as "my second link replaced my first".
        assert!(seen.len() > 400, "the id generator is barely random: {} distinct", seen.len());
    }

    /// The path a person's links live at is built from their own home directory
    /// and nothing else. There is no field in any request that reaches it.
    #[test]
    fn the_user_file_is_built_from_the_session_and_never_from_a_request() {
        let ident = |home: &str| auth::Identity {
            username: "alice".into(),
            uid: 1000,
            gid: 1000,
            home: home.into(),
            admin: false,
        };
        assert_eq!(
            user_file(&ident("/home/alice")),
            "/home/alice/.config/webdesk/links.json"
        );
        // A trailing slash on the home directory must not double.
        assert_eq!(
            user_file(&ident("/home/alice/")),
            "/home/alice/.config/webdesk/links.json"
        );
    }

    /// A host with a port and one without are the same host. `[::1]` keeps its
    /// brackets, because both of its forms have colons in them.
    #[test]
    fn a_port_is_not_part_of_the_host() {
        assert_eq!(host_only("desk.example:61443"), "desk.example");
        assert_eq!(host_only("desk.example"), "desk.example");
        assert_eq!(host_only("[::1]:61443"), "[::1]");
        assert_eq!(host_only("[::1]"), "[::1]");
    }
}
