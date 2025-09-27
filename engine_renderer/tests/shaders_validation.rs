use std::fs;

// This test ensures WGSL shaders in the repository parse and validate with naga.
// It runs as part of `cargo test` for the engine_renderer crate.

#[test]
fn validate_wgsl_shaders() {
    // List of shader files to validate. Add new WGSL files here as needed.
    let shaders = ["../shaders/instance.wgsl"];

    for rel in &shaders {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read shader {}: {}", path.display(), e));

        // Parse WGSL
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|_| panic!("WGSL parse failed for {}", path.display()));

        // Validate
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|_| panic!("WGSL validation failed for {}", path.display()));
    }
}
