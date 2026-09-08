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
use std::collections::BTreeMap;
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

// ------------------------------------------------------------------- glyphs

/// A pasted icon, reduced to geometry.
///
/// The ten built-in marks above are enough to tell a router from a media server
/// and no more. This is the way past them: copy an icon's SVG from
/// [iconify.design](https://iconify.design) -- 362,000 of them across 238 sets,
/// including `selfh.st`, which is a mark for very nearly every application
/// anybody self-hosts -- and paste it into the form.
///
/// **Nothing is fetched, by this program or by the browser.** No icon set is
/// vendored either. Both alternatives were measured before this was written: the
/// selfh.st set alone is 13.2 MB of JSON, against a 2.9 MB binary, and would
/// still have been a fixed list. Fetching from `api.iconify.design` at paint
/// time would have put a third-party request in front of every dock, which is
/// the same beacon this project already refused for favicons. Pasting has
/// neither cost. The geometry lands in `links.json` beside the name and the URL,
/// and from then on the icon is as local as the rest of the desk -- an
/// air-gapped host draws it exactly as well as a connected one.
///
/// **What is stored is not the markup that was pasted.** That distinction is the
/// whole security argument here and it is worth stating twice. SVG is a document
/// format: it carries `<script>`, `onload=`, `<foreignObject>` full of HTML, and
/// `href="javascript:"`. A stored string that ever reaches `innerHTML` is script
/// execution in the desk's own origin -- the origin whose login form takes a
/// system password and hands back a root-capable shell. So the browser reduces
/// the paste to shapes and numbers before it is ever sent here, this file
/// refuses anything that is not shapes and numbers, and the browser rebuilds the
/// icon with `createElementNS` and `setAttribute` rather than by parsing a string
/// a second time. There is no path from a stored byte to an executed one.
#[derive(Clone, Serialize, Deserialize)]
pub struct Glyph {
    /// The `viewBox` the shapes are drawn on. Every set uses its own grid --
    /// 24 for most, 512 for the app logos -- and the shapes mean nothing without
    /// it.
    pub w: f32,
    pub h: f32,
    pub shapes: Vec<Shape>,
}

/// One drawing primitive: a tag name and the attributes it is allowed to carry.
///
/// A map rather than a variant per tag, because the validation is a table either
/// way and a table is easier to read than seven structs. The names are short
/// because this is written into a JSON file once per link and read on every
/// dock paint.
#[derive(Clone, Serialize, Deserialize)]
pub struct Shape {
    /// The element to create. One of `SHAPES`.
    pub t: String,
    /// Attributes, already reduced to what `attr_ok` permits.
    pub a: BTreeMap<String, String>,
}

/// Caps. A pasted icon is a few hundred bytes; these are three orders of
/// magnitude above anything real, and they exist so that a paste cannot make
/// `links.json` expensive to parse on every dock paint.
const MAX_SHAPES: usize = 96;
const MAX_GLYPH_BYTES: usize = 16 * 1024;

/// The elements that may be drawn, and the geometry each may carry.
///
/// Everything not on this list is dropped rather than refused, because a set
/// that wraps its paths in a `<g>` or ships a `<title>` is not doing anything
/// wrong -- the browser flattens those before sending. What reaches here should
/// already be only this, and the check is what makes that true rather than
/// hoped for.
const SHAPES: &[(&str, &[&str])] = &[
    ("path", &["d", "fill-rule", "clip-rule"]),
    ("circle", &["cx", "cy", "r"]),
    ("ellipse", &["cx", "cy", "rx", "ry"]),
    ("rect", &["x", "y", "width", "height", "rx", "ry"]),
    ("line", &["x1", "y1", "x2", "y2"]),
    ("polyline", &["points"]),
    ("polygon", &["points"]),
];

/// Paint attributes any shape may carry.
///
/// `fill` and `stroke` are here and are the reason this is monochrome: they are
/// forced to `currentColor` or `none` and can be nothing else. That is not only
/// a matter of taste -- an icon takes the colour of the control it sits in, the
/// way every other icon in this desk does -- it also disposes of `fill="url(#g)"`
/// without a special case, since gradients are not among the elements that may
/// be drawn and a reference to one would dangle.
const PAINT: &[&str] = &[
    "fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin",
    "stroke-miterlimit", "stroke-dasharray", "stroke-dashoffset",
    "fill-opacity", "stroke-opacity", "opacity", "transform",
];

