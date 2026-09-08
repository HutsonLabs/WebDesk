//! Finding a program on `PATH`.
//!
//! One function, in a module of its own, because five callers need it and none
//! of them owns it. It lived in `engine.rs` while there was a container engine
//! to detect; when that file went, the lookup was the only part of it anything
//! else wanted -- `flatpak.rs` looks for `flatpak`, `systemd.rs` for
//! `systemctl`, `cockpit.rs` for `cockpit-bridge`, `session.rs` for a
//! compositor, and `deps.rs` for whatever a row names. Leaving it in one of
//! them would have made the other four depend on that one for no reason.
//!
//! Not `which(1)`. Shelling out to a program in order to find a program is a
//! process spawn, a shell's idea of `PATH`, and an exit code to interpret,
//! where the answer is a handful of `stat` calls. The default is the one a
//! login shell would have on every target distribution, for the case where
//! `PATH` is unset -- a systemd unit with no `Environment=PATH` is the ordinary
//! way that happens, and it is exactly the context these lookups run in.

use std::path::PathBuf;

pub fn which(prog: &str) -> Option<PathBuf> {
    std::env::var("PATH")
        .unwrap_or_else(|_| "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into())
        .split(':')
        .filter(|d| !d.is_empty())
        .map(|d| std::path::Path::new(d).join(prog))
        .find(|p| p.is_file())
}
