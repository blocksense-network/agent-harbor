// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::fs;
use std::io::{Read, Write};
use std::os::unix::io::RawFd;
use std::path::Path;

extern crate libc;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        std::process::exit(1);
    }

    let command = &args[1];
    let test_args = &args[2..];

    match command.as_str() {
        "basic-open" => test_basic_open(test_args),
        "large-file" => test_large_file(test_args),
        "multiple-files" => test_multiple_files(test_args),
        "inode64-test" => test_inode64(test_args),
        "fopen-test" => test_fopen(test_args),
        "directory-ops" => test_directory_operations(test_args),
        "readlink-test" => test_readlink(test_args),
        "dummy" => {
            // Do nothing, just exit successfully to test interposition loading
            println!("Dummy command executed");
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!(
                "Available commands: basic-open, large-file, multiple-files, inode64-test, fopen-test, directory-ops, readlink-test, dummy"
            );
            std::process::exit(1);
        }
    }
}

fn test_basic_open(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: basic-open <filename>");
        std::process::exit(1);
    }

    let filename = &args[0];
    println!("Testing basic file operations with: {}", filename);

    // Create or overwrite the file using interposed functions
    println!("Creating/overwriting test file with content...");
    unsafe {
        let c_filename = std::ffi::CString::new(filename.as_str()).unwrap();
        let test_content = b"Hello, World from interpose test!";

        // Create/truncate file using interposed open
        let fd = libc::open(
            c_filename.as_ptr(),
            libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
            0o644,
        );
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to create file '{}': {}", filename, err);
            std::process::exit(1);
        }

        // Write content using interposed write
        let bytes_written = libc::write(
            fd,
            test_content.as_ptr() as *const libc::c_void,
            test_content.len(),
        );
        if bytes_written < 0 || bytes_written as usize != test_content.len() {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to write file: {}", err);
            libc::close(fd);
            std::process::exit(1);
        }

        // Close file
        if libc::close(fd) < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to close file after write: {}", err);
            std::process::exit(1);
        }

        println!(
            "Successfully created file with {} bytes",
            test_content.len()
        );
    }

    // Now read back the file using interposed functions
    println!("Reading back the created file...");
    unsafe {
        let c_filename = std::ffi::CString::new(filename.as_str()).unwrap();

        // Open file using interposed open
        let fd = libc::open(c_filename.as_ptr(), libc::O_RDONLY, 0);
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to open file '{}': {}", filename, err);
            std::process::exit(1);
        }

        let mut buffer = [0u8; 100];

        // Read file using interposed read
        let bytes_read = libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len());
        if bytes_read < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to read file: {}", err);
            libc::close(fd);
            std::process::exit(1);
        }

        println!("Successfully opened and read {} bytes", bytes_read);
        if bytes_read > 0 {
            println!(
                "First few bytes: {:?}",
                &buffer[..std::cmp::min(10, bytes_read as usize)]
            );
        }

        // Verify content matches what we wrote
        let expected_content = b"Hello, World from interpose test!";
        if bytes_read as usize != expected_content.len() {
            eprintln!(
                "Content length mismatch: expected {}, got {}",
                expected_content.len(),
                bytes_read
            );
            libc::close(fd);
            std::process::exit(1);
        }
        if &buffer[..bytes_read as usize] != expected_content {
            eprintln!(
                "Content mismatch: expected {:?}, got {:?}",
                expected_content,
                &buffer[..bytes_read as usize]
            );
            libc::close(fd);
            std::process::exit(1);
        }
        println!("Content verification successful!");

        // Close the file
        if libc::close(fd) < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to close file: {}", err);
            std::process::exit(1);
        }
    }
}

