//! Installing, running and removing the applications in the catalog.
//!
//! **One manager, one book.** Every entry is a Flatpak installed on this host
//! and run under a headless compositor in the session of whoever opened it, with
//! its pixels carried to a canvas by `rfb.rs`. `flatpak.rs` puts it on the
//! machine, `systemd.rs` starts it in a user manager, and the record written at
//! install time is a line in `apps.json` saying which application id is now on
//! disk and whether WebDesk is the one that put it there.
//!
//! This file used to hold three managers and most of its length was the
//! difference between them. A container had to be created, given a port, given a
//! `/config` mount, given `PUID`/`PGID`, and served under a path prefix; a host
//! service had to be adopted, its unit written and its port taken on trust. Both
//! were reached through a reverse proxy on this origin. All of that is gone,
//! along with the questions that only existed to configure it -- a form to fill
//! in, a bind-mount validator, a port allocator, an image tag allow-list.
//!
//! **What is left is described by what it has not got.** No image, no port, no
//! prefix, no container, no state directory: the application keeps its files
//! where Flatpak already puts them, in the home directory of whoever ran it. An
//! install record therefore carries almost nothing, and that is the truth about
//! it rather than an omission -- see `Installed`.
//!
//! **Installed once, host-wide. Run per user.** Installing is `flatpak install
//! --system`: one copy on disk, part of the machine like a package, and gated on
//! the administrative group. *Running* it is a systemd user unit in the session
//! of whoever opened it, because an application whose subject is your home
//! directory is worth nothing pointed at somebody else's. That split is not an
//! implementation detail -- it is the line between who may decide what the
//! machine contains and who may use what it contains.
//!
//! **Who may do this.** Install and remove change what is on the host, so they
//! are checked against the same administrative group as the self-updater.
//! Everyone signed in may *list* the installed apps, and open and close them --
//! an installed app is part of the host, like a package, not a possession of
//! whoever installed it. Opening one runs a program as *you*, under your own uid,
//! in your own systemd manager, with no privilege anywhere in it. Having an
//! account on this machine already means being able to do that, and a check in
//! front of it would be WebDesk claiming to hand out something the host had
//! handed out already.
//!
//! This is the second place in the program where authorisation lives in code
//! rather than in the kernel. The first is `update.rs`, which explains why that
//! is a cost worth naming out loud.
//!
//! **What a user may choose: which entry, and nothing else.** There is no form.
//! Every question a container entry used to ask had one obviously right answer
//! for an application running on this host as this user -- the clock is the
//! host's, the identity is the caller's, the files are already theirs -- so
//! installing is a single press and there is no request field that could
//! describe what gets run. A request selects one of the operations the build
//! already contains; it never says what the operation is.

use crate::catalog;
use crate::{auth, session_of, unauthorized, AppState};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_STATE_DIR: &str = "/var/lib/webdesk";

const LOG_TAIL: usize = 64 * 1024;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

pub(crate) fn state_dir() -> PathBuf {
    PathBuf::from(env_or("WD_STATE_DIR", DEFAULT_STATE_DIR))
}

fn apps_file() -> PathBuf {
    state_dir().join("apps.json")
}

pub(crate) fn status_file() -> PathBuf {
    state_dir().join("apps.status")
}

pub(crate) fn log_file() -> PathBuf {
    state_dir().join("apps.log")
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn bad(status: StatusCode, msg: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": msg.to_string() }))).into_response()
}

// ------------------------------------------------------------------- state

/// One installed application, as recorded on disk.
///
/// Two fields carry anything about the application itself, and the shortness is
/// the point. There is no image, because nothing was pulled. There is no port,
/// because nothing listens on one. There is no environment, because the process
/// inherits the session's. There is no mount, because the application is already
/// on the filesystem it would have been shown. There is no device, because it is
/// already on the host where the devices are. A container record filled every one
/// of those in, and every one of them was a fact about a container rather than
/// about the application.
///
/// What is left is `flatpak`, which is the only part of this app that exists on
/// disk, and `adopted`, which decides what removal is allowed to do about it.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Installed {
    pub slug: String,
    /// The Flatpak application id installed on this host.
    ///
    /// The id rather than a bare flag because the id is the only handle onto
    /// the thing: there is no container name and no unit of our own to point at,
    /// there is a few hundred megabytes under `/var/lib/flatpak` with this name
    /// on it, and that is what `remove` has to be able to name.
    pub flatpak: String,
    /// Whether that Flatpak was already on this host before the entry was
    /// installed.
    ///
    /// `remove` reads this to decide whether it may take the application away
    /// again: WebDesk uninstalls what it put here and leaves alone what it merely
    /// found. An application somebody installed for their own reasons, which
    /// WebDesk then offered a window onto, does not stop being theirs because a
    /// tile came out of a dock.
    ///
    /// Settled at install time and never afterwards, because afterwards the two
    /// cases are indistinguishable -- `flatpak::provide` returns the same `Ok`
    /// for an application it downloaded and one it found already there.
    #[serde(default)]
    pub adopted: bool,
    #[serde(default)]
    pub installed: u64,
    #[serde(default)]
    pub actor: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Book {
    #[serde(default)]
    apps: BTreeMap<String, Installed>,
}

fn read_book() -> Book {
    match std::fs::read_to_string(apps_file()) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_default(),
        Err(_) => Book::default(),
    }
}

