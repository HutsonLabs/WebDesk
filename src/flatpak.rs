//! Flatpak, for the applications this host runs.
//!
//! `systemd.rs` runs the unit; this is what the unit's `ExecStart` names. The
//! application has to be on the machine before that unit is ever started, or the
//! service starts and is dead within a second with nothing in the Apps window
//! saying why -- so putting it there is a step of its own, before any record is
//! written.
//!
//! **Nothing here takes a name from a request.** The application id and the
//! repository a bundle comes from are `&'static str` in `catalog.rs`, for the
//! same reason a unit template is a constant: a request that could name an
//! application is a request that can install anything.
//!
//! **Two sources, and they make installing mean two different things.** Every
//! shipping entry names a Flathub id, and for those the remote *is* the
//! mechanism: `flatpak install` puts the application on the host and
//! `flatpak update` upgrades it, with no version to work out here, no asset to
//! find and nothing per-entry to configure. That is the reason the shelf in
//! `catalog.rs` could grow as fast as it did -- an entry costs an id.
//!
//! A **bundle** is the other source, and nothing ships as one today. It is kept
//! because the difference is not cosmetic and rediscovering it would be
//! expensive. A Flatpak built with `flatpak build-bundle` and no
//! `--runtime-repo` reports an origin that `flatpak remotes` has never heard of,
//! so `flatpak update` answers "Nothing to do" forever. For an application
//! published that way there really is no remote to update from: installing is
//! downloading a file and so is upgrading, which is why `newest_bundle` exists
//! rather than a one-line install against a remote, and why it walks the
//! releases instead of trusting `latest`. `term-hut-host` was the entry that
//! needed it; it went with the host-service kind, and this did not.
//!
//! `provide` and `update` are where the two meet. The installer asks for the
//! application and is not told which kind it got, because the difference is
//! entirely about where the bytes come from and not at all about what is on the
//! host afterwards -- and an installer that had to know would grow the same
//! two-branch decision a second time, in a file that has no reason to hold it.

use crate::catalog::{Flatpak, FlatpakSource};
use crate::which::which;
use std::path::Path;
use std::process::{Command, Stdio};

/// Whether this host already has the application.
///
/// `flatpak info` rather than parsing `flatpak list`: it exits non-zero for an
/// id that is not installed, so the answer is the exit code and there is no
/// output to misread.
pub fn installed(id: &str) -> bool {
    Command::new("flatpak")
        .args(["info", id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// This host's architecture in the words a release asset is named with.
///
/// The two the bundles are built for. Anything else returns `None` and the
/// install refuses rather than downloading a bundle that cannot run here.
pub fn arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x86_64"),
        "aarch64" => Some("aarch64"),
        _ => None,
    }
}

/// The host package managers this knows how to install a package with.
#[derive(Clone, Copy, PartialEq)]
pub enum Manager {
    Dnf,
    Apt,
    Pacman,
    Zypper,
}

impl Manager {
    pub fn bin(&self) -> &'static str {
        match self {
            Manager::Dnf => "dnf",
            Manager::Apt => "apt-get",
            Manager::Pacman => "pacman",
            Manager::Zypper => "zypper",
        }
    }

    /// The argv that installs packages without asking anybody anything. This
    /// runs with no terminal attached, so a manager that stops to confirm would
    /// hang the install rather than fail it.
    fn args(&self) -> &'static [&'static str] {
        match self {
            Manager::Dnf => &["install", "-y"],
            Manager::Apt => &["install", "-y"],
            Manager::Pacman => &["-S", "--noconfirm"],
            Manager::Zypper => &["install", "-y"],
        }
    }
}

/// Which manager this host has, in the order the target distributions are
/// listed in the README. `None` is a host WebDesk will not install packages on,
/// which is reported rather than guessed at.
pub fn manager() -> Option<Manager> {
    for m in [Manager::Dnf, Manager::Apt, Manager::Pacman, Manager::Zypper] {
        if which(m.bin()).is_some() {
            return Some(m);
        }
    }
    None
}

