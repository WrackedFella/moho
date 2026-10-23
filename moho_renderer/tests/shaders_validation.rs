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
fn skybox_shader_source_parses_and_validates() {
    let source = moho_renderer::pipeline::skybox_shader_source();

    let module = validate("skybox shader", &source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(entry_points.contains(&"vs_main"), "{entry_points:?}");
    assert!(entry_points.contains(&"fs_main"), "{entry_points:?}");
}

#[test]
fn skybox_and_main_shader_share_sky_gradient() {
    let skybox = moho_renderer::pipeline::skybox_shader_source();
    let main = moho_renderer::pipeline::main_shader_source();

    for (label, source) in [("skybox", &skybox), ("main", &main)] {
        assert!(source.contains("sky_gradient"), "{label} shader");
    }

    let module = validate("skybox shader", &skybox, Capabilities::empty());
    let gradients: Vec<_> = module
        .functions
        .iter()
        .filter(|(_, f)| f.name.as_deref() == Some("sky_gradient"))
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(gradients.len(), 1, "sky_gradient defined once");
    let fs_main = module
        .entry_points
        .iter()
        .find(|e| e.name == "fs_main")
        .expect("skybox fs_main");
    let calls_gradient = fs_main.function.body.iter().any(|statement| {
        matches!(statement, naga::Statement::Call { function, .. } if *function == gradients[0])
    });
    assert!(calls_gradient, "skybox fs_main calls sky_gradient");
}

#[test]
fn vertex_normals_go_through_normal_to_world() {
    let source = moho_renderer::pipeline::main_shader_source();

    let module = validate("main shader", &source, Capabilities::empty());

    let definitions: Vec<_> = module
        .functions
        .iter()
        .filter(|(_, f)| f.name.as_deref() == Some("normal_to_world"))
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(definitions.len(), 1, "normal_to_world defined once");
    let math_calls = |fun: naga::MathFunction| {
        module.functions[definitions[0]]
            .expressions
            .iter()
            .filter(|(_, e)| matches!(e, naga::Expression::Math { fun: f, .. } if *f == fun))
            .count()
    };
    assert_eq!(
        math_calls(naga::MathFunction::Cross),
        3,
        "one cross per cofactor column"
    );
    assert_eq!(math_calls(naga::MathFunction::Normalize), 1);
    assert_eq!(
        math_calls(naga::MathFunction::Sign),
        0,
        "sign(det) is 0 for a flattened placement and would zero the normal"
    );
    let normal_fn = &module.functions[definitions[0]];
    let selects = normal_fn
        .expressions
        .iter()
        .filter(|(_, e)| matches!(e, naga::Expression::Select { .. }))
        .count();
    assert_eq!(
        selects, 1,
        "the determinant picks +1 or -1, so mirrored normals stay outward"
    );
    // Column k of the matrix argument, as `m[k]`.
    let column = |handle: naga::Handle<naga::Expression>| match normal_fn.expressions[handle] {
        naga::Expression::AccessIndex { base, index } => matches!(
            normal_fn.expressions[base],
            naga::Expression::FunctionArgument(0)
        )
        .then_some(index),
        _ => None,
    };
    let crosses: Vec<_> = normal_fn
        .expressions
        .iter()
        .filter_map(|(_, e)| match *e {
            naga::Expression::Math {
                fun: naga::MathFunction::Cross,
                arg,
                arg1: Some(arg1),
                ..
            } => Some((column(arg), column(arg1))),
            _ => None,
        })
        .collect();
    assert_eq!(
        crosses,
        [(Some(1), Some(2)), (Some(2), Some(0)), (Some(0), Some(1))],
        "cofactor columns c1 x c2, c2 x c0, c0 x c1"
    );
    let vs_main = module
        .entry_points
        .iter()
        .find(|e| e.name == "vs_main")
        .expect("vs_main");
    let calls_it = vs_main.function.body.iter().any(|statement| {
        matches!(statement, naga::Statement::Call { function, .. } if *function == definitions[0])
    });
    assert!(calls_it, "vs_main calls normal_to_world");
}

/// The formula `normal_to_world` encodes in `shaders/vertex.wgsl`.
fn cofactor_normal(cols: [glam::Vec3; 3], n: glam::Vec3) -> glam::Vec3 {
    let det = cols[0].dot(cols[1].cross(cols[2]));
    let cofactor = glam::Mat3::from_cols(
        cols[1].cross(cols[2]),
        cols[2].cross(cols[0]),
        cols[0].cross(cols[1]),
    );
    (cofactor * n * det.signum()).normalize()
}

fn any_vec3() -> impl proptest::strategy::Strategy<Value = glam::Vec3> {
    use proptest::strategy::Strategy;
    (-3.0f32..3.0, -3.0f32..3.0, -3.0f32..3.0).prop_map(|(x, y, z)| glam::Vec3::new(x, y, z))
}

proptest::proptest! {
    /// Proves the derivation `normal_to_world` encodes (cofactor times the determinant's
    /// sign equals the inverse transpose, mirrored matrices included), not the WGSL itself.
    #[test]
    fn cofactor_with_determinant_sign_matches_inverse_transpose(
        c0 in any_vec3(), c1 in any_vec3(), c2 in any_vec3(), n in any_vec3(),
    ) {
        let m = glam::Mat3::from_cols(c0, c1, c2);
        // Near-singular matrices make the inverse itself ill-conditioned.
        proptest::prop_assume!(m.determinant().abs() > 0.5);
        proptest::prop_assume!(n.length() > 0.1);

        let expected = (m.inverse().transpose() * n).normalize();
        let actual = cofactor_normal([c0, c1, c2], n);

        proptest::prop_assert!(
            actual.abs_diff_eq(expected, 1e-3),
            "{actual:?} != {expected:?} for {m:?}"
        );
    }
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
fn tonemap_shader_parses_and_validates() {
    let source = moho_renderer::pipeline::tonemap_shader_source();

    let module = validate("tonemap shader", source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(entry_points.contains(&"vs_main"), "{entry_points:?}");
    assert!(entry_points.contains(&"fs_main"), "{entry_points:?}");
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

    let skybox = moho_renderer::pipeline::skybox_shader_source();
    let main = moho_renderer::pipeline::main_shader_source();
    let expected_tail = [
        ("sky_zenith", offset_of!(LightingGpu, sky_zenith)),
        ("sky_horizon", offset_of!(LightingGpu, sky_horizon)),
        ("sky_ground", offset_of!(LightingGpu, sky_ground)),
    ];

    for (label, source) in [("main shader", main.as_str()), ("skybox shader", &skybox)] {
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