/// Replace the book in one step, so a concurrent read never sees half of it.
fn write_book(b: &Book) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("apps.json.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(b)?)?;
    std::fs::rename(tmp, apps_file())
}

pub(crate) fn read_status() -> Value {
    match std::fs::read_to_string(status_file()) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_else(|_| json!({"state": "idle"})),
        Err(_) => json!({"state": "idle"}),
    }
}

pub(crate) fn write_status(v: &Value) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join("apps.status.new");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(tmp, status_file())
}

fn log_tail() -> String {
    let Ok(bytes) = std::fs::read(log_file()) else { return String::new() };
    let start = bytes.len().saturating_sub(LOG_TAIL);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

// ----------------------------------------------------------- authorisation

pub(crate) fn admin_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Arc<crate::Session>, Response> {
    let Some(session) = session_of(state, headers) else { return Err(unauthorized()) };
    if !session.ident.admin {
        tracing::warn!(
            user = %session.ident.username,
            "denied an app action: not in {:?}",
            auth::admin_groups()
        );
        return Err(bad(
            StatusCode::FORBIDDEN,
            format!(
                "installing apps requires membership of {}",
                auth::admin_groups().join(" or ")
            ),
        ));
    }
    Ok(session)
}

// ------------------------------------------------------------- the session

/// The two names one person's copy of one app is known by: the socket `wayvnc`
/// will listen on, and the user unit that starts the pair of them.
///
/// **The uid is a parameter of the identity and of nothing else.** There is no
/// uid in `SlugReq`, no uid in any body this file parses, and no argument here a
/// request could reach -- the caller passes the `Identity` its session cookie
/// resolved to, and the socket path falls out of that. This matters more than its
/// two lines suggest: the socket is the whole of somebody's screen and keyboard,
/// and a uid taken from a request would be a way to ask for somebody else's.
///
/// Paired here so that neither is ever spelled out by hand. The unit name comes
/// from `systemd::app_unit`, which is the one place a slug becomes a unit, and
/// the socket comes from `rfb::socket_path`, which is the one place a slug and a
/// uid become a path. Two spellings of either is how an open and a close end up
/// naming different units, and Close silently does nothing.
///
/// Neither is created here. `systemd::install_app_template` makes the directory,
/// with the mode and the owner it needs, at the same time as it enables lingering
/// and writes the template -- see `open` for why that is one call on every open
/// rather than three pieces of setup done once.
fn user_session(ident: &auth::Identity, slug: &str) -> (PathBuf, String) {
    (crate::rfb::socket_path(ident.uid, slug), crate::systemd::app_unit(slug))
}

/// The record `open` was asked for, or the reason it cannot be started.
///
/// `open` only. `close` checks that the record exists and stops there, for the
/// reason given at its own call site.
///
/// Two refusals, and they are different facts about different things. A slug with
/// no record is simply not installed -- a mistyped name, or an app somebody else
/// removed while this browser tab was sitting open.
///
/// A record whose catalog entry has gone is a rarer and more confusing thing: it
/// is installed, the dock still paints it, and it cannot be started. `session.rs`
/// resolves the slug back into an application id against the catalog compiled
/// into this binary and refuses anything that is not there, so starting the unit
/// would answer `ok`, hand the browser a WebSocket, and leave it waiting on a
/// compositor that exited immediately. Better to say it here, where there is
/// somebody reading.
fn openable(book: &Book, slug: &str) -> Result<Installed, String> {
    let Some(record) = book.apps.get(slug) else {
        return Err(format!("{slug} is not installed"));
    };
    if catalog::find(slug).is_none() {
        return Err(format!(
            "{slug} is installed but is no longer in this build's catalog, so there is \
             nothing here that knows how to start it"
        ));
    }
    Ok(record.clone())
}

/// The refusal owed to somebody whose host is missing what a streamed app needs.
///
/// A refusal that carries what it would take to succeed rather than a prompt, so
/// a client that ignores the extra field still gets an ordinary error with an
/// ordinary explanation, and declining is simply not making the second request.
///
/// It names the dependency *keys*, because a compositor and an RFB server are
/// host-wide, they unlock every entry at once, and `deps.rs` has an endpoint of
/// its own that installs them and streams into the same log. The browser goes
/// there and comes back to press Install again.
fn missing_deps(app: &catalog::App, absent: &[&'static crate::deps::Dep]) -> Value {
    let labels: Vec<&str> = absent.iter().map(|d| d.label).collect();
    let list = labels.join(", ");
    json!({
        "error": format!(
            "{} is drawn on this host, and this host has not got {list}.",
            app.name
        ),
        "offer": {
            // Keys rather than labels: this is the argument `/api/deps/install`
            // takes, and a refusal the browser has to translate before it can
            // act on it is one more place to get it wrong.
            "deps": absent.iter().map(|d| json!({
                "key": d.key,
                "label": d.label,
                "why": d.why,
            })).collect::<Vec<_>>(),
            "detail": format!(
                "WebDesk can install {list}, and then {} will install. They are needed once \
                 for the whole host rather than per application: every app that is drawn \
                 here uses the same compositor.",
                app.name
            ),
        },
    })
}

/// Take a system-wide Flatpak off this host.
///
/// `--system` because that is how it was put on -- a `--user` uninstall would
/// report success having removed nothing, since there is nothing of this app in
/// the calling user's installation to remove. `--noninteractive` because there is
/// nobody at a terminal to answer the prompt about related runtimes, and a removal
/// that blocked forever on an unread question would look exactly like one that
/// hung.
fn uninstall_flatpak(id: &str) -> Result<(), String> {
    let out = std::process::Command::new("flatpak")
        .args(["uninstall", "-y", "--system", "--noninteractive", id])
        .output()
        .map_err(|e| format!("could not run flatpak: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() { format!("flatpak uninstall {id} failed") } else { err })
}

// ---------------------------------------------------------------- handlers

/// What may be installed. Any session may read it; the UI uses `allowed` to
/// decide whether to offer the button, and every route re-checks regardless.
///
/// **`allowed` is about the person, not about the host.** Whether this machine
/// has what an app needs is a separate question with a separate answer, at
/// `/api/deps`, and folding the two together would hide the Install button on a
/// host that is one package away from being able to use it. An install that
/// cannot work is still refused with a sentence saying why -- by `deps::absent_for`
/// -- so this flag decides what is offered and never what is permitted.
pub async fn catalog_list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };

    let mut body = catalog::as_json();
    body["allowed"] = json!(session.ident.admin);
    body["admin"] = json!(session.ident.admin);
    body["admin_groups"] = json!(auth::admin_groups());
    Json(body).into_response()
}

/// The installed apps, each with its live state folded in. Any session may read
/// this -- it is what paints the dock.
///
/// **The list is host-wide; the states beside it are not.** Which apps are
/// installed is a fact about the machine and is the same for everybody. Whether
/// one is *running* is a fact about one person's session, so this reports the
/// caller's own -- which means two people looking at the same dock at the same
/// moment can honestly see different things.
///
/// The alternative -- reporting whether *anybody* has it open -- would be wrong
/// twice over. It would offer somebody a Close button for a window they have not
/// got, and it would quietly tell every account on the machine who is using what,
/// which is a thing a dock has no business saying.
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };
    let book = read_book();

    let apps: Vec<Value> = book
        .apps
        .values()
        .map(|a| {
            let entry = catalog::find(&a.slug);
            json!({
                "slug": a.slug,
                "name": entry.map(|c| c.name).unwrap_or(&a.slug),
                "tagline": entry.map(|c| c.tagline).unwrap_or(""),
                // `a-box` is what the dock draws when an installed app's catalog
                // entry has gone away in a later build, which is the only case
                // left that needs a fallback icon.
                "icon": entry.map(|c| c.icon).unwrap_or("a-box"),
                "notes": entry.map(|c| c.notes).unwrap_or(""),
                "flatpak": a.flatpak,
                "state": crate::systemd::user_state(
                    &session.ident.username,
                    &crate::systemd::app_unit(&a.slug),
                ),
                // The same object `/api/apps/catalog` sends, repeated here
                // because the dock is painted from this list and never reads the
                // catalog. Without it an app launched from the dock has no
                // geometry to open at, and since the window's size *is* the
                // resolution the browser asks the compositor for, the entry's
                // choice would be silently replaced by the desk's default --
                // 1600x1000 arriving as 1000x625 with nothing to say why.
                "streamed": entry.map(|c| json!({
                    "flatpak": c.streamed.flatpak.id,
                    "width": c.streamed.width,
                    "height": c.streamed.height,
                })),
                "ws": format!("/ws/rfb/{}", a.slug),
                "installed": a.installed,
                "actor": a.actor,
            })
        })
        .collect();

    Json(json!({ "apps": apps, "admin": session.ident.admin })).into_response()
}

