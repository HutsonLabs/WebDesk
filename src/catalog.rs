//! The catalog of installable applications.
//!
//! Every application WebDesk can install is described here, in the binary. It
//! is deliberately not a file on disk and not something the browser can add to:
//! the set of things that may be run on this host is a property of the build,
//! reviewed like any other code. "Install an app" means "choose one of these",
//! and nothing else.
//!
//! **One kind of entry.** Every entry is a Flatpak that runs on this host, as
//! the person who opened it, and is drawn into a WebDesk window: a headless
//! `cage` holding exactly one application, `wayvnc` turning that into RFB on a
//! Unix socket nothing but this process can open, and a canvas in the browser at
//! the other end. The address of an app is `/ws/rfb/<slug>` and there is no
//! other one.
//!
//! There used to be three kinds, and the other two were both HTTP services
//! WebDesk reverse-proxied under `/app/<slug>/`. That prefix is gone, and with
//! it the whole apparatus that existed to serve it: a container engine to detect
//! and drive, a port allocator, a bind-mount validator, `PUID`/`PGID`, a shared
//! memory size, a render-node passthrough, a per-app cookie rewriter, a
//! `X-Forwarded-Prefix` convention, and a `needs_origin` escape hatch that gave
//! an app a second listener when none of that was enough.
//!
//! **Every one of those was a question about how to make a container resemble
//! this machine and this user closely enough to be useful.** An application
//! already running on this machine as this user has no such question to answer.
//! There is no port, because nothing listens on one. There is no prefix, because
//! there is no proxy to be served under. There is no `/config`, because the
//! application keeps its state in `~/.var/app/<id>`, where Flatpak already put
//! it, per user. There is no `PUID`/`PGID`, because the process *is* the
//! signed-in user rather than a container being told to impersonate them. There
//! is no `TZ`, because it reads the host's clock, which is the clock the answer
//! would have been copied off. There is no shm size, because there is no
//! container whose `/dev/shm` was capped at 64 MB. And there is no render-node
//! flag, because a device on the host is not something you hand to a process on
//! the host -- it is simply already open to it.
//!
//! Those are not seven simplifications. They are one, said seven times. It is
//! what makes an entry here a name, an id, an icon and a first window size:
//! `flathub!` is short because there is genuinely nothing else to say, and the
//! reviewing that matters is about whether the application is worth having
//! rather than about whether it can be made to work.
//!
//! It also settles four of the README's old Known limits outright rather than
//! promising to. The container desktops put downloads in
//! `/var/lib/webdesk/appdata/<slug>/Downloads` instead of `~/Downloads`; every
//! one of the Selkies images shipped a passwordless root shell; they mounted
//! `/home` read-write; and each of them needed a gigabyte of `/dev/shm`. None of
//! those was a fix somebody had not got round to -- they are what a container
//! has to do to approximate a desktop session. A Flatpak on the host is not
//! approximating one. The user's real home, their fonts, their theme, the GPU
//! and a working `xdg-desktop-portal` are already there, so there is nothing to
//! bind in and nothing to loosen.
//!
//! **The cost, said plainly: the container boundary is gone.** A streamed
//! application can reach whatever its Flatpak sandbox permits, in the account of
//! whoever pressed open. That is the point of it rather than a flaw in it -- an
//! application whose subject is your files is worth nothing pointed at somebody
//! else's -- and it is why installing one is gated on the administrative group
//! while merely opening one is not.
//!
//! **What is not here, and where it went.** An operator who wants a web
//! application on the desk no longer gets one by having WebDesk run it. They run
//! it themselves, wherever they already run things, and give the desk a URL. See
//! `docs/url-apps.md`: that arrangement is not a catalog entry, because a URL is
//! not code this build has reviewed, and it is not this file's business.
//!
//! **An entry can cost too much to keep.** `intellij-idea` was here and is not
//! any more. Nothing about it was broken; it unpacks to roughly 9 GB, which on
//! the deployment host was more than the free space on the filesystem images
//! were stored in -- so the one entry most likely to fail an install was also
//! the one whose failure would take the rest of the machine down with it, by
//! filling the disk other services were writing to. Size is a property of an
//! entry like any other, and this is what it looks like when it decides the
//! answer. The arithmetic is gentler now: a Flathub install is an application
//! plus a runtime, and the runtime is shared, so the first GNOME entry pays
//! about 900 MB for `org.gnome.Platform` and every GNOME entry after it pays
//! only for itself. The entries worth arguing over are the ones that bring a
//! runtime -- or a Java runtime -- nothing else will use.

