// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FFI — C ABI for FSKit and other integrations
//!
//! This crate provides a C-compatible ABI for integrating AgentFS
//! with platform-specific filesystem frameworks like FSKit (macOS).

pub mod c_api;

#[cfg(test)]
mod tests {
    use super::c_api::*;
    use std::ffi::CString;

    fn create_fs() -> u64 {
        let cfg = CString::new("{\"max_memory_bytes\": 1048576, \"max_open_handles\": 128, \"max_branches\": 8, \"max_snapshots\": 8}").unwrap();
        let mut fs: u64 = 0;
        let rc = unsafe { af_fs_create(cfg.as_ptr(), &mut fs as *mut u64) } as i32;
        assert_eq!(rc, 0);
        fs
    }

    #[test]
    fn test_chown_and_non_utf8_create() {
        let fs = create_fs();
        // mkdir /raw
        let d = CString::new("/raw").unwrap();
        let rc = unsafe { af_mkdir(fs, 0, d.as_ptr(), 0o755) } as i32;
        assert_eq!(rc, 0);

        // Resolve parent id
        let mut nid: u64 = 0;
        let mut pid: u64 = 0;
        let rc = unsafe {
            af_resolve_id(
                fs,
                0,
                d.as_ptr(),
                &mut nid as *mut u64,
                &mut pid as *mut u64,
            )
        } as i32;
        assert_eq!(rc, 0);

        // Create child by id with invalid UTF-8 name bytes
        let name_bytes = [0x66u8, 0x6F, 0x80, 0x6F];
        let mut child_id: u64 = 0;
        let rc = unsafe {
            af_create_child_by_id(
                fs,
                nid,
                name_bytes.as_ptr(),
                name_bytes.len(),
                0,
                0o644,
                &mut child_id as *mut u64,
            )
        } as i32;
        assert_eq!(rc, 0);
        assert!(child_id > 0);

        // Open by id
        let opts = CString::new("{\"read\":true,\"write\":true}").unwrap();
        let mut h: u64 = 0;
        let rc =
            unsafe { af_open_by_id(fs, 0, child_id, opts.as_ptr(), &mut h as *mut u64) } as i32;
        assert_eq!(rc, 0);
        unsafe { af_close(fs, 0, h) };

        // Set owner on path (percent-encoded internal path is not public; chown via path of parent not tested here)
    }

    #[test]
    fn test_basic_readdir_and_attrs() {
        let fs = create_fs();
        let path = CString::new("/").unwrap();
        let mut buf = vec![0u8; 4096];
        let mut out_len: usize = 0;
        let rc = unsafe {
            af_readdir(
                fs,
                0,
                path.as_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len as *mut usize,
            )
        } as i32;
        assert_eq!(rc, 0);

        // getattr root
        let mut abuf = vec![0u8; 64];
        let rc = unsafe { af_getattr(fs, 0, path.as_ptr(), abuf.as_mut_ptr(), abuf.len()) } as i32;
        assert_eq!(rc, 0);
    }

    #[test]
    fn test_set_mode_times_rename_and_xattr() {
        let fs = create_fs();
        let fname = CString::new("/file").unwrap();
        // Create via open(create=true)
        let opts = CString::new("{\"read\":true,\"write\":true,\"create\":true,\"truncate\":true}")
            .unwrap();
        let mut h: u64 = 0;
        let rc =
            unsafe { af_open(fs, 0, fname.as_ptr(), opts.as_ptr(), &mut h as *mut u64) } as i32;
        assert_eq!(rc, 0);
        unsafe { af_close(fs, 0, h) };

        // chmod
        let rc = unsafe { af_set_mode(fs, 0, fname.as_ptr(), 0o600) } as i32;
        assert_eq!(rc, 0);
        // utimens
        let rc = unsafe { af_set_times(fs, 0, fname.as_ptr(), 10, 10, 10, 1) } as i32;
        assert_eq!(rc, 0);

        // xattr set/get/list
        let key = CString::new("user.test").unwrap();
        let val = b"value";
        let rc =
            unsafe { af_xattr_set(fs, 0, fname.as_ptr(), key.as_ptr(), val.as_ptr(), val.len()) }
                as i32;
        assert_eq!(rc, 0);
        let mut vbuf = vec![0u8; 32];
        let mut vlen: usize = 0;
        let rc = unsafe {
            af_xattr_get(
                fs,
                0,
                fname.as_ptr(),
                key.as_ptr(),
                vbuf.as_mut_ptr(),
                vbuf.len(),
                &mut vlen as *mut usize,
            )
        } as i32;
        assert_eq!(rc, 0);
        assert_eq!(&vbuf[..vlen], val);
        let mut lbuf = vec![0u8; 64];
        let mut llen: usize = 0;
        let rc = unsafe {
            af_xattr_list(
                fs,
                0,
                fname.as_ptr(),
                lbuf.as_mut_ptr(),
                lbuf.len(),
                &mut llen as *mut usize,
            )
        } as i32;
        assert_eq!(rc, 0);

        // rename
        let newname = CString::new("/file2").unwrap();
        let rc = unsafe { af_rename(fs, 0, fname.as_ptr(), newname.as_ptr()) } as i32;
        assert_eq!(rc, 0);
    }
}

// Re-export C API functions
pub use c_api::*;
