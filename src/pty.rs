//! Terminal sessions.
//!
//! The shell is started with `su - <user>`, which is the least code that gets
//! the privileges right: su is setuid-aware, runs its own PAM session, sets up
//! the environment and lands in the user's home. The daemon is root, so no
//! password is requested.
//!
//! One socket carries one shell, and five jobs move data across it: a thread
//! reading the PTY, a task writing to the browser, a thread writing to the PTY,
//! a task applying resizes and a thread waiting on the child. They are kept
//! apart on purpose. When reading and writing shared one loop, a shell that had
//! stopped draining its input -- anything holding ^S, or just a full pipe --
//! also stopped the loop from noticing a resize, and a browser that had stopped
//! reading pushed that stall all the way back into the shell.

use crate::{session_of, AppState};
use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch, Notify};

/// How often the server pings an idle socket.
///
/// Nothing crosses an idle terminal for hours at a time, and the things between
/// this process and the browser -- a NAT table, a reverse proxy, an access
/// point -- all reap a connection that quiet without telling either end. The
/// first anyone hears of it is the next keystroke going nowhere. A ping every
/// twenty seconds is well inside the shortest of those timeouts.
const PING_EVERY: Duration = Duration::from_secs(20);

/// How long a socket may go without answering a ping before it is presumed
/// dead. Three missed pings, so a single lost packet is not a dropped session.
const PONG_GRACE: Duration = Duration::from_secs(70);

/// How much unsent output may pile up before the oldest is discarded.
///
/// The alternative is worse than it sounds: blocking here blocks the thread
/// reading the PTY, which fills the kernel's PTY buffer, which blocks the
/// *shell*. A browser that cannot keep up would hang the command it is running.
/// A megabyte is far more than a slow link is ever behind by; past that, losing
/// the oldest scrollback is the cheapest thing to give up, and the browser is
/// told it happened rather than left with a silent hole.
const MAX_PENDING: usize = 1 << 20;

#[derive(Deserialize)]
#[serde(tag = "t")]
enum Control {
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

/// The window size the browser already knows at connect time.
///
/// Without it every session opens at 24x80 and re-wraps a moment later, which
/// is visible on every launch and corrupts the first prompt of anything that
/// drew itself before the resize arrived.
#[derive(Deserialize)]
pub struct Open {
    cols: Option<u16>,
    rows: Option<u16>,
}

/// Sizes outside this are a bug or a hostile client; the ioctl takes them
/// happily and the shell then misbehaves in ways that are hard to trace back.
fn sane(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(1, 1000), rows.clamp(1, 1000))
}

pub async fn ws_term(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(open): Query<Open>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(session) = session_of(&state, &headers) else {
        return (axum::http::StatusCode::UNAUTHORIZED, "not signed in").into_response();
    };
    let username = session.ident.username.clone();
    let size = sane(open.cols.unwrap_or(80), open.rows.unwrap_or(24));
    ws.on_upgrade(move |socket| run(socket, username, size))
}

fn find_su() -> Option<&'static str> {
    ["/bin/su", "/usr/bin/su"].into_iter().find(|p| std::path::Path::new(p).exists())
}

/// Output read from the PTY but not yet handed to the browser.
#[derive(Default)]
struct Pending {
    chunks: VecDeque<Vec<u8>>,
    bytes: usize,
    /// Bytes discarded under `MAX_PENDING` since the browser was last told.
    dropped: usize,
    /// The PTY has closed: the shell is gone and no more output is coming.
    eof: bool,
}

