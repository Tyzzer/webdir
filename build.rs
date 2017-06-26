fn main() {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    println!("cargo:rustc-cfg=apple");

    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    println!("cargo:rustc-cfg=freebsdlike");
}
