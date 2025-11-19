// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use core::cell::Cell;
use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};
use libc::c_char;

#[cfg(target_pointer_width = "64")]
type MachHeader = libc::mach_header_64;

#[cfg(target_pointer_width = "32")]
type MachHeader = libc::mach_header;

extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_header(index: u32) -> *const MachHeader;
    fn NSLookupSymbolInImage(
        image: *const MachHeader,
        symbol_name: *const c_char,
        options: u32,
    ) -> *mut c_void;
    fn NSAddressOfSymbol(symbol: *mut c_void) -> *mut c_void;
}

const NSLOOKUPSYMBOLINIMAGE_OPTION_BIND: u32 = 0x00000001;
const NSLOOKUPSYMBOLINIMAGE_OPTION_RETURN_ON_ERROR: u32 = 0x00000008;

static RUNTIME_READY: AtomicBool = AtomicBool::new(false);

thread_local! {
    static HOOK_DEPTH: Cell<usize> = Cell::new(0);
}

pub fn enable_hooks() {
    RUNTIME_READY.store(true, Ordering::Release);
}

pub fn hooks_enabled() -> bool {
    RUNTIME_READY.load(Ordering::Acquire)
}

#[repr(C)]
pub struct Interpose {
    pub replacement: *const (),
    pub replacee: *const (),
}

unsafe impl Sync for Interpose {}

unsafe fn same_image(a: *const c_void, b: *const c_void) -> bool {
    let mut info_a = MaybeUninit::<libc::Dl_info>::uninit();
    if libc::dladdr(a, info_a.as_mut_ptr()) == 0 {
        return false;
    }
    let info_a = info_a.assume_init();
    let mut info_b = MaybeUninit::<libc::Dl_info>::uninit();
    if libc::dladdr(b, info_b.as_mut_ptr()) == 0 {
        return false;
    }
    let info_b = info_b.assume_init();
    info_a.dli_fbase == info_b.dli_fbase
}

unsafe fn resolve_by_scanning(mach_symbol: &'static str, dispatch: *const c_void) -> *const c_void {
    let count = _dyld_image_count();
    for i in 0..count {
        let header = _dyld_get_image_header(i);
        if header.is_null() {
            continue;
        }
        let symbol = NSLookupSymbolInImage(
            header,
            mach_symbol.as_ptr() as *const c_char,
            NSLOOKUPSYMBOLINIMAGE_OPTION_BIND | NSLOOKUPSYMBOLINIMAGE_OPTION_RETURN_ON_ERROR,
        );
        if symbol.is_null() {
            continue;
        }
        let ptr = NSAddressOfSymbol(symbol);
        if !ptr.is_null() && !same_image(ptr, dispatch) {
            return ptr as *const c_void;
        }
    }
    core::ptr::null()
}

pub unsafe fn resolve_original(
    symbol: &'static str,
    mach_symbol: &'static str,
    dispatch: *const c_void,
) -> *const c_void {
    // Always use resolve_by_scanning to avoid issues with multiple libraries
    // interposing the same function
    let ptr = resolve_by_scanning(mach_symbol, dispatch);
    if ptr.is_null() {
        panic!(
            "stackable-interpose: unable to resolve original symbol {}",
            symbol.trim_end_matches('\0')
        );
    }
    ptr
}

pub unsafe fn canonical_symbol<T>(symbol: &'static str, local: *mut T) -> *mut T {
    let ptr = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr() as *const c_char);
    if ptr.is_null() { local } else { ptr as *mut T }
}

pub struct HookGuard;

