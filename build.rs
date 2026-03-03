fn main() {
    // Only build shadow_rs when the feature is enabled
    #[cfg(feature = "shadow_rs")]
    {
        use shadow_rs::ShadowBuilder;
        let _ = ShadowBuilder::builder()
            .deny_const(Default::default())
            .build();
    }

    // Automatically enable platform-specific features
    println!("cargo:rerun-if-changed=build.rs");

    // Enable macOS-specific features
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-cfg=feature=\"macos\"");
    }

    // Enable Windows-specific features
    #[cfg(windows)]
    {
        println!("cargo:rustc-cfg=feature=\"windows\"");
    }

    // Enable Unix-specific features
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        println!("cargo:rustc-cfg=feature=\"unix\"");
    }
}