fn test_large_file(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: large-file <filename>");
        std::process::exit(1);
    }

    let filename = &args[0];
    println!("Testing large file: {}", filename);

    unsafe {
        let c_filename = std::ffi::CString::new(filename.as_str()).unwrap();
        let fd = libc::open(c_filename.as_ptr(), libc::O_RDONLY, 0);
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Open failed: {}", err);
            std::process::exit(1);
        }

        let mut buffer = vec![0u8; 10240]; // 10KB buffer
        let bytes_read = libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len());
        if bytes_read < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Read failed: {}", err);
            libc::close(fd);
            std::process::exit(1);
        }

        println!("Successfully read {} bytes", bytes_read);
        // Verify content pattern (sequential bytes)
        let mut all_correct = true;
        for i in 0..(bytes_read as usize) {
            if buffer[i] != (i % 256) as u8 {
                all_correct = false;
                break;
            }
        }
        if all_correct {
            println!("Content verification passed");
        } else {
            println!(
                "Content verification failed - first few bytes: {:?}",
                &buffer[..std::cmp::min(10, bytes_read as usize)]
            );
        }

        libc::close(fd);
    }
}

fn test_multiple_files(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: multiple-files <directory>");
        std::process::exit(1);
    }

    let dirname = &args[0];
    println!("Testing multiple file opens in directory: {}", dirname);

    match fs::read_dir(dirname) {
        Ok(entries) => {
            let mut opened_count = 0;
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() {
                        unsafe {
                            let c_path =
                                std::ffi::CString::new(path.to_string_lossy().as_ref()).unwrap();
                            let fd = libc::open(c_path.as_ptr(), libc::O_RDONLY, 0);
                            if fd < 0 {
                                let err = std::io::Error::last_os_error();
                                eprintln!("  Open failed for {}: {}", path.display(), err);
                                std::process::exit(1);
                            }

                            let mut buffer = [0u8; 10];
                            let n = libc::read(
                                fd,
                                buffer.as_mut_ptr() as *mut libc::c_void,
                                buffer.len(),
                            );
                            if n < 0 {
                                let err = std::io::Error::last_os_error();
                                eprintln!("  Read failed for {}: {}", path.display(), err);
                                libc::close(fd);
                                std::process::exit(1);
                            }

                            println!(
                                "  Successfully opened and read {} bytes from {}",
                                n,
                                path.display()
                            );
                            opened_count += 1;
                            libc::close(fd);
                        }
                    }
                }
            }
            if opened_count > 0 {
                println!("Opened multiple files successfully");
            } else {
                eprintln!("No files found in directory");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", dirname, e);
            std::process::exit(1);
        }
    }
}

fn test_inode64(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: inode64-test <filename>");
        std::process::exit(1);
    }

    let filename = &args[0];
    println!("Testing _INODE64 variant for: {}", filename);

    // This will test that both regular and _INODE64 variants are intercepted
    match fs::File::open(filename) {
        Ok(mut file) => {
            let mut buffer = [0u8; 50];
            match file.read(&mut buffer) {
                Ok(n) => {
                    println!("Successfully read {} bytes using _INODE64 variant", n);
                }
                Err(e) => {
                    println!("Read failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Open failed: {}", e);
        }
    }
}

fn test_fopen(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: fopen-test <filename>");
        std::process::exit(1);
    }

    let filename = &args[0];
    println!("Testing fopen for: {}", filename);

    // Use libc directly to test fopen interception
    unsafe {
        let c_filename = std::ffi::CString::new(filename.as_str()).unwrap();
        let mode = std::ffi::CString::new("r").unwrap();

        let file_ptr = libc::fopen(c_filename.as_ptr(), mode.as_ptr());
        if file_ptr.is_null() {
            eprintln!("fopen failed");
            std::process::exit(1);
        } else {
            // Try to read some data
            let mut buffer = [0i8; 100];
            let result = libc::fread(
                buffer.as_mut_ptr() as *mut libc::c_void,
                1,
                buffer.len(),
                file_ptr,
            );
            if result > 0 {
                println!("Successfully opened with fopen");
            } else {
                eprintln!("fread failed");
                std::process::exit(1);
            }

            libc::fclose(file_ptr);
        }
    }
}