/// Where a Flatpak comes from, which decides what installing and updating mean.
///
/// Two shapes, and the difference is not cosmetic. A remote has a repository
/// behind it, so `flatpak update` is a real upgrade path and the whole install
/// is one command with no version to work out. A bundle has none, so installing
/// is downloading a file and so is upgrading -- which is why `newest_bundle`
/// exists at all.
/// `Bundle` is unconstructed today and kept deliberately, the way `flatpak.rs`
/// still knows how to install one. It was `term-hut-host`'s source, and that
/// entry went with the host-service kind; the capability is three functions in
/// `flatpak.rs` that already work, and an entry published outside Flathub is an
/// ordinary thing to meet. Deleting it would mean rediscovering `newest_bundle`
/// the next time one turns up.
#[allow(dead_code)]
pub enum FlatpakSource {
    /// Flathub, the remote nearly every desktop Flatpak is published to.
    ///
    /// `flatpak install --system flathub <id>` is the entire install, and
    /// `flatpak update --system <id>` the entire update. There is nothing
    /// per-entry to configure, which is the point: an entry naming a Flathub id
    /// is a name and an icon and nothing else.
    ///
    /// The remote URL is a constant in `flatpak.rs`, not a field here. A remote
    /// that could be named by an entry would be a remote that could be named by
    /// a request one refactor later, and the rule this file is built on is that
    /// the set of things that may be run is a property of the build.
    Flathub,
    /// A bundle from a GitHub repository's releases.
    ///
    /// For an application that publishes no remote to add. A Flatpak built with
    /// `flatpak build-bundle` and no `--runtime-repo` reports an origin no
    /// `flatpak remotes` knows, and `flatpak update` answers "Nothing to do"
    /// forever -- so the newest release has to be found and fetched by hand.
    Bundle { repo: &'static str },
}

/// An application that draws on this host and is streamed into a window here.
///
/// The only kind of entry there is, and the one that gets closest to running
/// the application locally -- because it *is* running locally. A Flatpak on the
/// host has the signed-in user's real home directory, their fonts, their theme,
/// the machine's GPU and a working `xdg-desktop-portal`, none of which a
/// container could be given without being handed the host.
///
/// What WebDesk adds is a way to see it: a headless `cage` holding exactly one
/// application, `wayvnc` turning that into RFB on a socket nothing but this
/// process can open, and `rfb.rs` carrying those bytes to a canvas in the
/// browser. No port is published, no image is pulled, no prefix is negotiated,
/// and no state directory is invented -- the app keeps its state where Flatpak
/// already puts it, in `~/.var/app/<id>`, per user.
///
/// **Installed once, run per user.** Installing is `--system`, host-wide, and
/// gated on the administrative group: one copy on disk, part of the machine
/// like a package. Running is a systemd *user* unit in
/// the session of whoever opened it, because an application whose subject is
/// your files is worth nothing pointed at somebody else's. See
/// `systemd::APP_UNIT`.
pub struct Streamed {
    /// The Flatpak this entry runs.
    pub flatpak: Flatpak,
    /// The size of the headless output `cage` is started with, in pixels.
    ///
    /// A starting point rather than a limit: it is what the compositor comes up
    /// at before the browser has said how big its window is. Chosen per entry
    /// because a terminal and an image editor do not want the same first
    /// impression.
    pub width: u16,
    pub height: u16,
}

/// A Flatpak-packaged application WebDesk installs before starting its unit.
pub struct Flatpak {
    /// The application id, as `flatpak info` would be given it.
    pub id: &'static str,
    /// Where it comes from, and so what installing and updating mean.
    pub source: FlatpakSource,
}

/// A host program the service cannot start without, and how to get it.
///
/// The package name is per manager and deliberately incomplete: a manager with
/// no name here is one where nobody has checked what provides this, and
/// guessing would install the wrong thing or fail with a message about a
/// package that never existed. `None` there means the install refuses and
/// prints `provision` instead, which is the honest answer.
pub struct Prereq {
    /// The binary to look for on `PATH`.
    pub bin: &'static str,
    /// `dnf`/`yum` package name.
    pub dnf: Option<&'static str>,
    /// `apt-get` package name.
    pub apt: Option<&'static str>,
    /// `pacman` package name.
    pub pacman: Option<&'static str>,
    /// `zypper` package name.
    pub zypper: Option<&'static str>,
}

pub struct App {
    /// The key of this entry everywhere that is not Rust.
    ///
    /// It reaches four places: the socket `/ws/rfb/<slug>`, the systemd user
    /// unit `webdesk-app@<slug>.service`, the key of the record in `apps.json`,
    /// and the argument `webdesk app-session` resolves back against this file.
    /// Two entries sharing one would collide in every one of those -- and not
    /// by failing. The second install would find the first's record and refuse.
    pub slug: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub icon: &'static str,
    /// The Flatpak this entry runs, and the size it first draws at.
    pub streamed: Streamed,
    pub notes: &'static str,
}

/// One Flathub application, drawn on this host and streamed into a window.
///
/// The shortness is the whole argument for this arrangement. A container entry
/// had to answer for a published port, a state directory, `PUID`/`PGID`, a
/// shared memory size, a clock, a render node and whether the application
/// tolerated a path prefix. None of those questions exist here. The app runs on
/// the host as the person who opened it, so its state, its identity, its fonts,
/// its GPU and its clock are already the right ones, and there is no prefix
/// because there is no proxy.
///
/// What is left is a name, an id, an icon and a first window size -- and of
/// those only the id is load-bearing. `scripts/flathub-entry.py` writes one of
/// these from an application id, which is the intended way to add an app.
macro_rules! flathub {
    ($slug:literal, $name:literal, $id:literal, $icon:literal, $tagline:literal,
     $w:literal x $h:literal $(,)?) => {
        App {
            slug: $slug,
            name: $name,
            tagline: $tagline,
            icon: $icon,
            streamed: Streamed {
                // Nothing beyond the compositor and the RFB server stands
                // between an entry and running, and those are host-wide rather
                // than per entry -- see `deps::RUNTIME`. That an entry needs no
                // prerequisite of its own is the other half of why it is this
                // short.
                flatpak: Flatpak { id: $id, source: FlatpakSource::Flathub },
                width: $w,
                height: $h,
            },
            notes: "Runs on this host as you, with your home directory, your fonts and your \
                    GPU, and is drawn into this window. Its files are your files.",
        }
    };
}

pub static CATALOG: &[App] = &[
    // Every one of these names its own mark rather than `a-box`. Most come from
    // Simple Icons through `scripts/brand-icons.py`; Remmina and Disk Usage
    // Analyzer are not in that set and sit in the sprite by hand. `a-box` stays
    // as the fallback the UI reaches for when an installed app's catalog entry
    // has gone away, which is the only case left that needs it. An entry naming
    // an icon that is not in `ui/ui-icons.svg` would draw a blank square, and
    // `every_icon_the_catalog_names_is_in_the_sprite` is what catches that
    // before somebody opens the Apps window.
    //
    // Firefox, Inkscape and OnlyOffice were LinuxServer images before they were
    // entries here. Each one lost a multi-gigabyte pull, a passwordless root
    // shell, a share of `/home` handed to every container on the box, and a
    // gigabyte of shared memory. What each one gained is your actual home
    // directory, the resolution following the window, and a zoom.
    flathub!(
        "firefox",
        "Firefox",
        "org.mozilla.firefox",
        "a-firefox",
        "The browser, running on this host rather than on your machine.",
        // A browser is the one application where the window is the point, so it
        // opens at the size the rest of the working entries do rather than at
        // the 1280x720 its own screenshot suggests.
        1600 x 1000,
    ),
    flathub!(
        "inkscape",
        "Inkscape",
        "org.inkscape.Inkscape",
        "a-inkscape",
        "Vector drawing, for the SVGs this desktop is drawn with.",
        // 295 MB on the GNOME runtime the rest of this shelf already pays for,
        // against roughly 3 GB unpacked as an image. The cheapest of the three
        // to move and the one whose files most want to be the host's.
        1600 x 1000,
    ),
    flathub!(
        "onlyoffice",
        "OnlyOffice",
        "org.onlyoffice.desktopeditors",
        "a-onlyoffice",
        "Documents, spreadsheets and slides, close to the shapes Office makes.",
        // 1.2 GB on the Freedesktop runtime, where the image was around 6 GB
        // unpacked -- the entry the README singles out for checking `df` before
        // installing. It is still the largest thing here and no longer the kind
        // of large that decides anything.
        //
        // `org.onlyoffice.desktopeditors`, so the id's own tail would make the
        // slug `desktopeditors`. It is `onlyoffice` instead: a slug is what a
        // person types and what a container was once named, and the entry this
        // replaces was called that.
        1600 x 1000,
    ),
    flathub!(
        "gimp",
        "GIMP",
        "org.gimp.GIMP",
        "a-gimp",
        "Photo and image editing, on the machine the images are already on.",
        // Roughly 1.3 GB installed, most of which is the GNOME runtime the rest
        // of this shelf then reuses for nothing. It earns the space by being
        // the half of the drawing story Inkscape is not: that entry makes
        // vectors, this one edits pixels, and between them a screenshot taken
        // on this host can be cropped and the logo next to it redrawn without
        // either file being downloaded, edited elsewhere and uploaded back.
        1600 x 1000,
    ),
    flathub!(
        "dbeaver",
        "DBeaver",
        "io.dbeaver.DBeaverCommunity",
        "a-dbeaver",
        "A database client, on the side of the firewall the database is on.",
        // Roughly 800 MB, because it carries a Java runtime of its own and
        // shares nothing with the GNOME entries around it -- the most expensive
        // thing here, and still the least arguable. A Postgres or MySQL bound
        // to 127.0.0.1, which is how it ought to be bound, is reachable from
        // exactly one machine and this is that machine. What people do instead
        // is open an SSH tunnel from a laptop, which is the same access with a
        // second credential to manage and a step to forget.
        1600 x 1000,
    ),
    flathub!(
        "remmina",
        "Remmina",
        "org.remmina.Remmina",
        "a-remmina",
        "RDP, VNC and SSH out to the other machines this host can see.",
        // Small, and the only entry here whose subject is not this host. A
        // server usually sits on a segment a laptop cannot reach: the
        // hypervisor's management interface, a switch, the Windows box holding
        // the licence server. Streaming a remote-desktop client from inside
        // that segment makes WebDesk the jump host, which is a thing operators
        // otherwise build on purpose and then have to maintain.
        1280 x 800,
    ),
    flathub!(
        "baobab",
        "Disk Analyzer",
        "org.gnome.baobab",
        "a-baobab",
        "Where the disk went, as a picture rather than a column of numbers.",
        // A few megabytes, and it is on this shelf because of `intellij-idea`.
        // The paragraph above about size describes an install that could have
        // filled the filesystem other services were writing to; this is the
        // tool for the morning after one does. `du -sh` reaches the same answer
        // eventually, one directory at a time, and the difference is that this
        // shows the whole tree at once -- which matters most in the case where
        // you do not yet know where to look.
        1100 x 750,
    ),
    flathub!(
        "localsend-app",
        "LocalSend",
        "org.localsend.localsend_app",
        "a-localsend",
        "Send files to the machines beside this one, with no share to set up.",
        // 55 MB on the Freedesktop runtime OnlyOffice already paid for, which
        // makes it near enough free. It answers the question the Files window
        // raises and cannot answer itself: getting a file onto this host from
        // a laptop on the same segment, without a share to export, an upload
        // form to build or an `scp` incantation to get right.
        //
        // Note which way it looks. The peers it discovers are whatever *this
        // host* can see on its own network, not what the machine you are
        // sitting at can see. That is the useful direction here -- it is how a
        // file reaches a server that has no share -- and it is the surprising
        // one everywhere else, so it is worth knowing before the list of
        // devices is not the list you expected.
        //
        // It asks for a status-notifier bus name and there is no tray in this
        // compositor to give it. Closing the window ends the app rather than
        // hiding it, so it receives while it is open and not otherwise.
        1280 x 944,
    ),
    flathub!(
        "bitwarden",
        "Bitwarden",
        "com.bitwarden.desktop",
        "a-bitwarden",
        "The password vault, on the host where the passwords get used.",
        // 487 MB of Electron on the Freedesktop runtime: the largest thing
        // here after OnlyOffice, and the one whose real cost is not the disk.
        // A vault opened here is decrypted in a process on this machine, in
        // your own user session. That is the same trust already placed in the
        // host by having a shell on it, and it is still worth saying out loud,
        // because a password manager is exactly the application where "it runs
        // on the server" stops being an implementation detail.
        //
        // Two consequences to know before reaching for it. Its manifest asks
        // for `devices=all`, every device node this host has rather than the
        // render node, and a hardware key is plugged into *this* machine and
        // not into the one you are sitting at -- so FIDO2 unlock is the host's
        // key or it is nobody's. And it wants a tray it cannot have, so
        // closing the window ends it instead of minimising it, which for a
        // vault is the better of the two outcomes.
        1280 x 768,
    ),
];

pub fn find(slug: &str) -> Option<&'static App> {
    CATALOG.iter().find(|a| a.slug == slug)
}

