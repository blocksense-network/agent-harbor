// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use core::cell::Cell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};
use libc::c_char;

#[link(name = "dl")]
extern "C" {
    fn dlsym(handle: *const c_void, symbol: *const c_char) -> *const c_void;
}

const RTLD_NEXT: *const c_void = -1isize as *const c_void;

/// # Safety
///
/// Performs dynamic symbol resolution via `dlsym(RTLD_NEXT, ...)`.
/// The returned pointer may be invalid if the symbol is missing.
pub unsafe fn dlsym_next(symbol: &'static str) -> *const u8 {
    let ptr = dlsym(RTLD_NEXT, symbol.as_ptr() as *const c_char);
    if ptr.is_null() {
        panic!(
            "stackable-hooks: unable to find underlying function for {}",
            symbol
        );
    }
    ptr as *const u8
}

static RUNTIME_READY: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Use const initializer to satisfy clippy::missing_const_for_thread_local
    static HOOK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Enable hooks to start intercepting function calls.
///
/// Hooks are automatically enabled during library initialization, but can
/// be manually controlled as needed.
pub fn enable_hooks() {
    RUNTIME_READY.store(true, Ordering::Release);
}

/// Disable hooks to stop intercepting function calls.
///
/// When hooks are disabled, hooked functions call the original system functions
/// directly without executing any hook logic.
pub fn disable_hooks() {
    RUNTIME_READY.store(false, Ordering::Release);
}

pub fn hooks_enabled() -> bool {
    RUNTIME_READY.load(Ordering::Acquire)
}

/// Returns true when hooks are allowed to run (no active reentrancy guard).
pub fn hooks_allowed() -> bool {
    HOOK_DEPTH.with(|cell| cell.get() == 0)
}

/// Execute `f` with reentrancy temporarily suppressed.
pub fn with_reentrancy<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    HOOK_DEPTH.with(|cell| {
        let current = cell.get();
        if current == 0 {
            f()
        } else {
            cell.set(current - 1);
            let result = f();
            cell.set(current);
            result
        }
    })
}

pub struct HookGuard;

impl HookGuard {
    pub fn new() -> Self {
        HOOK_DEPTH.with(|cell| cell.set(cell.get() + 1));
        HookGuard
    }
}

impl Default for HookGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        HOOK_DEPTH.with(|cell| {
            let current = cell.get();
            if current > 0 {
                cell.set(current - 1);
            }
        });
    }
}

#[ctor::ctor]
fn auto_enable_hooks() {
    enable_hooks();
}

/// # Safety
///
/// Returns a canonical pointer shared across every copy of the library.
pub unsafe fn canonical_symbol<T>(symbol: &'static str, local: *mut T) -> *mut T {
    let ptr = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr() as *const c_char);
    if ptr.is_null() { local } else { ptr as *mut T }
}

/// # Safety
///
/// Resolves the original (uninterposed) symbol.
pub unsafe fn resolve_original(
    symbol: &'static str,
    _mach_symbol: &'static str,
    _dispatch: *const c_void,
) -> *const c_void {
    dlsym_next(symbol) as *const c_void
}

#[macro_export]
macro_rules! call_next {
    ($($args:expr ),* $(,)?) => {
        $crate::ld_preload::with_reentrancy(|| unsafe {
            __stackable_call_next_fn!()(__stackable_current_self!(), $($args),*)
        })
    };
}

