fn main() {
    // GPUI's composed render trees need more than Windows' default 1 MiB,
    // particularly in unoptimized development builds. Keep the UI on the
    // main thread and reserve stack space without committing it all up front.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=kernelledger-desktop=/STACK:16777216");
    }
}