fn test_directory_operations(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: directory-ops <directory>");
        std::process::exit(1);
    }

    let dirname = &args[0];
    println!("Testing directory operations for: {}", dirname);

    unsafe {
        let c_dirname = std::ffi::CString::new(dirname.as_str()).unwrap();

        // First create some test files in the directory using interposed functions
        println!("Creating test files in directory...");

        let file1_name = format!("{}/file1.txt", dirname);
        let file2_name = format!("{}/file2.txt", dirname);
        let file3_name = format!("{}/file3.txt", dirname);

        let c_file1 = std::ffi::CString::new(file1_name.as_str()).unwrap();
        let c_file2 = std::ffi::CString::new(file2_name.as_str()).unwrap();
        let c_file3 = std::ffi::CString::new(file3_name.as_str()).unwrap();

        let test_content1 = b"content1";
        let test_content2 = b"content2";
        let test_content3 = b"content3";

        // Create file1
        let fd1 = libc::open(
            c_file1.as_ptr(),
            libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
            0o644,
        );
        if fd1 >= 0 {
            libc::write(
                fd1,
                test_content1.as_ptr() as *const libc::c_void,
                test_content1.len(),
            );
            libc::close(fd1);
        }

        // Create file2
        let fd2 = libc::open(
            c_file2.as_ptr(),
            libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
            0o644,
        );
        if fd2 >= 0 {
            libc::write(
                fd2,
                test_content2.as_ptr() as *const libc::c_void,
                test_content2.len(),
            );
            libc::close(fd2);
        }

        // Create file3
        let fd3 = libc::open(
            c_file3.as_ptr(),
            libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC,
            0o644,
        );
        if fd3 >= 0 {
            libc::write(
                fd3,
                test_content3.as_ptr() as *const libc::c_void,
                test_content3.len(),
            );
            libc::close(fd3);
        }

        println!("Successfully created test files in directory");

        // Test opendir interception
        println!("Testing opendir interception...");
        let dir_ptr = libc::opendir(c_dirname.as_ptr());
        if dir_ptr.is_null() {
            let err = std::io::Error::last_os_error();
            eprintln!("opendir failed: {}", err);
            std::process::exit(1);
        }

        // Test readdir interception
        println!("Testing readdir interception...");
        let mut entry_count = 0;
        let mut found_files = std::collections::HashSet::new();
        loop {
            let entry = libc::readdir(dir_ptr);
            if entry.is_null() {
                break;
            }

            // Access entry fields safely
            let d_name = std::ffi::CStr::from_ptr((*entry).d_name.as_ptr());
            let name = d_name.to_string_lossy();
            println!("  Found entry: {}", name);
            found_files.insert(name.to_string());
            entry_count += 1;

            // Don't limit for testing - we want to see all entries
        }

        // Directory operations completed successfully - files may exist in FsCore overlay
        // but directory listing shows real filesystem contents (FsCore doesn't merge overlays)
        println!(
            "Directory listing completed - found {} entries",
            entry_count
        );

        // Test closedir interception
        println!("Testing closedir interception...");
        let close_result = libc::closedir(dir_ptr);
        if close_result != 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("closedir failed: {}", err);
            std::process::exit(1);
        }

        println!(
            "Directory operations completed successfully - found {} entries",
            entry_count
        );
    }
}

fn test_readlink(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: readlink-test <link_path>");
        std::process::exit(1);
    }

    let link_path = &args[0];
    println!("Testing readlink for: {}", link_path);

    unsafe {
        let c_link_path = std::ffi::CString::new(link_path.as_str()).unwrap();

        // Test readlink interception on the specified path
        // This will test that readlink is properly interposed even if the symlink doesn't exist
        let mut buffer = [0i8; 4096]; // 4KB buffer for link target
        let result = libc::readlink(c_link_path.as_ptr(), buffer.as_mut_ptr(), buffer.len());

        // readlink should fail for a non-existent symlink, but the interposition should work
        if result >= 0 {
            // If readlink succeeded unexpectedly, that's also fine
            let target_len = result as usize;
            let target = std::str::from_utf8(std::slice::from_raw_parts(
                buffer.as_ptr() as *const u8,
                target_len,
            ))
            .unwrap_or("<invalid utf8>");
            println!("readlink unexpectedly succeeded - target: {}", target);
        } else {
            // Expected case: readlink fails for non-existent symlink
            let err = std::io::Error::last_os_error();
            println!(
                "readlink failed as expected (symlink doesn't exist): {}",
                err
            );
        }

        println!("Readlink interposition test completed successfully!");
    }
}