#[macro_export]
macro_rules! call_real {
    ($real_fn:ident $(, $args:expr )* $(,)?) => {
        $crate::__stackable_paste! {
            $crate::ld_preload::with_reentrancy(|| unsafe {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                [<__stackable_call_real_ $real_fn>](shared, $($args),*)
            })
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __stackable_export_hook {
    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::__stackable_hook_impl!(
            $real_fn,
            $hook_fn,
            [ $( ($v : $t) ),* ],
            $r,
            $priority,
            $body
        );
    };

    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::__stackable_export_hook! { priority: $priority, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::__stackable_export_hook! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> $r => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::__stackable_export_hook! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __stackable_hook_register {
    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::__stackable_paste! {
            #[allow(non_camel_case_types)]
            #[repr(C)]
            struct [<__StackableNode_ $real_fn>] {
                _private: [u8; 0],
            }

            #[allow(non_camel_case_types)]
            #[repr(C)]
            struct [<__StackableShared_ $real_fn>] {
                _private: [u8; 0],
            }

            impl ::std::panic::RefUnwindSafe for [<__StackableNode_ $real_fn>] {}
            impl ::std::panic::UnwindSafe for [<__StackableNode_ $real_fn>] {}
            impl ::std::panic::RefUnwindSafe for [<__StackableShared_ $real_fn>] {}
            impl ::std::panic::UnwindSafe for [<__StackableShared_ $real_fn>] {}

            extern "C" {
                fn [<__stackable_get_shared_ $real_fn>]() -> *mut [<__StackableShared_ $real_fn>];
                fn [<__stackable_call_real_ $real_fn>](
                    shared: *mut [<__StackableShared_ $real_fn>],
                    $($v : $t),*
                ) -> $r;
                fn [<__stackable_call_next_ $real_fn>](
                    node: *mut [<__StackableNode_ $real_fn>],
                    $($v : $t),*
                ) -> $r;
                fn [<__stackable_register_dynamic_ $real_fn>](
                    priority: i32,
                    hook: unsafe extern "C" fn(*mut [<__StackableNode_ $real_fn>], $($t),*) -> $r,
                ) -> *mut [<__StackableNode_ $real_fn>];
            }

            pub unsafe extern "C" fn $hook_fn(
                node: *mut [<__StackableNode_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                macro_rules! __stackable_current_self {
                    () => { node };
                }
                macro_rules! __stackable_call_next_fn {
                    () => {
                        $crate::__stackable_paste! { [<__stackable_call_next_ $real_fn>] }
                    };
                }
                match ::std::panic::catch_unwind(|| {
                    $body
                }) {
                    Ok(value) => value,
                    Err(_) => {
                        let shared = [<__stackable_get_shared_ $real_fn>]()
                            as *mut [<__StackableShared_ $real_fn>];
                        [<__stackable_call_real_ $real_fn>](shared, $($v),*)
                    }
                }
            }

            #[ctor::ctor]
            fn [<__stackable_attach_ $real_fn _ $hook_fn>]() {
                unsafe {
                    [<__stackable_register_dynamic_ $real_fn>]($priority, $hook_fn);
                }
            }
        }
    };

    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::__stackable_hook_register! { priority: $priority, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::__stackable_hook_register! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> $r => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::__stackable_hook_register! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };
}

#[macro_export]
macro_rules! hook {
    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        #[cfg(stackable_hooks_internal_export)]
        $crate::__stackable_export_hook! {
            priority: $priority,
            unsafe fn $real_fn ( $( $v : $t ),* ) -> $r => $hook_fn $body
        }
        #[cfg(not(stackable_hooks_internal_export))]
        $crate::__stackable_hook_register! {
            priority: $priority,
            unsafe fn $real_fn ( $( $v : $t ),* ) -> $r => $hook_fn $body
        }
    };

    (priority: $priority:expr, unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: $priority, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> $r => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $( $v:ident : $t:ty ),* ) => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: 0, unsafe fn $real_fn ( $( $v : $t ),* ) -> () => $hook_fn $body }
    };
}

#[macro_export]
macro_rules! real {
    ($($tt:tt)*) => {
        compile_error!(
            "stackable_hooks::real!() has been replaced by stackable_hooks::call_next!()"
        );
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __stackable_hook_impl {
    ($real_fn:ident, $hook_fn:ident, [ $( ($v:ident : $t:ty) ),* ], $r:ty, $priority:expr, $body:block) => {
        $crate::__stackable_paste! {
            type [<__StackableHookFn_ $real_fn>] =
                unsafe extern "C" fn(*mut [<__StackableNode_ $real_fn>], $($t),*) -> $r;

            #[allow(non_camel_case_types)]
            #[repr(C)]
            pub struct [<__StackableNode_ $real_fn>] {
                next: *mut [<__StackableNode_ $real_fn>],
                hook: [<__StackableHookFn_ $real_fn>],
                priority: i32,
            }

            type [<__StackableDynHookFn_ $real_fn>] =
                unsafe extern "C" fn(*mut [<__StackableNode_ $real_fn>], $($t),*) -> $r;

            #[allow(non_camel_case_types)]
            #[repr(C)]
            pub struct [<__StackableShared_ $real_fn>] {
                lock: ::libc::pthread_mutex_t,
                head: *mut [<__StackableNode_ $real_fn>],
                real: *const ::core::ffi::c_void,
            }

            impl [<__StackableShared_ $real_fn>] {
                const fn new() -> Self {
                    Self {
                        lock: ::libc::PTHREAD_MUTEX_INITIALIZER,
                        head: ::core::ptr::null_mut(),
                        real: ::core::ptr::null(),
                    }
                }
            }

            #[allow(non_upper_case_globals)]
            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.b.__STACKABLE_SHARED_", stringify!($real_fn))
            )]
            #[no_mangle]
            static mut [<__STACKABLE_SHARED_ $real_fn>]: [<__StackableShared_ $real_fn>] =
                [<__StackableShared_ $real_fn>]::new();

            #[allow(non_upper_case_globals)]
            static mut [<__STACKABLE_NODE_ $real_fn _ $hook_fn>]: [<__StackableNode_ $real_fn>] =
                [<__StackableNode_ $real_fn>] {
                    next: ::core::ptr::null_mut(),
                    hook: $hook_fn,
                    priority: $priority,
                };

            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.t.__stackable_get_shared_", stringify!($real_fn))
            )]
            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_get_shared_ $real_fn>]() -> *mut [<__StackableShared_ $real_fn>] {
                static mut CACHED: *mut [<__StackableShared_ $real_fn>] = ::core::ptr::null_mut();
                if !CACHED.is_null() {
                    return CACHED;
                }
                let ptr = $crate::ld_preload::canonical_symbol(
                    concat!("__STACKABLE_SHARED_", stringify!($real_fn), "\0"),
                    &mut [<__STACKABLE_SHARED_ $real_fn>] as *mut _,
                );
                CACHED = ptr;
                ptr
            }

            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.t.__stackable_call_real_", stringify!($real_fn))
            )]
            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_call_real_ $real_fn>](
                shared: *mut [<__StackableShared_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                if (*shared).real.is_null() {
                    (*shared).real = $crate::ld_preload::resolve_original(
                        concat!(stringify!($real_fn), "\0"),
                        concat!("_", stringify!($real_fn), "\0"),
                        [<__stackable_dispatch_ $real_fn>] as *const () as *const ::core::ffi::c_void,
                    );
                    #[cfg(debug_assertions)]
                    {
                        const PREFIX: &str =
                            concat!("[stackable-hooks] resolved ", stringify!($real_fn), " -> 0x");
                        const DISPATCH_PREFIX: &str =
                            concat!("[stackable-hooks] dispatch  ", stringify!($real_fn), " -> 0x");
                        let mut buf = [0u8; 96];
                        unsafe fn write_hex_prefix(
                            prefix: &str,
                            value: usize,
                            buf: &mut [u8],
                        ) -> usize {
                            let mut idx = 0;
                            let prefix_bytes = prefix.as_bytes();
                            while idx < prefix_bytes.len() {
                                buf[idx] = prefix_bytes[idx];
                                idx += 1;
                            }
                            let mut v = value;
                            if v == 0 {
                                buf[idx] = b'0';
                                idx += 1;
                            } else {
                                let mut digits = [0u8; ::core::mem::size_of::<usize>() * 2];
                                let mut d_idx = digits.len();
                                while v > 0 {
                                    let nibble = (v & 0xF) as u8;
                                    d_idx -= 1;
                                    digits[d_idx] = match nibble {
                                        0..=9 => b'0' + nibble,
                                        _ => b'a' + (nibble - 10),
                                    };
                                    v >>= 4;
                                }
                                while d_idx < digits.len() {
                                    buf[idx] = digits[d_idx];
                                    idx += 1;
                                    d_idx += 1;
                                }
                            }
                            buf[idx] = b'\n';
                            idx + 1
                        }
                        let len = write_hex_prefix(
                            PREFIX,
                            (*shared).real as usize,
                            &mut buf,
                        );
                        libc::write(
                            libc::STDERR_FILENO,
                            buf.as_ptr() as *const libc::c_void,
                            len,
                        );
                        let dispatch_len = write_hex_prefix(
                            DISPATCH_PREFIX,
                            [<__stackable_dispatch_ $real_fn>] as usize,
                            &mut buf,
                        );
                        libc::write(
                            libc::STDERR_FILENO,
                            buf.as_ptr() as *const libc::c_void,
                            dispatch_len,
                        );
                    }
                }
                let real_fn: unsafe extern "C" fn($($t),*) -> $r =
                    ::core::mem::transmute((*shared).real);
                real_fn($($v),*)
            }

            #[allow(clippy::too_many_arguments)]
            unsafe fn [<__stackable_call_chain_ $real_fn>](
                shared: *mut [<__StackableShared_ $real_fn>],
                start: *mut [<__StackableNode_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                if start.is_null() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                ((*start).hook)(start, $($v),*)
            }

            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.t.__stackable_dispatch_", stringify!($real_fn))
            )]
            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_dispatch_ $real_fn>]($($v : $t),*) -> $r {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                if !$crate::ld_preload::hooks_enabled() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                let head = (*shared).head;
                if head.is_null() || !$crate::hooks_allowed() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                let _stackable_guard = $crate::HookGuard::new();
                [<__stackable_call_chain_ $real_fn>](shared, head, $($v),*)
            }

            #[allow(non_snake_case)]
            #[allow(clippy::too_many_arguments)]
            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_call_next_ $real_fn>](
                node: *mut [<__StackableNode_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                if !$crate::ld_preload::hooks_enabled() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                let next = if node.is_null() {
                    (*shared).head
                } else {
                    (*node).next
                };
                [<__stackable_call_chain_ $real_fn>](shared, next, $($v),*)
            }

            unsafe fn [<__stackable_insert_node_ $real_fn>](
                shared: *mut [<__StackableShared_ $real_fn>],
                node: *mut [<__StackableNode_ $real_fn>],
            ) {
                let lock_rc = ::libc::pthread_mutex_lock(&mut (*shared).lock as *mut _);
                if lock_rc != 0 {
                    panic!(
                        "stackable-hooks: pthread_mutex_lock failed with code {}",
                        lock_rc
                    );
                }

                if (*shared).head.is_null() {
                    (*shared).head = node;
                } else {
                    // Insert in priority order (lower priority numbers = higher priority)
                    let node_priority = (*node).priority;
                    if node_priority <= (*(*shared).head).priority {
                        // Insert at head
                        (*node).next = (*shared).head;
                        (*shared).head = node;
                    } else {
                        // Find insertion point
                        let mut current = (*shared).head;
                        while !(*current).next.is_null() && (*(*current).next).priority < node_priority {
                            current = (*current).next;
                        }
                        (*node).next = (*current).next;
                        (*current).next = node;
                    }
                }

                let unlock_rc =
                    ::libc::pthread_mutex_unlock(&mut (*shared).lock as *mut _);
                if unlock_rc != 0 {
                    panic!(
                        "stackable-hooks: pthread_mutex_unlock failed with code {}",
                        unlock_rc
                    );
                }
            }

            unsafe fn [<__stackable_register_node_ $real_fn _ $hook_fn>]() {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                let node = &mut [<__STACKABLE_NODE_ $real_fn _ $hook_fn>] as *mut _;
                [<__stackable_insert_node_ $real_fn>](shared, node);
            }

            #[ctor::ctor]
            fn [<__stackable_register_ $real_fn _ $hook_fn>]() {
                unsafe {
                    [<__stackable_register_node_ $real_fn _ $hook_fn>]();
                }
            }

            pub unsafe extern "C" fn $hook_fn(
                node: *mut [<__StackableNode_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                macro_rules! __stackable_current_self {
                    () => { node };
                }
                macro_rules! __stackable_call_next_fn {
                    () => {
                        $crate::__stackable_paste! { [<__stackable_call_next_ $real_fn>] }
                    };
                }
                match ::std::panic::catch_unwind(|| {
                    $body
                }) {
                    Ok(value) => value,
                    Err(_) => {
                        let shared = [<__stackable_get_shared_ $real_fn>]()
                            as *mut [<__StackableShared_ $real_fn>];
                        [<__stackable_call_real_ $real_fn>](shared, $($v),*)
                    }
                }
            }

            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.t.", stringify!($real_fn))
            )]
            #[no_mangle]
            pub unsafe extern "C" fn $real_fn($($v : $t),*) -> $r {
                [<__stackable_dispatch_ $real_fn>]($($v),*)
            }

            #[cfg_attr(
                target_env = "gnu",
                link_section = concat!(".gnu.linkonce.t.__stackable_register_dynamic_", stringify!($real_fn))
            )]
            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_register_dynamic_ $real_fn>](
                priority: i32,
                hook: [<__StackableDynHookFn_ $real_fn>],
            ) -> *mut [<__StackableNode_ $real_fn>] {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                let node = ::std::boxed::Box::into_raw(::std::boxed::Box::new(
                    [<__StackableNode_ $real_fn>] {
                        next: ::core::ptr::null_mut(),
                        hook,
                        priority,
                    },
                ));
                [<__stackable_insert_node_ $real_fn>](shared, node);
                node
            }
        }
    };
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod stub_dispatchers {
    #![allow(non_camel_case_types, non_snake_case, static_mut_refs)]

    const STUB_PRIORITY: i32 = i32::MAX;

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn getpid() -> libc::pid_t => __stackable_stub_getpid {
            call_next!()
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn fork() -> libc::pid_t => __stackable_stub_fork {
            call_next!()
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn clone(
            fn_: extern "C" fn(*mut libc::c_void) -> libc::c_int,
            child_stack: *mut libc::c_void,
            flags: libc::c_int,
            arg: *mut libc::c_void,
            ptid: *mut libc::pid_t,
            tls: *mut libc::c_void,
            ctid: *mut libc::pid_t
        ) -> libc::c_int => __stackable_stub_clone {
            call_next!(fn_, child_stack, flags, arg, ptid, tls, ctid)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn write(
            fd: libc::c_int,
            buf: *const libc::c_void,
            count: libc::size_t
        ) -> libc::ssize_t => __stackable_stub_write {
            call_next!(fd, buf, count)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn writev(
            fd: libc::c_int,
            iov: *const libc::iovec,
            iovcnt: libc::c_int
        ) -> libc::ssize_t => __stackable_stub_writev {
            call_next!(fd, iov, iovcnt)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn dup(oldfd: libc::c_int) -> libc::c_int => __stackable_stub_dup {
            call_next!(oldfd)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn dup2(oldfd: libc::c_int, newfd: libc::c_int) -> libc::c_int
            => __stackable_stub_dup2 {
            call_next!(oldfd, newfd)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn dup3(
            oldfd: libc::c_int,
            newfd: libc::c_int,
            flags: libc::c_int
        ) -> libc::c_int => __stackable_stub_dup3 {
            call_next!(oldfd, newfd, flags)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn close(fd: libc::c_int) -> libc::c_int => __stackable_stub_close {
            call_next!(fd)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn fcntl(fd: libc::c_int, cmd: libc::c_int, arg: libc::c_int) -> libc::c_int
            => __stackable_stub_fcntl {
            call_next!(fd, cmd, arg)
        }
    }

    crate::hook! {
        priority: STUB_PRIORITY,
        unsafe fn sendmsg(
            fd: libc::c_int,
            msg: *const libc::msghdr,
            flags: libc::c_int
        ) -> libc::ssize_t => __stackable_stub_sendmsg {
            call_next!(fd, msg, flags)
        }
    }
}
