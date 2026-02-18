fn main() {
    // Only build shadow_rs when the feature is enabled
    #[cfg(feature = "shadow_rs")]
    {
        use shadow_rs::ShadowBuilder;
        let _ = ShadowBuilder::builder()
            .deny_const(Default::default())
            .build();
    }
}
