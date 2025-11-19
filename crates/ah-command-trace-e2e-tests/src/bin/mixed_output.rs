// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::fs::File;
use std::io::{IoSlice, Write};
use std::os::unix::io::{AsRawFd, FromRawFd};

#[allow(clippy::print_stdout, clippy::print_stderr, clippy::disallowed_methods)]
fn main() -> std::io::Result<()> {
    // 1. Write to stdout using write
    println!("Hello stdout from mixed_output");

    // 2. Write to stderr using write
    eprintln!("Hello stderr from mixed_output");

    // 3. Write alternating chunks of varying sizes
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();
    let stderr = std::io::stderr();
    let mut stderr_lock = stderr.lock();

    // 1 byte
    stdout_lock.write_all(b"1")?;
    stdout_lock.flush()?;
    stderr_lock.write_all(b"2")?;
    stderr_lock.flush()?;

    // 4 KiB
    let chunk_4k = vec![b'A'; 4096];
    stdout_lock.write_all(&chunk_4k)?;
    stdout_lock.flush()?;
    let chunk_4k_err = vec![b'B'; 4096];
    stderr_lock.write_all(&chunk_4k_err)?;
    stderr_lock.flush()?;

    // 4. Use writev (via IoSlice)
    let s1 = b"writev";
    let s2 = b" stdout";
    let s3 = b"\n";
    let bufs = [IoSlice::new(s1), IoSlice::new(s2), IoSlice::new(s3)];
    // We can't easily call writev directly safely without libc or specific crates,
    // but std::io::Write::write_vectored uses writev under the hood on Unix.
    let _ = stdout_lock.write_vectored(&bufs)?;
    stdout_lock.flush()?;

    // 5. Duplicate stdout to FD 7 and write
    let stdout_fd = std::io::stdout().as_raw_fd();
    unsafe {
        libc::dup2(stdout_fd, 7);
    }

    let mut fd7 = unsafe { File::from_raw_fd(7) };
    fd7.write_all(b"Writing to FD 7 (dup of stdout)\n")?;
    fd7.flush()?;

    // Close FD 7
    drop(fd7);

    // 6. ANSI control codes and binary data
    stdout_lock.write_all(b"\x1b[31mRed Text\x1b[0m\n")?;
    stdout_lock.write_all(b"\x00\x01\x02\x03 Binary Data \xff\xfe\n")?;
    stdout_lock.flush()?;

    Ok(())
}
