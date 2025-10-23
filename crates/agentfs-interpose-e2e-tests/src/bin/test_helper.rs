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
        "dummy" => {
            // Do nothing, just exit successfully to test interposition loading
            println!("Dummy command executed");
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Available commands: basic-open, large-file, multiple-files, inode64-test, fopen-test, dummy");
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
    println!("Testing basic open of: {}", filename);

    // Use dlsym to get open function dynamically to ensure interposition works
    unsafe {
        let c_filename = std::ffi::CString::new(filename.as_str()).unwrap();

        // Use dlsym to dynamically resolve open function
        let open_func: Option<unsafe extern "C" fn(*const libc::c_char, libc::c_int, libc::mode_t) -> libc::c_int> =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, b"open\0".as_ptr() as *const libc::c_char));

        let fd = if let Some(open_func) = open_func {
            open_func(c_filename.as_ptr(), libc::O_RDONLY, 0)
        } else {
            libc::open(c_filename.as_ptr(), libc::O_RDONLY, 0)
        };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to open file '{}': {}", filename, err);
            std::process::exit(1);
        }

        let mut buffer = [0u8; 100];

        // Use dlsym for read as well
        let read_func: Option<unsafe extern "C" fn(libc::c_int, *mut libc::c_void, libc::size_t) -> libc::ssize_t> =
            std::mem::transmute(libc::dlsym(libc::RTLD_DEFAULT, b"read\0".as_ptr() as *const libc::c_char));

        let bytes_read = if let Some(read_func) = read_func {
            read_func(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
        } else {
            libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len())
        };
        if bytes_read < 0 {
            let err = std::io::Error::last_os_error();
            eprintln!("Failed to read file: {}", err);
            libc::close(fd);
            std::process::exit(1);
        }

        println!("Successfully opened and read {} bytes", bytes_read);
        if bytes_read > 0 {
            println!("First few bytes: {:?}", &buffer[..std::cmp::min(10, bytes_read as usize)]);
        }

        libc::close(fd);
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
            println!("Content verification failed - first few bytes: {:?}", &buffer[..std::cmp::min(10, bytes_read as usize)]);
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
                            let c_path = std::ffi::CString::new(path.to_string_lossy().as_ref()).unwrap();
                            let fd = libc::open(c_path.as_ptr(), libc::O_RDONLY, 0);
                            if fd < 0 {
                                let err = std::io::Error::last_os_error();
                                eprintln!("  Open failed for {}: {}", path.display(), err);
                                std::process::exit(1);
                            }

                            let mut buffer = [0u8; 10];
                            let n = libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len());
                            if n < 0 {
                                let err = std::io::Error::last_os_error();
                                eprintln!("  Read failed for {}: {}", path.display(), err);
                                libc::close(fd);
                                std::process::exit(1);
                            }

                            println!("  Successfully opened and read {} bytes from {}", n, path.display());
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
            let result = libc::fread(buffer.as_mut_ptr() as *mut libc::c_void, 1, buffer.len(), file_ptr);
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
