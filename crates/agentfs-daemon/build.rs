fn main() {
    // Link against CoreFoundation framework on macOS for CFMessagePort
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