/// Install host packages, with everything the manager says going to the log the
/// Apps window is already streaming.
pub fn install_packages(packages: &[String], log: &Path) -> Result<(), String> {
    let Some(m) = manager() else {
        return Err("no package manager on this host".into());
    };
    let mut args: Vec<String> = m.args().iter().map(|s| s.to_string()).collect();
    args.extend(packages.iter().cloned());
    logged(m.bin(), &args, log)
}

/// The newest release carrying a bundle for this architecture, as
/// `(version, download url)`.
///
/// Walks the releases rather than taking `/releases/latest`, and that is not
/// caution for its own sake. It was written against a repository whose newest
/// release was a macOS-only build with no `.flatpak` asset at all, so `latest`
/// would have refused to install on a host where a dozen usable bundles were one
/// page down. A project that publishes for several platforms out of one release
/// stream is the ordinary case, not the odd one.
pub fn newest_bundle(repo: &str) -> Result<(String, String), String> {
    let Some(arch) = arch() else {
        return Err(format!("no bundle is built for {}", std::env::consts::ARCH));
    };
    let suffix = format!("_{arch}.flatpak");
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=30");
    let releases = crate::update::github_json(&url)?;
    let list = releases.as_array().ok_or("unexpected release list")?;

    for release in list {
        let Some(assets) = release["assets"].as_array() else { continue };
        for asset in assets {
            let name = asset["name"].as_str().unwrap_or_default();
            if !name.ends_with(&suffix) {
                continue;
            }
            let Some(url) = asset["browser_download_url"].as_str() else { continue };
            let version = release["tag_name"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('v')
                .to_string();
            return Ok((version, url.to_string()));
        }
    }
    Err(format!("no {arch} bundle in the last 30 releases of {repo}"))
}

/// Flathub's repository description, as `flatpak remote-add` takes it.
///
/// A constant here rather than a field on an entry. A remote is where code comes
/// from, so a remote an entry could name is a remote a request could name one
/// refactor later -- the same rule that keeps unit bodies and application ids in
/// `catalog.rs` out of reach of the browser.
pub const FLATHUB_URL: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Add the Flathub remote if this host has not got it. Idempotent by the flag,
/// so this is safe to call before every install and cheap when it is a no-op.
pub fn ensure_flathub(log: &Path) -> Result<(), String> {
    logged(
        "flatpak",
        &[
            "remote-add".into(),
            "--if-not-exists".into(),
            "--system".into(),
            "flathub".into(),
            FLATHUB_URL.into(),
        ],
        log,
    )
}

/// Put the application on the host, whichever way this entry gets one.
///
/// The one entry point the installer calls, so that "where does this Flatpak
/// come from" is answered here and not in `apps.rs`. Already-installed is the
/// ordinary case on a host that had the app before this entry did, and
/// reinstalling over it would cost minutes to arrive where it already is.
pub fn provide(fp: &Flatpak, log: &Path) -> Result<(), String> {
    if installed(fp.id) {
        return Ok(());
    }
    match fp.source {
        FlatpakSource::Flathub => {
            ensure_flathub(log)?;
            logged(
                "flatpak",
                &[
                    "install".into(),
                    "-y".into(),
                    "--system".into(),
                    "--noninteractive".into(),
                    "flathub".into(),
                    fp.id.into(),
                ],
                log,
            )
        }
        FlatpakSource::Bundle { repo } => {
            let (version, url) = newest_bundle(repo)?;
            tracing::info!(id = %fp.id, %version, "installing a bundle");
            install_bundle(&url, log)
        }
    }
}

/// Bring the application up to date, which is two different operations.
///
/// Nothing calls this yet, and that is recorded in the README's *Not built yet*
/// rather than hidden behind an `#[allow]` with no explanation. The mechanism is
/// the part that was worth building with the rest of the Flatpak path, because
/// it is the part that would have been guessed at later; the button, the route
/// and the question of who may press it are a separate decision, and the
/// README lists that decision under Not built yet.
///
/// A remote has a repository behind it, so this is one command. A bundle has
/// none -- `flatpak update` answers "Nothing to do" forever against an origin no
/// remote knows -- so upgrading is downloading the newest file again.
#[allow(dead_code)]
pub fn update(fp: &Flatpak, log: &Path) -> Result<(), String> {
    match fp.source {
        FlatpakSource::Flathub => logged(
            "flatpak",
            &[
                "update".into(),
                "-y".into(),
                "--system".into(),
                "--noninteractive".into(),
                fp.id.into(),
            ],
            log,
        ),
        FlatpakSource::Bundle { repo } => {
            let (_, url) = newest_bundle(repo)?;
            install_bundle(&url, log)
        }
    }
}

/// Download a bundle and install it system-wide.
///
/// `--system` rather than `--user` because the unit is a system unit: a user
/// installation lives under the installing user's home and would not be there
/// for the service. `--reinstall` so that installing over the same version is
/// an upgrade path rather than an error.
pub fn install_bundle(url: &str, log: &Path) -> Result<(), String> {
    let file = std::env::temp_dir().join("webdesk-termhut.flatpak");
    let path = file.to_string_lossy().to_string();
    logged(
        "curl",
        &["-fsSL".into(), "--max-time".into(), "900".into(), "-o".into(), path.clone(), url.into()],
        log,
    )?;
    let result = logged(
        "flatpak",
        &["install".into(), "-y".into(), "--system".into(), "--reinstall".into(), path.clone()],
        log,
    );
    // The bundle is a few hundred megabytes and nothing reads it again.
    let _ = std::fs::remove_file(&file);
    result
}

/// Keep the service user's runtime directory and bus alive with nobody logged
/// in, which is what `flatpak-spawn --host` needs to reach the portal.
///
/// Best effort: a host where this fails is one where the service may still come
/// up, and a hard failure here would refuse an install over something only some
/// of the app's features need.
pub fn enable_linger(user: &str) {
    let _ = Command::new("loginctl")
        .args(["enable-linger", user])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// One command, with its output appended to the install log.
///
/// The same shape as `engine::run_logged` and for the same reason: the Apps
/// window is already polling this file, so anything written here is on screen
/// while it happens rather than summarised after it fails.
pub(crate) fn logged(bin: &str, args: &[String], log: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| format!("could not open the install log: {e}"))?;
    let _ = writeln!(file, "$ {bin} {}", args.join(" "));

    let err_file = file.try_clone().map_err(|e| format!("could not open the install log: {e}"))?;
    let status = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(err_file))
        .status()
        .map_err(|e| format!("could not run {bin}: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{bin} {} failed ({status})", args.first().map(String::as_str).unwrap_or("")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An id nothing could have installed reads as absent rather than as an
    /// error, on a host with flatpak and on one without -- the second half is
    /// what keeps this runnable on the machines this is developed on.
    #[test]
    fn an_app_that_is_not_installed_is_not_an_error() {
        assert!(!installed("com.example.definitely-not-installed"));
    }

    /// A Flathub id is a name on somebody else's server, so nothing in this
    /// repository can confirm that it exists. What can be confirmed is its
    /// shape, and that is worth doing because of where the alternative fails: a
    /// mistyped id builds, ships, and then costs somebody an install on the
    /// deployment host that runs long enough to add the remote before `flatpak
    /// install` says the reference was not found. The shape catches the
    /// mistakes that actually happen -- an image reference or a human-readable
    /// name pasted into the id field, or a component dropped -- and a
    /// reverse-DNS application id has at least three of those components, no
    /// slash and no space.
    #[test]
    fn every_flathub_id_looks_like_an_application_id() {
        for app in crate::catalog::CATALOG {
            let fp = &app.streamed.flatpak;
            if matches!(fp.source, FlatpakSource::Flathub) {
                assert!(
                    fp.id.matches('.').count() >= 2,
                    "{}: {} has too few components to be a reverse-DNS application id",
                    app.slug,
                    fp.id
                );
                assert!(
                    !fp.id.contains('/') && !fp.id.contains(' '),
                    "{}: {} reads as an image reference or a display name, not an id",
                    app.slug,
                    fp.id
                );
                assert!(
                    fp.id.split('.').all(|part| !part.is_empty()),
                    "{}: {} has an empty component, so a dot is doubled or trailing",
                    app.slug,
                    fp.id
                );
            }
        }
    }

    /// The architectures the bundles are built for. A host that is neither is
    /// told so instead of being handed a bundle that cannot run.
    #[test]
    fn only_the_two_built_architectures_are_offered() {
        assert!(matches!(arch(), Some("x86_64") | Some("aarch64") | None));
    }
}