fn is_num(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 24
        && v.parse::<f32>().map(|n| n.is_finite()).unwrap_or(false)
}

/// The characters a path `d` is made of, and nothing else.
///
/// This is the attribute the whole feature rests on, and the reason it can rest
/// on it: a `d` is a command alphabet and a set of numbers. It has no syntax for
/// a URL, a script, an entity or an element, so a string that contains only
/// these characters cannot express anything but a shape. That is a stronger
/// statement than "we did not find anything bad in it".
fn is_geometry(v: &str) -> bool {
    !v.is_empty()
        && v.chars().all(|c| {
            c.is_ascii_digit()
                || matches!(c, 'M' | 'm' | 'L' | 'l' | 'H' | 'h' | 'V' | 'v'
                    | 'C' | 'c' | 'S' | 's' | 'Q' | 'q' | 'T' | 't'
                    | 'A' | 'a' | 'Z' | 'z' | 'e' | 'E'
                    | '.' | ',' | '+' | '-' | ' ' | '\t' | '\n' | '\r')
        })
}

/// A list of numbers: `points`, `stroke-dasharray`.
fn is_numlist(v: &str) -> bool {
    !v.is_empty()
        && v.chars().all(|c| {
            c.is_ascii_digit() || matches!(c, '.' | ',' | '+' | '-' | 'e' | 'E' | ' ' | '\t' | '\n' | '\r')
        })
}

/// A transform list, as the named functions and numbers and nothing else.
///
/// Kept because some sets draw a rotated variant by transforming the base shape
/// rather than by emitting different geometry, and dropping it silently would
/// paste an icon that came out the wrong way round. `url(...)` cannot survive
/// this: the only words permitted are the six function names.
fn is_transform(v: &str) -> bool {
    if v.is_empty() || v.len() > 512 {
        return false;
    }
    let mut word = String::new();
    for c in v.chars() {
        if c.is_ascii_alphabetic() {
            word.push(c);
            continue;
        }
        if c == '(' {
            if !matches!(
                word.as_str(),
                "matrix" | "translate" | "scale" | "rotate" | "skewX" | "skewY"
            ) {
                return false;
            }
            word.clear();
            continue;
        }
        if !word.is_empty() {
            return false;
        }
        if !(c.is_ascii_digit()
            || matches!(c, '.' | ',' | '+' | '-' | 'e' | 'E' | ')' | ' ' | '\t' | '\n' | '\r'))
        {
            return false;
        }
    }
    word.is_empty()
}

/// Whether this attribute may appear on this tag, and whether its value is what
/// that attribute is made of.
fn attr_ok(tag: &str, key: &str, value: &str) -> bool {
    if value.len() > 4096 {
        return false;
    }
    let geometry = SHAPES.iter().find(|(t, _)| *t == tag).map(|(_, a)| *a).unwrap_or(&[]);
    let permitted = geometry.contains(&key) || PAINT.contains(&key);
    if !permitted {
        return false;
    }
    match key {
        "d" => is_geometry(value),
        "points" | "stroke-dasharray" => is_numlist(value),
        "transform" => is_transform(value),
        // Forced rather than checked -- see `PAINT`.
        "fill" | "stroke" => value == "currentColor" || value == "none",
        "fill-rule" | "clip-rule" => value == "nonzero" || value == "evenodd",
        "stroke-linecap" => matches!(value, "butt" | "round" | "square"),
        "stroke-linejoin" => matches!(value, "miter" | "round" | "bevel" | "arcs" | "miter-clip"),
        _ => is_num(value),
    }
}