/// The catalog as the browser sees it.
///
/// Shorter than it was by everything the proxy used to need. What is left is
/// what a tile is drawn from, plus the two facts a *window* needs before it can
/// exist.
///
/// The id travels because the window's own chrome names it. "This is
/// org.gimp.GIMP, running on this host as you" is the one fact about a streamed
/// app that a person cannot see for themselves once it is drawn, and it is the
/// fact that decides whether they should be typing anything into it.
///
/// The size travels because the canvas has to exist before the first frame
/// arrives, and creating it at a guessed size means the first thing anybody sees
/// is the window resizing itself. Nothing on the host can set the resolution:
/// `cage`'s output is created at a hardcoded 1280x720 and only a client asking
/// for a desktop size changes it, so the entry's width and height are the
/// resolution the application will run at, by way of the window they open.
pub fn as_json() -> serde_json::Value {
    let apps: Vec<_> = CATALOG
        .iter()
        .map(|a| {
            serde_json::json!({
                "slug": a.slug,
                "name": a.name,
                "tagline": a.tagline,
                "icon": a.icon,
                "notes": a.notes,
                "streamed": {
                    "flatpak": a.streamed.flatpak.id,
                    "width": a.streamed.width,
                    "height": a.streamed.height,
                },
            })
        })
        .collect();
    serde_json::json!({ "apps": apps })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slug is the one string an entry contributes to places that are not
    /// Rust, and a character outside `[a-z0-9-]` is a problem spread thin: a
    /// path, a systemd instance name, a JSON key and a directory would each want
    /// it escaped differently, and only some of them would say so when it was
    /// not.
    #[test]
    fn every_slug_is_unique_and_safe_everywhere_it_is_used() {
        let mut seen = std::collections::HashSet::new();
        for a in CATALOG {
            assert!(seen.insert(a.slug), "two entries are both called {}", a.slug);
            assert!(!a.slug.is_empty(), "{} has an empty slug", a.name);
            assert!(
                a.slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{} is not safe as a path, a unit instance and a directory at once",
                a.slug
            );
        }
    }

    /// Two entries naming one Flatpak id would share an installation on disk,
    /// and `remove` would take the application away from whichever of them was
    /// still installed. Not caught by the slug test: the slugs would differ.
    #[test]
    fn no_two_entries_name_the_same_flatpak() {
        let mut seen = std::collections::HashSet::new();
        for a in CATALOG {
            assert!(
                seen.insert(a.streamed.flatpak.id),
                "{} names an application id another entry already installs",
                a.slug
            );
        }
    }

    /// A window with no size is a canvas created at a guess and then resized in
    /// front of the person who opened it. Zero is the value a hand-written entry
    /// gets by forgetting, so it is the one worth refusing.
    #[test]
    fn every_entry_opens_at_a_size_somebody_chose() {
        for a in CATALOG {
            assert!(a.streamed.width >= 640, "{} opens too narrow to use", a.slug);
            assert!(a.streamed.height >= 480, "{} opens too short to use", a.slug);
        }
    }
}
