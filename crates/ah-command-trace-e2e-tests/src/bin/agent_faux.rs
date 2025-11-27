// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//
// A lightweight faux agent that spawns a handful of subprocesses using
// different launch paths (direct exec, shell pipeline, Python + subprocess)
// so integration tests can verify the command-trace shim observes all of them.

use std::process::Command;

fn main() {
    // 1) Direct child
    let _ = Command::new("echo").arg("direct child out").status();

    // 2) Shell pipeline (stdout)
    let _ = Command::new("sh").arg("-c").arg("echo shell pipeline | tr a-z A-Z").status();

    // 3) Shell pipeline with stderr
    let _ = Command::new("sh").arg("-c").arg("echo shell stderr 1>&2").status();

    // 4) Python spawning helpers
    let py_script = r#"
import subprocess, sys, os
subprocess.run(["echo", "python child out"])
subprocess.run(["sh", "-c", "echo python shell via sh"])
sys.stderr.write("python stderr line\n")
"#;
    let _ = Command::new("python3").arg("-c").arg(py_script).status();

    // 5) Another direct command to ensure trailing events flush
    let _ = Command::new("printf").arg("trailing child\n").status();
}
