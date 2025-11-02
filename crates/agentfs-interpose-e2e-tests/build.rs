fn main() {
    // Link against CoreFoundation framework on macOS
    // The test_helper binary directly calls CoreFoundation functions
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