async fn run(socket: WebSocket, username: String, (cols, rows): (u16, u16)) {
    let (mut tx, mut rx) = socket.split();

    let Some(su) = find_su() else {
        let _ = tx.send(Message::Text("su not found on this system\r\n".into())).await;
        return;
    };

    let pair = match native_pty_system().openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            let _ = tx.send(Message::Text(format!("openpty failed: {e}\r\n").into())).await;
            return;
        }
    };

    let mut cmd = CommandBuilder::new(su);
    cmd.arg("-");
    cmd.arg(&username);
    cmd.env("TERM", "xterm-256color");

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Message::Text(format!("failed to start shell: {e}\r\n").into())).await;
            return;
        }
    };
    drop(pair.slave);

    // Kill through a cloned killer rather than the child itself: the child is
    // moved onto the thread that waits for it, which is the only way to learn
    // the exit status and the only thing that reaps it.
    let mut killer = child.clone_killer();

    let master = Arc::new(Mutex::new(pair.master));
    let (reader, writer) = {
        let m = master.lock().unwrap();
        match (m.try_clone_reader(), m.take_writer()) {
            (Ok(r), Ok(w)) => (r, w),
            _ => {
                let _ = killer.kill();
                return;
            }
        }
    };

    // The child is waited on, not merely killed. Before, a shell that exited on
    // its own stayed a zombie until the browser closed the tab, and the browser
    // was told "session ended" without ever being told what the status was.
    let (exit_tx, exit_rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = exit_tx.send(child.wait());
    });

    // PTY -> browser. Blocking reads live on their own thread, and never block
    // on the browser: see `MAX_PENDING`.
    let pending = Arc::new(Mutex::new(Pending::default()));
    let woke = Arc::new(Notify::new());
    {
        let pending = pending.clone();
        let woke = woke.clone();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        {
                            let mut p = pending.lock().unwrap();
                            p.chunks.push_back(buf[..n].to_vec());
                            p.bytes += n;
                            while p.bytes > MAX_PENDING {
                                match p.chunks.pop_front() {
                                    Some(old) => {
                                        p.bytes -= old.len();
                                        p.dropped += old.len();
                                    }
                                    None => break,
                                }
                            }
                        }
                        woke.notify_one();
                    }
                }
            }
            pending.lock().unwrap().eof = true;
            woke.notify_one();
        });
    }

    // browser -> PTY, on a thread of its own so that a write which blocks --
    // the shell not reading, the buffer full -- stalls only itself.
    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Write;
        let mut writer = writer;
        while let Some(data) = in_rx.blocking_recv() {
            if writer.write_all(&data).is_err() || writer.flush().is_err() {
                break;
            }
        }
    });

    // Resizes go through a watch rather than a queue: dragging a window edge
    // produces a burst of them and only the last one is worth an ioctl.
    let (size_tx, mut size_rx) = watch::channel((cols, rows));
    let resizer = {
        let master = master.clone();
        tokio::spawn(async move {
            while size_rx.changed().await.is_ok() {
                let (cols, rows) = *size_rx.borrow_and_update();
                let master = master.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = master.lock().unwrap().resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                })
                .await;
            }
        })
    };

    let last_pong = Arc::new(Mutex::new(Instant::now()));

    let mut pump = {
        let pending = pending.clone();
        let woke = woke.clone();
        let last_pong = last_pong.clone();
        tokio::spawn(async move {
            let mut ping = tokio::time::interval(PING_EVERY);
            ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ping.tick().await; // the first tick is immediate

            loop {
                // Everything waiting goes out as one frame. Coalescing matters
                // under load: a build's output arrives in 8 KiB reads and would
                // otherwise become thousands of tiny websocket frames.
                let (batch, dropped, eof) = {
                    let mut p = pending.lock().unwrap();
                    let mut batch = Vec::with_capacity(p.bytes);
                    for chunk in p.chunks.drain(..) {
                        batch.extend_from_slice(&chunk);
                    }
                    p.bytes = 0;
                    (batch, std::mem::take(&mut p.dropped), p.eof)
                };

                if dropped > 0 {
                    let note = format!(
                        "\r\n\x1b[33m[webdesk: {dropped} bytes of output dropped; \
                         the browser could not keep up]\x1b[0m\r\n"
                    );
                    if tx.send(Message::Text(note.into())).await.is_err() {
                        break;
                    }
                }
                if !batch.is_empty() && tx.send(Message::Binary(Bytes::from(batch))).await.is_err()
                {
                    break;
                }

                if eof {
                    // Say why it ended. The browser uses this to tell a shell
                    // that exited -- which should stay ended -- apart from a
                    // connection that dropped, which it should reconnect. The
                    // status rides in the close reason, and NORMAL is reserved
                    // for exactly this case: every other way out of this loop
                    // closes with AWAY, so a proxy that drops the reason string
                    // still leaves the browser able to tell the two apart.
                    let reason = match tokio::time::timeout(Duration::from_secs(2), exit_rx).await {
                        Ok(Ok(Ok(status))) => format!("exit:{}", status.exit_code()),
                        _ => "exit:?".to_string(),
                    };
                    let _ = tx
                        .send(Message::Close(Some(CloseFrame {
                            code: close_code::NORMAL,
                            reason: reason.into(),
                        })))
                        .await;
                    return;
                }

                tokio::select! {
                    _ = woke.notified() => {}
                    _ = ping.tick() => {
                        if last_pong.lock().unwrap().elapsed() > PONG_GRACE {
                            tracing::info!("terminal socket stopped answering; closing");
                            break;
                        }
                        if tx.send(Message::Ping(Bytes::new())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            // Not `close()`, which would say NORMAL: this socket is being given
            // up on -- the browser went away, or stopped answering pings -- and
            // the shell it was attached to did not choose to end.
            let _ = tx
                .send(Message::Close(Some(CloseFrame {
                    code: close_code::AWAY,
                    reason: "".into(),
                })))
                .await;
        })
    };

    loop {
        tokio::select! {
            // The shell is gone, or the socket is. Either way this session is
            // over and the loop must not sit in `rx.next()` waiting for a
            // browser that has nothing left to say.
            _ = &mut pump => break,
            msg = rx.next() => {
                let Some(Ok(msg)) = msg else { break };
                match msg {
                    Message::Binary(data) => {
                        if in_tx.send(data.to_vec()).is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => {
                        if let Ok(Control::Resize { cols, rows }) =
                            serde_json::from_str::<Control>(&text)
                        {
                            let _ = size_tx.send(sane(cols, rows));
                        }
                    }
                    Message::Pong(_) => *last_pong.lock().unwrap() = Instant::now(),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    let _ = killer.kill();
    drop(in_tx);
    resizer.abort();
    pump.abort();
}