impl HookGuard {
    pub fn new() -> Self {
        HOOK_DEPTH.with(|cell| cell.set(cell.get() + 1));
        HookGuard
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

pub fn hooks_allowed() -> bool {
    HOOK_DEPTH.with(|cell| cell.get() == 0)
}

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

#[macro_export]
macro_rules! call_next {
    ($self_expr:expr, $real_fn:ident $(, $args:expr )* $(,)?) => {
        $crate::__stackable_paste! {
            $crate::dyld_insert_libraries::with_reentrancy(|| unsafe {
                [<__stackable_call_next_ $real_fn>](
                    ($self_expr as *mut ::core::ffi::c_void)
                        as *mut [<__StackableNode_ $real_fn>],
                    $($args),*
                )
            })
        }
    };
}

#[macro_export]
macro_rules! call_real {
    ($real_fn:ident $(, $args:expr )* $(,)?) => {
        $crate::__stackable_paste! {
            $crate::dyld_insert_libraries::with_reentrancy(|| unsafe {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                [<__stackable_call_real_ $real_fn>](shared, $($args),*)
            })
        }
    };
}

#[macro_export]
macro_rules! hook {
    (priority: $priority:expr, unsafe fn $real_fn:ident ( $self_ident:ident $(, $v:ident : $t:ty)* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::__stackable_hook_impl!(
            $real_fn,
            $hook_fn,
            $self_ident,
            [ $( ($v : $t) ),* ],
            $r,
            $priority,
            $body
        );
    };

    (priority: $priority:expr, unsafe fn $real_fn:ident ( $self_ident:ident $(, $v:ident : $t:ty)* ) => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: $priority, unsafe fn $real_fn ( $self_ident $(, $v : $t)* ) -> () => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $self_ident:ident $(, $v:ident : $t:ty)* ) -> $r:ty => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: 0, unsafe fn $real_fn ( $self_ident $(, $v : $t)* ) -> $r => $hook_fn $body }
    };

    (unsafe fn $real_fn:ident ( $self_ident:ident $(, $v:ident : $t:ty)* ) => $hook_fn:ident $body:block) => {
        $crate::hook! { priority: 0, unsafe fn $real_fn ( $self_ident $(, $v : $t)* ) -> () => $hook_fn $body }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __stackable_hook_impl {
    ($real_fn:ident, $hook_fn:ident, $self_ident:ident, [ $( ($v:ident : $t:ty) ),* ], $r:ty, $priority:expr, $body:block) => {
        $crate::__stackable_paste! {
            type [<__StackableHookFn_ $real_fn>] =
                unsafe extern "C" fn(*mut [<__StackableNode_ $real_fn>], $($t),*) -> $r;

            #[allow(non_camel_case_types)]
            #[repr(C)]
            struct [<__StackableNode_ $real_fn>] {
                next: *mut [<__StackableNode_ $real_fn>],
                hook: [<__StackableHookFn_ $real_fn>],
                priority: i32,
            }

            #[allow(non_camel_case_types)]
            #[repr(C)]
            struct [<__StackableShared_ $real_fn>] {
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

            extern "C" {
                fn $real_fn($($v : $t),*) -> $r;
            }

            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_get_shared_ $real_fn>]() -> *mut [<__StackableShared_ $real_fn>] {
                static mut CACHED: *mut [<__StackableShared_ $real_fn>] = ::core::ptr::null_mut();
                if !CACHED.is_null() {
                    return CACHED;
                }
                let ptr = $crate::dyld_insert_libraries::canonical_symbol(
                    concat!("__STACKABLE_SHARED_", stringify!($real_fn), "\0"),
                    &mut [<__STACKABLE_SHARED_ $real_fn>] as *mut _,
                );
                CACHED = ptr;
                ptr
            }

            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_call_real_ $real_fn>](
                shared: *mut [<__StackableShared_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                if (*shared).real.is_null() {
                    (*shared).real = $crate::dyld_insert_libraries::resolve_original(
                        concat!(stringify!($real_fn), "\0"),
                        concat!("_", stringify!($real_fn), "\0"),
                        [<__stackable_dispatch_ $real_fn>] as *const () as *const ::core::ffi::c_void,
                    );
                    #[cfg(debug_assertions)]
                    {
                        const PREFIX: &str =
                            concat!("[stackable-interpose] resolved ", stringify!($real_fn), " -> 0x");
                        const DISPATCH_PREFIX: &str =
                            concat!("[stackable-interpose] dispatch  ", stringify!($real_fn), " -> 0x");
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

            #[no_mangle]
            pub unsafe extern "C" fn [<__stackable_dispatch_ $real_fn>]($($v : $t),*) -> $r {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                if !$crate::dyld_insert_libraries::hooks_enabled() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                let head = (*shared).head;
                [<__stackable_call_chain_ $real_fn>](shared, head, $($v),*)
            }

            #[allow(non_snake_case)]
            pub unsafe extern "C" fn [<__stackable_call_next_ $real_fn>](
                node: *mut [<__StackableNode_ $real_fn>],
                $($v : $t),*
            ) -> $r {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                if !$crate::dyld_insert_libraries::hooks_enabled() {
                    return [<__stackable_call_real_ $real_fn>](shared, $($v),*);
                }
                let next = if node.is_null() {
                    (*shared).head
                } else {
                    (*node).next
                };
                [<__stackable_call_chain_ $real_fn>](shared, next, $($v),*)
            }

            unsafe fn [<__stackable_register_node_ $real_fn _ $hook_fn>]() {
                let shared = [<__stackable_get_shared_ $real_fn>]();
                let node = &mut [<__STACKABLE_NODE_ $real_fn _ $hook_fn>] as *mut _;

                let lock_rc = ::libc::pthread_mutex_lock(&mut (*shared).lock as *mut _);
                if lock_rc != 0 {
                    panic!(
                        "stackable-interpose: pthread_mutex_lock failed with code {}",
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
                        "stackable-interpose: pthread_mutex_unlock failed with code {}",
                        unlock_rc
                    );
                }
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
                match ::std::panic::catch_unwind(|| {
                    #[allow(unused_variables)]
                    let $self_ident = node as *mut ::core::ffi::c_void;
                    $body
                }) {
                    Ok(value) => value,
                    Err(_) => {
                        let shared = [<__stackable_get_shared_ $real_fn>]();
                        [<__stackable_call_real_ $real_fn>](shared, $($v),*)
                    }
                }
            }

            #[allow(non_upper_case_globals)]
            #[link_section="__DATA,__interpose"]
            #[used]
            static [<__STACKABLE_INTERPOSE_ $real_fn>]: $crate::dyld_insert_libraries::Interpose =
                $crate::dyld_insert_libraries::Interpose {
                    replacement: [<__stackable_dispatch_ $real_fn>] as *const (),
                    replacee: $real_fn as *const (),
                };
        }
    };
}

#[macro_export]
macro_rules! real {
    ($($tt:tt)*) => {
        compile_error!(
            "stackable_interpose::real!() has been replaced by stackable_interpose::call_next!()"
        );
    };
}