/// How the install that is running is getting on, and what it has printed.
///
/// Host-wide and admin-gated, unlike `list`, and the two are not inconsistent. An
/// install changes what the machine contains, so there is exactly one of them at
/// a time and it is the same one for everybody watching. It is only *running*
/// that is per person, and running is not reported here.
pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(r) = admin_session(&state, &headers) {
        return r;
    }
    Json(json!({ "status": read_status(), "log": log_tail() })).into_response()
}

#[derive(Deserialize)]
pub struct InstallReq {
    slug: String,
}

/// Put one catalog entry's Flatpak on this host, for every account on it.
///
/// Returns as soon as the download starts. A few hundred megabytes off Flathub is
/// the long part, so the browser polls `/api/apps/status` -- the same shape the
/// self-updater uses.
pub async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InstallReq>,
) -> Response {
    let session = match admin_session(&state, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };

    let Some(app) = catalog::find(&req.slug) else {
        return bad(StatusCode::NOT_FOUND, format!("{} is not in the catalog", req.slug));
    };

    let actor = session.ident.username.clone();
    if read_book().apps.contains_key(app.slug) {
        return bad(StatusCode::CONFLICT, format!("{} is already installed", app.name));
    }
    if read_status()["state"] == "running" {
        return bad(StatusCode::CONFLICT, "another install is already running");
    }

    let absent = crate::deps::absent_for(crate::deps::Need::Streamed);
    if !absent.is_empty() {
        return (StatusCode::CONFLICT, Json(missing_deps(app, &absent))).into_response();
    }

    // `'static` because the install runs on a blocking task that outlives this
    // call, and the entry it is installing has to still be there when it gets to
    // `provide`.
    let fp: &'static catalog::Flatpak = &app.streamed.flatpak;
    // Whether the host already had it, settled *before* installing, because
    // afterwards there is no way to tell -- `provide` returns the same `Ok` for
    // an application it downloaded and one it found.
    let adopted = crate::flatpak::installed(fp.id);
    let id = fp.id.to_string();
    let name = app.name.to_string();
    let slug = app.slug.to_string();

    let _ = write_status(&json!({
        "state": "running",
        "phase": "downloading",
        "slug": app.slug,
        "name": app.name,
        "started": now(),
        "actor": actor,
    }));
    let _ = std::fs::write(log_file(), b"");

    tracing::warn!(user = %actor, slug = %app.slug, id = %fp.id, "installing an app");

    tokio::task::spawn_blocking(move || {
        let log = log_file();
        if let Err(e) = crate::flatpak::provide(fp, &log) {
            let _ = write_status(&json!({
                "state": "failed", "phase": "downloading", "slug": slug, "name": name,
                "finished": now(), "actor": actor, "error": e,
            }));
            return;
        }

        let record = Installed {
            slug: slug.clone(),
            flatpak: id,
            adopted,
            installed: now(),
            actor: actor.clone(),
        };
        let mut book = read_book();
        book.apps.insert(slug.clone(), record);
        if let Err(e) = write_book(&book) {
            let _ = write_status(&json!({
                "state": "failed", "phase": "recording", "slug": slug, "name": name,
                "finished": now(), "actor": actor,
                "error": format!("the application is installed but could not be recorded: {e}"),
            }));
            return;
        }

        let _ = write_status(&json!({
            "state": "done", "phase": "installed", "slug": slug, "name": name,
            "finished": now(), "actor": actor,
        }));
    });

    Json(json!({ "ok": true, "started": true, "slug": app.slug })).into_response()
}

