// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Helper binary for passthrough recorder tests.
//! Listens on a Unix control socket (path from CONTROL_SOCKET env).
//! Commands (newline-delimited) supported:
//! - OUT <text>
//! - ERR <text>
//! - SLEEP <ms>
//! - ECHO_INPUT (reads one line from stdin, echoes to stdout as `ECHO:<line>`)
//! - PRINT_SIZE (PTY only: prints current winsize as `SIZE <cols> <rows>`)
//! - EXIT

use std::{
    io::{BufRead, Read, Write},
    os::unix::net::UnixListener,
    time::Duration,
};

fn main() -> std::io::Result<()> {
    let control_path =
        std::env::var("CONTROL_SOCKET").expect("CONTROL_SOCKET must be set by the test harness");

    let listener = UnixListener::bind(control_path)?;
    for stream in listener.incoming() {
        let mut ctrl = stream?;
        let mut line = String::new();
        ctrl.read_to_string(&mut line)?;
        for raw in line.split('\n') {
            if raw.is_empty() {
                continue;
            }
            let mut parts = raw.splitn(2, ' ');
            let cmd = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("");
            match cmd {
                "OUT" => {
                    let _ = writeln!(std::io::stdout(), "{rest}");
                    let _ = writeln!(std::io::stdout(), "ACK {rest}");
                    std::io::stdout().flush().ok();
                }
                "ERR" => {
                    let _ = writeln!(std::io::stderr(), "{rest}");
                    std::io::stderr().flush().ok();
                }
                "SLEEP" => {
                    if let Ok(ms) = rest.trim().parse::<u64>() {
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                }
                "ECHO_INPUT" => {
                    let stdin = std::io::stdin();
                    let mut locked = stdin.lock();
                    let mut buf = String::new();
                    if locked.read_line(&mut buf).is_ok() {
                        let buf = buf.trim_end_matches(&['\n', '\r'][..]);
                        let _ = writeln!(std::io::stdout(), "ECHO:{buf}");
                        std::io::stdout().flush().ok();
                    }
                }
                "PRINT_SIZE" => {
                    if let Some((cols, rows)) = current_winsize() {
                        let _ = writeln!(std::io::stdout(), "SIZE {cols} {rows}");
                        std::io::stdout().flush().ok();
                    }
                }
                "EXIT" => return Ok(()),
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn current_winsize() -> Option<(u16, u16)> {
    use libc::{TIOCGWINSZ, ioctl, winsize};
    let mut ws = winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fd = 0; // stdin
    let res = unsafe { ioctl(fd, TIOCGWINSZ, &mut ws) };
    if res == 0 {
        Some((ws.ws_col, ws.ws_row))
    } else {
        None
    }
}
