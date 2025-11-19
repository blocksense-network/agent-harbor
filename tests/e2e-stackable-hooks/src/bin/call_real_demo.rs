// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Demo binary that demonstrates call_real! usage from application code

#[allow(clippy::print_stdout, clippy::print_stderr, clippy::disallowed_methods)]
fn main() {
    println!("Call Real Demo - Testing call_real! from application code");

    let mut buffer = [0u8; 10];

    // Test 1: Normal read call (should be hooked and potentially blocked)
    println!("Test 1: Normal read call (may be hooked)");
    let normal_result = unsafe {
        libc::read(
            libc::STDIN_FILENO,
            buffer.as_mut_ptr() as *mut libc::c_void,
            0, // Read 0 bytes to avoid blocking
        )
    };
    println!("Normal read result: {}", normal_result);

    // Test 2: call_real! bypass (should work regardless of hooks)
    println!("Test 2: call_real! bypass");

    // Use dlsym to find the hook infrastructure functions at runtime
    let get_shared_ptr =
        unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"__stackable_get_shared_read".as_ptr()) };
    let call_real_ptr =
        unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"__stackable_call_real_read".as_ptr()) };

    let real_result = if !get_shared_ptr.is_null() && !call_real_ptr.is_null() {
        unsafe {
            type GetSharedFn = extern "C" fn() -> *mut libc::c_void;
            type CallRealFn = extern "C" fn(
                *mut libc::c_void,
                libc::c_int,
                *mut libc::c_void,
                libc::size_t,
            ) -> libc::ssize_t;

            let get_shared: GetSharedFn = std::mem::transmute(get_shared_ptr);
            let call_real: CallRealFn = std::mem::transmute(call_real_ptr);
            let shared = get_shared();
            call_real(
                shared,
                libc::STDIN_FILENO,
                buffer.as_mut_ptr() as *mut libc::c_void,
                0,
            )
        }
    } else {
        // Fallback if functions not found
        unsafe {
            libc::read(
                libc::STDIN_FILENO,
                buffer.as_mut_ptr() as *mut libc::c_void,
                0,
            )
        }
    };

    println!("call_real! read result: {}", real_result);

    // Verify the results
    if normal_result >= 0 && real_result >= 0 {
        println!("[OK] Both calls succeeded");
    } else if normal_result < 0 && real_result >= 0 {
        println!("[OK] call_real! bypassed hook that was blocking the normal call");
    } else {
        println!(
            "[WARN] Unexpected results: normal={}, real={}",
            normal_result, real_result
        );
    }

    println!("Demo completed");
}