#[derive(Deserialize)]
pub struct SlugReq {
    slug: String,
    /// Only read by `remove`: consent to take the application off this host for
    /// everybody.
    ///
    /// A bare `true` rather than a list -- what would be uninstalled is decided
    /// by the record, and letting the answer name an application id would make
    /// this a way to uninstall anything on the machine. The browser is agreeing
    /// to a sentence WebDesk wrote.
    #[serde(default)]
    accept_uninstall: bool,
}

/// Take an app out of the dock, and off the machine if it was ours to remove.
///
/// **Who put the thing there decides what removal means.** If WebDesk installed
/// the Flatpak, removal uninstalls it: a `--system` Flatpak that is in nobody's
/// dock is a few hundred megabytes that nothing on this machine will ever start
/// again, and leaving it would mean the only way to get the disk back is a shell
/// -- which is precisely what an administrator opened the Apps window to avoid.
/// If the host already had it, removal leaves it exactly where it was. WebDesk
/// offered a window onto somebody's application and is now withdrawing the
/// window, not the application.
///
/// **And it will not quietly interrupt anybody.** The Flatpak is host-wide, so
/// uninstalling it takes the application away from every account at once, and
/// some of those accounts may have it open with unsaved work in it this second.
///
/// That is not a theoretical worry, and it reaches further than the browser. The
/// unit's `ExecStop` kills the Flatpak by application id rather than by instance,
/// so stopping somebody's session also closes the same application if they have
/// it open on the machine's own screen -- the session they were sitting in front
/// of, not the one in a tab. `systemd.rs` chose that deliberately, and the effect
/// here is to make removal unambiguously destructive to work in progress rather
/// than arguably so.
///
/// WebDesk cannot see whose work -- a user unit lives in a manager this process
/// only reaches one user at a time -- so it does not pretend to, and the honest
/// answer to a question you cannot answer is to ask. The first request refuses
/// and says plainly what removing will do; the browser comes back with
/// `accept_uninstall` or does not come back.
///
/// The refusal comes before anything is stopped or deleted, so a declined removal
/// leaves the app installed, the record intact and the caller's own session still
/// running.
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SlugReq>,
) -> Response {
    let session = match admin_session(&state, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let mut book = read_book();
    let Some(record) = book.apps.get(&req.slug).cloned() else {
        return bad(StatusCode::NOT_FOUND, format!("{} is not installed", req.slug));
    };
    let name = catalog::find(&req.slug).map(|a| a.name).unwrap_or(&req.slug).to_string();
    let id = record.flatpak.clone();

    // Asked before anything moves, so that declining costs nothing: no session
    // stopped, no record forgotten, nothing to put back.
    if !record.adopted && !req.accept_uninstall {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!(
                    "{name} is installed once for this whole host, so removing it \
                     uninstalls it for everyone."
                ),
                "offer": {
                    "uninstall": id,
                    "detail": format!(
                        "WebDesk installed {id} on this host and will uninstall it. \
                         Anyone who has {name} open right now will have it stop -- \
                         here or at the machine's own screen -- and anything unsaved \
                         in it will be lost. Each person's own files stay where they \
                         are, in their home directory.",
                    ),
                },
            })),
        )
            .into_response();
    }

    // This administrator's own session, and only ever theirs: the user comes out
    // of the session cookie, never out of the request. Stopped first because it
    // is the one window this removal is certain to be closing, and leaving it
    // drawing an application that is about to be uninstalled underneath it is
    // worse than closing it.
    let (_, unit) = user_session(&session.ident, &req.slug);
    let user = session.ident.username.clone();
    let u = user.clone();
    let stopped =
        tokio::task::spawn_blocking(move || crate::systemd::user_act("stop", &u, &unit)).await;
    if let Ok(Err(e)) = stopped {
        tracing::warn!(slug = %req.slug, user = %user, "could not stop the session: {e}");
    }

    let mut uninstalled = false;
    let mut note: Option<String> = None;

    if record.adopted {
        // Found here, so left here. Said out loud rather than left to be
        // noticed, because "the app vanished from my dock and the disk did not
        // come back" is otherwise an unexplained fact about the machine.
        note = Some(format!(
            "{id} was already installed on this host before {name} was added here, \
             so it has been left installed."
        ));
        tracing::warn!(slug = %req.slug, id = %id, "forgetting a Flatpak WebDesk did not install");
    } else {
        let app = id.clone();
        let done = tokio::task::spawn_blocking(move || uninstall_flatpak(&app)).await;
        match done {
            Ok(Ok(())) => uninstalled = true,
            // Reported, not fatal: refusing to forget it would leave an app that
            // can never be removed from the dock. The sentence goes back to the
            // browser as well as into the log, since this is the half somebody
            // consented to.
            Ok(Err(e)) => {
                tracing::warn!(slug = %req.slug, id = %id, "could not uninstall: {e}");
                note = Some(format!("{id} could not be uninstalled: {e}"));
            }
            Err(e) => note = Some(format!("{id} could not be uninstalled: {e}")),
        }
    }

    book.apps.remove(&req.slug);
    if let Err(e) = write_book(&book) {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, format!("could not update the app list: {e}"));
    }

    tracing::warn!(user = %session.ident.username, slug = %req.slug, "removed an app");
    Json(json!({ "ok": true, "uninstalled": uninstalled, "note": note })).into_response()
}

