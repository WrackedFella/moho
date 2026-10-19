//! Every WGSL shader the renderer loads parses and validates with naga.

use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::path::Path;

fn validate(label: &str, source: &str, capabilities: Capabilities) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| {
        panic!(
            "WGSL parse failed for {label}:\n{}",
            e.emit_to_string(source)
        )
    });

    Validator::new(ValidationFlags::all(), capabilities)
        .validate(&module)
        .unwrap_or_else(|e| panic!("WGSL validation failed for {label}: {e:?}"));

    module
}

fn validate_file(name: &str, capabilities: Capabilities) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../shaders")
        .join(name);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    validate(name, &source, capabilities);
}

#[test]
fn main_shader_source_parses_and_validates() {
    let source = moho_renderer::pipeline::main_shader_source();

    let module = validate("main shader", &source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(entry_points.contains(&"vs_main"), "{entry_points:?}");
    assert!(entry_points.contains(&"fs_main"), "{entry_points:?}");
}

#[test]
fn skybox_shader_parses_and_validates() {
    validate_file("skybox.wgsl", Capabilities::empty());
}

#[test]
fn shadow_shader_parses_and_validates() {
    validate_file("shadow.wgsl", Capabilities::IMMEDIATES);
}

#[test]
fn shadow_shader_has_a_fragment_stage() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/shadow.wgsl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    let module = validate("shadow.wgsl", &source, Capabilities::IMMEDIATES);

    assert!(
        module
            .entry_points
            .iter()
            .any(|e| e.name == "fs_main" && e.stage == naga::ShaderStage::Fragment),
        "fs_main fragment entry point missing"
    );
}

#[test]
fn gtao_shader_parses_and_validates() {
    validate_file("gtao.wgsl", Capabilities::empty());
}

#[test]
fn ssao_blur_shader_parses_and_validates() {
    validate_file("ssao_blur.wgsl", Capabilities::empty());
}

#[test]
fn ui_shader_parses_and_validates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/ui.wgsl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    let module = validate("ui.wgsl", &source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    for expected in [
        "vs_main",
        "fs_main_linear_framebuffer",
        "fs_main_gamma_framebuffer",
    ] {
        assert!(
            entry_points.contains(&expected),
            "{expected} missing: {entry_points:?}"
        );
    }
}

#[test]
fn marker_shader_parses_and_validates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/marker.wgsl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    let module = validate("marker.wgsl", &source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["vs_main", "fs_main"] {
        assert!(
            entry_points.contains(&expected),
            "{expected} missing: {entry_points:?}"
        );
    }
}
