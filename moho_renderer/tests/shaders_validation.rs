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

#[test]
fn marker_shader_declares_the_inflate_override() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/marker.wgsl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    let module = validate("marker.wgsl", &source, Capabilities::empty());

    let (handle, declared) = module
        .overrides
        .iter()
        .find(|(_, o)| o.name.as_deref() == Some("marker_inflate"))
        .expect("marker.wgsl declares `override marker_inflate`");
    assert_eq!(
        module.types[declared.ty].inner,
        naga::TypeInner::Scalar(naga::Scalar::F32),
        "marker_inflate is an f32"
    );
    assert!(declared.init.is_none(), "the pipeline supplies the value");
    let fragment = module
        .entry_points
        .iter()
        .find(|e| e.name == "fs_main")
        .expect("fs_main");
    assert!(
        fragment
            .function
            .expressions
            .iter()
            .any(|(_, e)| matches!(e, naga::Expression::Override(h) if *h == handle)),
        "the fragment discard reads marker_inflate"
    );
}

#[test]
fn vertex_input_locations_match_the_main_pipeline_layout() {
    let source = moho_renderer::pipeline::main_shader_source();
    let module = validate("main shader", &source, Capabilities::empty());
    let entry = module
        .entry_points
        .iter()
        .find(|e| e.name == "vs_main")
        .expect("vs_main");
    let mut shader_locations = Vec::new();
    for argument in &entry.function.arguments {
        match &argument.binding {
            Some(naga::Binding::Location { location, .. }) => shader_locations.push(*location),
            Some(naga::Binding::BuiltIn(_)) => {}
            None => {
                let naga::TypeInner::Struct { members, .. } = &module.types[argument.ty].inner
                else {
                    panic!("vs_main argument without a binding is not a struct");
                };
                for member in members {
                    if let Some(naga::Binding::Location { location, .. }) = &member.binding {
                        shader_locations.push(*location);
                    }
                }
            }
        }
    }
    shader_locations.sort_unstable();
    let mut pipeline_locations: Vec<u32> = moho_renderer::pipeline::MAIN_VERTEX_ATTRIBUTES
        .iter()
        .chain(moho_renderer::pipeline::MAIN_INSTANCE_ATTRIBUTES.iter())
        .map(|a| a.shader_location)
        .collect();
    pipeline_locations.sort_unstable();

    assert_eq!(shader_locations, pipeline_locations);
    assert!(
        shader_locations.contains(&13),
        "the per-vertex material is read at location 13"
    );
}