/// `POST /api/apps/open` -- start this user's session for an app.
///
/// The one call the desk makes when a dock icon is clicked. It starts the
/// caller's *own* session -- their compositor, their Flatpak, their socket -- and
/// answers with the WebSocket to point a canvas at.
///
/// **Open to anyone signed in, unlike install.** Opening runs a program as *you*:
/// your uid, your home directory, your files, in your own systemd manager, with
/// no privilege anywhere in it. Having an account on this machine already means
/// being able to do that.
///
/// **Twice is the same as once.** `start` on a unit systemd already has active is
/// a no-op, and that is why it is `start` here and not `restart`: somebody
/// clicking a dock icon for an app they have open in another tab must land back
/// in the session they left, not in a fresh compositor with their work gone.
pub async fn open(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SlugReq>,
) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };
    let record = match openable(&read_book(), &req.slug) {
        Ok(r) => r,
        Err(e) => return bad(StatusCode::NOT_FOUND, e),
    };

    let (socket, unit) = user_session(&session.ident, &req.slug);

    tracing::info!(
        user = %session.ident.username, slug = %req.slug, id = %record.flatpak,
        socket = %socket.display(), "opening an app"
    );

    let (user, uid) = (session.ident.username.clone(), session.ident.uid);
    let started = tokio::task::spawn_blocking(move || {
        // On every open, not once at install. Two of the three things this does
        // are not setup that survives: `/run/webdesk/rfb/<uid>` is under `/run`
        // and is gone after a reboot, and lingering has to be on for there to be
        // a user bus to reach at all. Only the template itself is durable, and
        // writing it is a no-op when the file already says what it should. Doing
        // this at install time would also be doing it in the wrong person's
        // session: the install was somebody else's, possibly before this account
        // existed.
        crate::systemd::install_app_template(uid, &user)?;
        crate::systemd::user_act("start", &user, &unit)
    })
    .await;

    match started {
        Ok(Ok(())) => Json(json!({
            "ok": true,
            "ws": format!("/ws/rfb/{}", req.slug),
        }))
        .into_response(),
        Ok(Err(e)) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// `POST /api/apps/close` -- stop this user's session for an app.
///
/// Closing the window does not call this; quitting does. A streamed app behaves
/// like an application on a desktop, where closing the last window and quitting
/// are different acts and the second one is the one that loses your unsaved work.
///
/// It reaches slightly further than the tab it was pressed in. The unit's
/// `ExecStop` kills the Flatpak by application id, so quitting here also quits the
/// same application if you have it open at the machine's own screen. That is
/// `systemd.rs`'s decision; what it means for this handler is that Close is a quit
/// of the *application* for this user, not of one window of it, and the UI should
/// ask before sending it.
///
/// **Only ever your own.** The unit is named from the slug and the manager is
/// named from the session cookie's identity, so there is no argument here that
/// could point this at somebody else's session -- not a uid, not a username, not a
/// unit name. That is worth being explicit about because this is the one call in
/// the file that destroys work rather than state: an administrator removing the
/// whole application is refused until they say they mean it, and nobody,
/// administrator or not, can reach across into another account and stop what is
/// running in it.
pub async fn close(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SlugReq>,
) -> Response {
    let Some(session) = session_of(&state, &headers) else { return unauthorized() };
    // The record has to exist, and that is the whole check -- deliberately less
    // than `open` makes. `open` also refuses a slug whose catalog entry has gone
    // in a later build, because starting it would hand the browser a WebSocket
    // and a compositor that exits at once. Closing has the opposite asymmetry: if
    // a session from an earlier build is still running, stopping it is exactly
    // what somebody wants, and the unit name comes from the slug rather than from
    // the catalog. Refusing here would leave a compositor nothing could reach.
    if !read_book().apps.contains_key(&req.slug) {
        return bad(StatusCode::NOT_FOUND, format!("{} is not installed", req.slug));
    }

    let (_, unit) = user_session(&session.ident, &req.slug);
    let user = session.ident.username.clone();
    tracing::info!(user = %user, slug = %req.slug, "closing an app");
    let u = user.clone();
    let stopped =
        tokio::task::spawn_blocking(move || crate::systemd::user_act("stop", &u, &unit)).await;

    match stopped {
        Ok(Ok(())) => Json(json!({ "ok": true })).into_response(),
        Ok(Err(e)) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// Tell a running session how big its output should be.
///
/// Written to the session's control socket rather than sent down the RFB stream,
/// because the stream cannot carry it: `wayvnc` through 0.7.2 -- Debian, Ubuntu
/// and EPEL 9 -- never registers a handler for a client's `SetDesktopSize`, so the
/// request noVNC makes is received and dropped with nothing logged anywhere. See
/// `rfb::control_path`.
///
/// Best effort by design. A session that has not finished starting has no socket
/// yet, and the size it was started with is already the right one, so there is
/// nothing here worth failing a request over.
fn tell_session_size(uid: u32, slug: &str, w: u32, h: u32, scale: f32) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Write};
    let path = crate::rfb::control_path(uid, slug);
    let mut sock = std::os::unix::net::UnixStream::connect(&path)
        .map_err(|e| format!("this app is not listening for a size yet: {e}"))?;
    let timeout = Some(std::time::Duration::from_secs(5));
    let _ = sock.set_read_timeout(timeout);
    let _ = sock.set_write_timeout(timeout);
    writeln!(sock, "{w}x{h}@{scale}")
        .map_err(|e| format!("could not ask for {w}x{h} at {scale}x: {e}"))?;
    let mut reply = String::new();
    BufReader::new(&sock).read_line(&mut reply).map_err(|e| format!("no answer: {e}"))?;
    match reply.trim() {
        "ok" => Ok(()),
        other => Err(other.to_string()),
    }
}

/// `POST /api/apps/resize` -- `{"slug":"…","width":1600,"height":1000}`.
///
/// The browser is the only thing that knows how big the window is, and on this
/// path it is also the only thing that can say so: the compositor will resize on
/// request and the VNC server will not pass the request on, so it comes back out
/// of band through WebDesk instead.
///
/// Open to any session, and scoped to that session's own uid, for the same reason
/// `open` is: this changes the size of a compositor running as you, and somebody
/// else's is not reachable from here by any spelling of the request.
pub async fn resize(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some(session) = crate::session_of(&state, &headers) else {
        return crate::unauthorized();
    };
    #[derive(Deserialize)]
    struct Req {
        slug: String,
        width: u32,
        height: u32,
        /// How much bigger the application should draw itself.
        ///
        /// Not the same question as the size, and worth saying why they are one
        /// request. The width and height are the framebuffer -- what the browser
        /// paints one pixel for one pixel. The scale divides that into the
        /// logical space the application lays itself out in, so 200% on the same
        /// window means the same sharpness with everything twice the size, rather
        /// than a smaller picture stretched. Sending them together is what keeps
        /// the output from being briefly at a new size with an old scale, which
        /// reads as a flicker.
        #[serde(default = "one")]
        scale: f32,
    }
    fn one() -> f32 {
        1.0
    }
    let Ok(req) = serde_json::from_slice::<Req>(&body) else {
        return bad(StatusCode::BAD_REQUEST, "a slug and a size are needed");
    };
    if catalog::find(&req.slug).is_none() {
        return bad(StatusCode::NOT_FOUND, "no application by that name");
    }
    let (uid, slug) = (session.ident.uid, req.slug.clone());
    let (w, h, scale) = (req.width, req.height, req.scale);
    match tokio::task::spawn_blocking(move || tell_session_size(uid, &slug, w, h, scale)).await {
        Ok(Ok(())) => {
            Json(json!({ "ok": true, "width": w, "height": h, "scale": scale })).into_response()
        }
        // Not an error the browser should act on: the size it asked for is the
        // size the session was started with in the ordinary case, and a window
        // that popped a message every time it was dragged would be worse than one
        // that stayed the size it was.
        Ok(Err(e)) => Json(json!({ "ok": false, "reason": e })).into_response(),
        Err(e) => bad(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_app() -> catalog::App {
        catalog::App {
            slug: "demo",
            name: "Demo",
            tagline: "",
            icon: "a-box",
            streamed: catalog::Streamed {
                flatpak: catalog::Flatpak {
                    id: "org.example.Demo",
                    source: catalog::FlatpakSource::Flathub,
                },
                width: 1280,
                height: 800,
            },
            notes: "",
        }
    }

    fn record(slug: &str, id: &str) -> Installed {
        Installed {
            slug: slug.into(),
            flatpak: id.into(),
            adopted: false,
            installed: 0,
            actor: String::new(),
        }
    }

    /// An identity with nothing in it but a name and a uid, for the tests about
    /// what is built from one. Never a real account on the machine running these:
    /// the point of every one of them is that nothing is looked up.
    fn ident(username: &str, uid: u32) -> auth::Identity {
        auth::Identity {
            username: username.into(),
            uid,
            gid: uid,
            home: format!("/home/{username}"),
            admin: false,
        }
    }

    #[test]
    fn every_icon_the_catalog_names_is_in_the_sprite() {
        // A renamed or mistyped id is invisible until someone opens the Apps
        // window and finds a blank square, so it is checked at build time
        // against the file that actually ships.
        const SPRITE: &str = include_str!("../ui/ui-icons.svg");
        for a in catalog::CATALOG {
            assert!(
                SPRITE.contains(&format!("id=\"{}\"", a.icon)),
                "{} names icon {}, which is not in ui/ui-icons.svg",
                a.slug,
                a.icon
            );
        }
        // The fallback the UI draws for an app whose entry has gone away.
        assert!(SPRITE.contains("id=\"a-box\""));
    }

    /// An install that cannot succeed must refuse before it starts rather than
    /// download several hundred megabytes and fail at the first click, with the
    /// reason in a user unit's journal. The refusal has to be structured as well
    /// as polite: the browser turns it into the button that installs what is
    /// missing, so the keys `/api/deps/install` takes have to be in it.
    #[test]
    fn an_install_refuses_a_host_that_has_not_got_what_it_needs() {
        static CAGE: crate::deps::Dep = crate::deps::Dep {
            offered: true,
            group: None,
            vendor_repo: None,
            key: "cage",
            label: "Cage",
            why: "Without it there is no compositor to draw the application into.",
            need: crate::deps::Need::Streamed,
            prereq: catalog::Prereq {
                bin: "cage",
                dnf: Some("cage"),
                apt: Some("cage"),
                pacman: Some("cage"),
                zypper: None,
            },
        };
        let body = missing_deps(&demo_app(), &[&CAGE]);

        // A client that reads nothing but `error` still gets a sentence naming
        // what is wrong, which is what makes this an ordinary refusal rather than
        // a prompt only the current UI understands.
        let error = body["error"].as_str().unwrap();
        assert!(error.contains("Cage"), "{error}");
        // And the key, unturned into anything: it is the argument the installer
        // endpoint takes, and a browser that had to translate a label back into
        // one would be a second place to get the mapping wrong.
        assert_eq!(body["offer"]["deps"][0]["key"], "cage");
        assert!(body["offer"]["detail"].as_str().unwrap().contains("Demo"));
    }

    /// `open` refuses two different things and says two different sentences.
    ///
    /// A slug with no record is not installed -- a mistyped name, or an app
    /// somebody removed while this tab was open. A record whose catalog entry has
    /// gone is installed and unstartable: `session.rs` resolves the slug against
    /// the catalog compiled into the binary, so starting the unit would answer
    /// `ok` and leave the browser waiting on a compositor that exited at once.
    #[test]
    fn open_refuses_what_is_not_installed_and_what_it_could_not_start() {
        // Matched rather than unwrap_err'd, so that `Installed` never needs a
        // Debug impl.
        let refusal = |book: &Book, slug: &str| match openable(book, slug) {
            Ok(_) => panic!("{slug} was allowed to open"),
            Err(e) => e,
        };

        let mut book = Book::default();
        assert!(refusal(&book, "nothing-here").contains("not installed"));

        book.apps.insert("gone-away".into(), record("gone-away", "org.example.Gone"));
        let e = refusal(&book, "gone-away");
        assert!(e.contains("catalog"), "{e}");

        // And one that is both installed and still in the catalog opens.
        let real = catalog::CATALOG[0].slug;
        book.apps.insert(real.into(), record(real, catalog::CATALOG[0].streamed.flatpak.id));
        assert!(openable(&book, real).is_ok());
    }

    /// The uid that names somebody's socket comes from their session and from
    /// nowhere else. That socket is the whole of a person's screen and keyboard,
    /// so a uid a request could supply would be a way to ask for somebody else's
    /// -- and the guard against it is that there is no uid anywhere in the request
    /// to read. A body that names one parses fine and the value goes nowhere.
    #[test]
    fn the_socket_path_is_built_from_the_session_and_never_from_the_request() {
        let req: SlugReq =
            serde_json::from_str(r#"{"slug":"gimp","uid":0,"user":"root","unit":"anything"}"#)
                .unwrap();

        let (mine, unit) = user_session(&ident("alice", 1000), &req.slug);
        let (theirs, _) = user_session(&ident("bob", 1001), &req.slug);

        assert_eq!(mine, crate::rfb::socket_path(1000, "gimp"));
        assert_eq!(theirs, crate::rfb::socket_path(1001, "gimp"));
        assert_ne!(mine, theirs, "two people would share one screen");
        // The slug becomes a unit name in exactly one place, and this is the call
        // that uses it. A second spelling here is how an open and a close would
        // end up naming different units and Close would silently do nothing.
        assert_eq!(unit, crate::systemd::app_unit("gimp"));
        assert!(mine.starts_with(crate::rfb::socket_dir(1000)));
    }

    /// Consent is asked for once, for the destructive half, and only when there is
    /// something destructive to do. WebDesk uninstalls what it installed and
    /// leaves what it found, so the record has to remember which -- and it has to
    /// remember it from install time, because afterwards the two are
    /// indistinguishable.
    #[test]
    fn a_flatpak_this_host_already_had_is_marked_so_removal_can_leave_it() {
        let ours = record("gimp", "org.gimp.GIMP");
        assert!(!ours.adopted, "a record must default to WebDesk having installed it");

        let theirs = Installed { adopted: true, ..record("gimp", "org.gimp.GIMP") };
        assert!(theirs.adopted);
    }

    /// The record survives a round trip through the file it is stored in.
    ///
    /// Worth asserting because the shape changed: a record written by a build
    /// that still had containers carries a dozen fields this one does not read,
    /// and carried the application id in an `Option` rather than a `String`. The
    /// extra fields are ignored, which is what `serde` does by default and is
    /// the behaviour wanted. The `Option` is not -- a book from such a host
    /// fails to parse, and `read_book` answers with an empty book rather than an
    /// error, so the cost is a dock that has forgotten what is installed rather
    /// than a process that will not start. Reinstalling puts the record back;
    /// the Flatpaks were never touched.
    #[test]
    fn a_record_round_trips_through_the_book() {
        let mut book = Book::default();
        book.apps.insert("gimp".into(), record("gimp", "org.gimp.GIMP"));
        let text = serde_json::to_string(&book).unwrap();
        let back: Book = serde_json::from_str(&text).unwrap();
        assert_eq!(back.apps["gimp"].flatpak, "org.gimp.GIMP");
        assert!(!back.apps["gimp"].adopted);
    }
}
