pub fn fixture_root() -> std::path::PathBuf {
    // Tests assume chart lives under appengine/charts/control-plane
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().expect("crate has parent")
        .join("..")
        .canonicalize().expect("canonicalize workspace");
    // go up to appengine
    root
}
