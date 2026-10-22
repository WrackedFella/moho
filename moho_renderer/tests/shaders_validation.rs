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

/// The WGSL type a shader input must have to read an attribute of `format`.
fn wgsl_type_of(format: wgpu::VertexFormat) -> naga::TypeInner {
    use naga::{Scalar, TypeInner, VectorSize};
    match format {
        wgpu::VertexFormat::Float32 => TypeInner::Scalar(Scalar::F32),
        wgpu::VertexFormat::Uint32 => TypeInner::Scalar(Scalar::U32),
        wgpu::VertexFormat::Float32x3 => TypeInner::Vector {
            size: VectorSize::Tri,
            scalar: Scalar::F32,
        },
        wgpu::VertexFormat::Float32x4 => TypeInner::Vector {
            size: VectorSize::Quad,
            scalar: Scalar::F32,
        },
        other => panic!("no WGSL type mapped for {other:?}"),
    }
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
    let mut shader_inputs = Vec::new();
    for argument in &entry.function.arguments {
        match &argument.binding {
            Some(naga::Binding::Location { location, .. }) => {
                shader_inputs.push((*location, module.types[argument.ty].inner.clone()));
            }
            Some(naga::Binding::BuiltIn(_)) => {}
            None => {
                let naga::TypeInner::Struct { members, .. } = &module.types[argument.ty].inner
                else {
                    panic!("vs_main argument without a binding is not a struct");
                };
                for member in members {
                    if let Some(naga::Binding::Location { location, .. }) = &member.binding {
                        shader_inputs.push((*location, module.types[member.ty].inner.clone()));
                    }
                }
            }
        }
    }
    shader_inputs.sort_by_key(|(location, _)| *location);
    let mut pipeline_inputs: Vec<(u32, naga::TypeInner)> =
        moho_renderer::pipeline::MAIN_VERTEX_ATTRIBUTES
            .iter()
            .chain(moho_renderer::pipeline::MAIN_INSTANCE_ATTRIBUTES.iter())
            .map(|a| (a.shader_location, wgsl_type_of(a.format)))
            .collect();
    pipeline_inputs.sort_by_key(|(location, _)| *location);

    assert_eq!(
        shader_inputs, pipeline_inputs,
        "locations and types of vs_main's inputs"
    );
    assert!(
        shader_inputs.contains(&(13, naga::TypeInner::Scalar(naga::Scalar::U32))),
        "the per-vertex material is a u32 at location 13"
    );
}

/// Offsets and total size of `Lighting` as naga lays it out in `source`.
fn lighting_layout(label: &str, source: &str) -> (Vec<(String, u32)>, u32) {
    let module = validate(label, source, Capabilities::empty());
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, t)| t.name.as_deref() == Some("Lighting"))
        .unwrap_or_else(|| panic!("{label} declares struct Lighting"));
    let naga::TypeInner::Struct { members, span } = &ty.inner else {
        panic!("{label}: Lighting is not a struct");
    };
    let members = members
        .iter()
        .map(|m| (m.name.clone().unwrap_or_default(), m.offset))
        .collect();
    (members, *span)
}

#[test]
fn lighting_struct_matches_lighting_gpu_in_every_shader() {
    use moho_renderer::LightingGpu;
    use std::mem::{offset_of, size_of};

    let skybox_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/skybox.wgsl");
    let skybox = std::fs::read_to_string(&skybox_path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", skybox_path.display()));
    let main = moho_renderer::pipeline::main_shader_source();
    let expected_tail = [
        ("sky_zenith", offset_of!(LightingGpu, sky_zenith)),
        ("sky_horizon", offset_of!(LightingGpu, sky_horizon)),
        ("sky_ground", offset_of!(LightingGpu, sky_ground)),
    ];

    for (label, source) in [("main shader", main.as_str()), ("skybox.wgsl", &skybox)] {
        let (members, span) = lighting_layout(label, source);

        assert_eq!(span as usize, size_of::<LightingGpu>(), "{label}: span");
        assert_eq!(members.len(), 9, "{label}: member count {members:?}");
        let offsets: Vec<u32> = members.iter().map(|(_, o)| *o).collect();
        assert_eq!(
            offsets,
            (0..9).map(|i| i * 16).collect::<Vec<u32>>(),
            "{label}: one vec4 per member"
        );
        assert_eq!(
            offsets[5] as usize,
            offset_of!(LightingGpu, params),
            "{label}: params offset"
        );
        for (i, (name, offset)) in expected_tail.iter().enumerate() {
            let (member_name, member_offset) = &members[6 + i];
            assert_eq!(member_name, name, "{label}: member {}", 6 + i);
            assert_eq!(*member_offset as usize, *offset, "{label}: {name} offset");
        }
    }
}