/// Reduce a submitted glyph to what may be stored, or say why it cannot be.
///
/// Refusing rather than sanitising, for one attribute and one tag at a time.
/// Quietly dropping an attribute would draw an icon that is not the one somebody
/// pasted and give them no way to find out why; the browser has already done the
/// reduction, so anything arriving here that this rejects is a client that
/// disagrees with the server about the format, and that is worth a message.
fn validate_glyph(g: &Glyph) -> Result<Glyph, String> {
    if !(g.w.is_finite() && g.h.is_finite()) || g.w <= 0.0 || g.h <= 0.0 || g.w > 8192.0 || g.h > 8192.0 {
        return Err("that icon has no usable viewBox".into());
    }
    if g.shapes.is_empty() {
        return Err("there is nothing to draw in that icon".into());
    }
    if g.shapes.len() > MAX_SHAPES {
        return Err(format!("that icon has more than {MAX_SHAPES} shapes in it"));
    }

    let mut out = Vec::with_capacity(g.shapes.len());
    let mut bytes = 0usize;
    for s in &g.shapes {
        let Some((tag, _)) = SHAPES.iter().find(|(t, _)| *t == s.t) else {
            return Err(format!("{} is not a shape this build draws", s.t));
        };
        let mut a = BTreeMap::new();
        for (k, v) in &s.a {
            if !attr_ok(tag, k, v) {
                return Err(format!("{k} is not something a {tag} may carry here"));
            }
            bytes += k.len() + v.len();
            a.insert(k.clone(), v.clone());
        }
        if a.is_empty() {
            return Err(format!("that {tag} has no geometry in it"));
        }
        out.push(Shape { t: (*tag).to_string(), a });
    }
    if bytes > MAX_GLYPH_BYTES {
        return Err("that icon is larger than this build stores".into());
    }
    Ok(Glyph { w: g.w, h: g.h, shapes: out })
}

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
    /// The built-in mark, and the fallback when there is no glyph. Always set,
    /// so a link drawn by an older build -- or one whose glyph failed to
    /// validate on the way in -- still has something to show.
    pub icon: String,
    /// A pasted icon, if there is one. `None` is the ordinary case and means the
    /// sprite mark above is what gets drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<Glyph>,
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
    /// A pasted icon, already reduced to shapes by the browser. `None` clears
    /// whatever was there and falls back to `icon`, which is how the form's
    /// "use a built-in mark instead" works without a second route.
    #[serde(default)]
    glyph: Option<Glyph>,
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

    // The pasted icon, if there is one. `icon` above is still required and still
    // checked: it is what gets drawn if this link is ever read by a build that
    // does not know about glyphs, and what the form falls back to when somebody
    // clears the paste.
    let glyph = match &body.glyph {
        Some(g) => Some(validate_glyph(g)?),
        None => None,
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
        glyph,
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
        "glyph": l.glyph,
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
            glyph: None,
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

    // ------------------------------------------------------------- glyphs

    fn shape(t: &str, pairs: &[(&str, &str)]) -> Shape {
        Shape {
            t: t.into(),
            a: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn glyph(shapes: Vec<Shape>) -> Glyph {
        Glyph { w: 24.0, h: 24.0, shapes }
    }

    fn refuse(g: &Glyph) -> String {
        match validate_glyph(g) {
            Ok(_) => panic!("a glyph that should have been refused was accepted"),
            Err(e) => e,
        }
    }

    /// The ordinary case: a real icon, copied from a real set, survives.
    ///
    /// Both paint models, because half of Iconify is filled shapes and the other
    /// half is stroked ones -- and an icon that lost its stroke attributes would
    /// come out as a solid blob rather than as a drawing.
    #[test]
    fn a_real_icon_of_either_paint_model_is_kept() {
        // simple-icons:jellyfin, filled.
        let filled = glyph(vec![shape(
            "path",
            &[("fill", "currentColor"), ("d", "M12 .002C8.826.002-1.398 18.537.16 21.666z")],
        )]);
        assert_eq!(validate_glyph(&filled).unwrap().shapes.len(), 1);

        // lucide:house, stroked -- every one of these attributes has to survive.
        let stroked = glyph(vec![shape(
            "path",
            &[
                ("fill", "none"),
                ("stroke", "currentColor"),
                ("stroke-width", "2"),
                ("stroke-linecap", "round"),
                ("stroke-linejoin", "round"),
                ("d", "M3 10.5 12 3l9 7.5V21H3z"),
            ],
        )]);
        let out = validate_glyph(&stroked).unwrap();
        assert_eq!(out.shapes[0].a.get("stroke-width").map(String::as_str), Some("2"));
        assert_eq!(out.shapes[0].a.get("fill").map(String::as_str), Some("none"));
    }

    /// Every shape this build draws, with its own geometry, is accepted -- and
    /// geometry belonging to a different shape is not. A `cx` on a `rect` is not
    /// dangerous, it is a client that has misunderstood the format, and saying so
    /// is better than storing an attribute that will never be read.
    #[test]
    fn each_shape_carries_only_its_own_geometry() {
        assert!(validate_glyph(&glyph(vec![shape("circle", &[("cx", "12"), ("cy", "12"), ("r", "9")])])).is_ok());
        assert!(validate_glyph(&glyph(vec![shape("rect", &[("x", "3"), ("y", "3"), ("width", "18"), ("height", "18"), ("rx", "2")])])).is_ok());
        assert!(validate_glyph(&glyph(vec![shape("line", &[("x1", "0"), ("y1", "0"), ("x2", "24"), ("y2", "24")])])).is_ok());
        assert!(validate_glyph(&glyph(vec![shape("polygon", &[("points", "12,2 22,20 2,20")])])).is_ok());
        assert!(validate_glyph(&glyph(vec![shape("ellipse", &[("cx", "12"), ("cy", "12"), ("rx", "9"), ("ry", "5")])])).is_ok());

        let e = refuse(&glyph(vec![shape("rect", &[("cx", "12")])]));
        assert!(e.contains("rect"), "{e}");
    }

    /// **The test this whole feature rests on.** Every one of these is a way to
    /// get script execution out of an SVG, and every one of them is a string that
    /// would be stored and later put back into a document if this validator let
    /// it through. The desk's own origin is the one whose login form takes a
    /// system password, so this is not a theoretical severity.
    ///
    /// Note what is being asserted: not that these particular strings are caught,
    /// but that the *shape* of the format has no room for them. A `d` is an
    /// alphabet of path commands and numbers; a tag is one of seven names.
    #[test]
    fn nothing_that_could_execute_survives() {
        // Elements that are not shapes, whatever they carry.
        for tag in ["script", "foreignObject", "image", "use", "a", "animate", "set",
                    "style", "iframe", "linearGradient", "svg", "g"] {
            let e = refuse(&glyph(vec![shape(tag, &[("d", "M0 0")])]));
            assert!(e.contains(tag), "{tag}: {e}");
        }

        // Attributes that are not geometry or paint, on a tag that is.
        for (k, v) in [
            ("onload", "alert(1)"),
            ("onclick", "alert(1)"),
            ("href", "javascript:alert(1)"),
            ("xlink:href", "javascript:alert(1)"),
            ("style", "background:url(javascript:alert(1))"),
            ("id", "x"),
            ("class", "y"),
            ("requiredExtensions", "z"),
        ] {
            let e = refuse(&glyph(vec![shape("path", &[("d", "M0 0"), (k, v)])]));
            assert!(e.contains(k), "{k}: {e}");
        }

        // A `d` that is not a path. There is no syntax in the command alphabet
        // for any of this, which is the point -- it is refused for containing
        // characters a path cannot contain, not for matching a pattern somebody
        // thought of.
        for d in [
            "M0 0\"/><script>alert(1)</script><path d=\"M0 0",
            "url(#x)",
            "javascript:alert(1)",
            "M0 0 L1 1 <!--",
            "M0 0&#59;",
        ] {
            assert!(refuse(&glyph(vec![shape("path", &[("d", d)])])).contains('d'));
        }
    }

    /// Monochrome is enforced here and not only in the browser. A paint value
    /// that is anything but `currentColor` or `none` is refused, which disposes
    /// of `url(#gradient)` without a rule of its own -- there are no gradients to
    /// point at, so a reference to one could only dangle.
    #[test]
    fn paint_is_currentcolor_or_nothing() {
        for bad in ["#ff0000", "red", "url(#grad)", "rgb(1,2,3)", "inherit"] {
            let e = refuse(&glyph(vec![shape("path", &[("d", "M0 0"), ("fill", bad)])]));
            assert!(e.contains("fill"), "{bad}: {e}");
        }
        assert!(validate_glyph(&glyph(vec![shape("path", &[("d", "M0 0"), ("fill", "currentColor")])])).is_ok());
        assert!(validate_glyph(&glyph(vec![shape("path", &[("d", "M0 0"), ("fill", "none")])])).is_ok());
    }

    /// `transform` is kept because some sets draw a rotated variant by
    /// transforming the base shape, and dropping it would paste an icon that
    /// came out the wrong way round. It is the one attribute here whose value has
    /// words in it, so the words are an allow-list of six.
    #[test]
    fn a_transform_may_only_name_the_six_functions() {
        for ok in ["rotate(45 12 12)", "translate(2,3) scale(1.5)", "matrix(1 0 0 1 0 0)", "skewX(10)"] {
            assert!(
                validate_glyph(&glyph(vec![shape("path", &[("d", "M0 0"), ("transform", ok)])])).is_ok(),
                "{ok} was refused"
            );
        }
        for bad in ["url(#x)", "attr(href)", "rotate(45) url(#y)", "expression(alert(1))", "translate(1);x"] {
            let e = refuse(&glyph(vec![shape("path", &[("d", "M0 0"), ("transform", bad)])]));
            assert!(e.contains("transform"), "{bad}: {e}");
        }
    }

    /// A viewBox is what the geometry means. Without a usable one the shapes are
    /// numbers on no grid, and the icon draws at whatever size the browser
    /// guesses -- which is how a paste ends up as a full-window smear.
    #[test]
    fn a_glyph_without_a_usable_grid_is_refused() {
        for (w, h) in [(0.0, 24.0), (24.0, 0.0), (-24.0, 24.0), (f32::NAN, 24.0),
                       (f32::INFINITY, 24.0), (99999.0, 24.0)] {
            let g = Glyph { w, h, shapes: vec![shape("path", &[("d", "M0 0")])] };
            assert!(refuse(&g).contains("viewBox"), "{w}x{h} was accepted");
        }
        assert!(validate_glyph(&Glyph { w: 512.0, h: 512.0, shapes: vec![shape("path", &[("d", "M0 0")])] }).is_ok());
    }

    /// An icon with nothing in it, and one with far too much. Neither is an
    /// attack; both are a `links.json` that costs something to parse on every
    /// dock paint for no drawing in return.
    #[test]
    fn an_empty_or_enormous_glyph_is_refused() {
        assert!(refuse(&glyph(vec![])).contains("nothing to draw"));
        let many: Vec<Shape> = (0..MAX_SHAPES + 1).map(|_| shape("path", &[("d", "M0 0")])).collect();
        assert!(refuse(&glyph(many)).contains("shapes"));
        // A shape with no attributes at all draws nothing and is a client bug.
        assert!(refuse(&glyph(vec![shape("path", &[])])).contains("geometry"));
        // And one enormous path rather than many small ones.
        let huge = "M0 0 ".repeat(4000);
        assert!(refuse(&glyph(vec![shape("path", &[("d", &huge)])])).contains('d'));
    }

    /// A link keeps its built-in mark even when it has a pasted one, so a build
    /// that has never heard of glyphs still draws something -- and so clearing
    /// the paste has somewhere to fall back to without a second request.
    #[test]
    fn a_pasted_icon_never_replaces_the_fallback_mark() {
        let mut b = body("https://example.com/");
        b.glyph = Some(glyph(vec![shape("path", &[("d", "M0 0"), ("fill", "currentColor")])]));
        let l = validate(&b, None).unwrap();
        assert_eq!(l.icon, DEFAULT_ICON, "the sprite mark must survive a paste");
        assert!(l.glyph.is_some());

        // And a refused glyph fails the whole link rather than being dropped
        // quietly, which would store a link drawn with an icon nobody chose.
        b.glyph = Some(glyph(vec![shape("script", &[("d", "M0 0")])]));
        assert!(validate(&b, None).is_err());
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
