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

fn read_shader(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../shaders")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()))
}

/// The global variable `handle` reaches through accesses, if it does.
fn root_global(
    function: &naga::Function,
    handle: naga::Handle<naga::Expression>,
) -> Option<naga::Handle<naga::GlobalVariable>> {
    match function.expressions[handle] {
        naga::Expression::GlobalVariable(global) => Some(global),
        naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
            root_global(function, base)
        }
        _ => None,
    }
}

/// Stores into `target` found in `block`, as `(stores outside any loop, stores inside a loop)`.
fn stores_into(
    function: &naga::Function,
    block: &naga::Block,
    target: naga::Handle<naga::GlobalVariable>,
    in_loop: bool,
    counts: &mut (usize, usize),
) {
    for statement in block {
        match statement {
            naga::Statement::Store { pointer, .. }
                if root_global(function, *pointer) == Some(target) =>
            {
                if in_loop {
                    counts.1 += 1;
                } else {
                    counts.0 += 1;
                }
            }
            naga::Statement::Block(inner) => stores_into(function, inner, target, in_loop, counts),
            naga::Statement::If { accept, reject, .. } => {
                stores_into(function, accept, target, in_loop, counts);
                stores_into(function, reject, target, in_loop, counts);
            }
            naga::Statement::Switch { cases, .. } => {
                for case in cases {
                    stores_into(function, &case.body, target, in_loop, counts);
                }
            }
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                stores_into(function, body, target, true, counts);
                stores_into(function, continuing, target, true, counts);
            }
            _ => {}
        }
    }
}

fn contains_statement(block: &naga::Block, wanted: &dyn Fn(&naga::Statement) -> bool) -> bool {
    block.iter().any(|statement| {
        wanted(statement)
            || match statement {
                naga::Statement::Block(inner) => contains_statement(inner, wanted),
                naga::Statement::If { accept, reject, .. } => {
                    contains_statement(accept, wanted) || contains_statement(reject, wanted)
                }
                naga::Statement::Switch { cases, .. } => cases
                    .iter()
                    .any(|case| contains_statement(&case.body, wanted)),
                naga::Statement::Loop {
                    body, continuing, ..
                } => contains_statement(body, wanted) || contains_statement(continuing, wanted),
                _ => false,
            }
    })
}

/// Stores into `target` outside any loop, in program order, as `(pointer, value)`.
fn straight_line_stores(
    function: &naga::Function,
    block: &naga::Block,
    target: naga::Handle<naga::GlobalVariable>,
    found: &mut Vec<(
        naga::Handle<naga::Expression>,
        naga::Handle<naga::Expression>,
    )>,
) {
    for statement in block {
        match statement {
            naga::Statement::Store { pointer, value }
                if root_global(function, *pointer) == Some(target) =>
            {
                found.push((*pointer, *value));
            }
            naga::Statement::Block(inner) => straight_line_stores(function, inner, target, found),
            naga::Statement::If { accept, reject, .. } => {
                straight_line_stores(function, accept, target, found);
                straight_line_stores(function, reject, target, found);
            }
            _ => {}
        }
    }
}

/// A `u32` literal expression's value.
fn u32_literal(function: &naga::Function, handle: naga::Handle<naga::Expression>) -> Option<u32> {
    match function.expressions[handle] {
        naga::Expression::Literal(naga::Literal::U32(n)) => Some(n),
        _ => None,
    }
}

/// For `arena[dst + n]` (or `arena[dst]` for n = 0): the shared `dst` expression and `n`.
fn arena_word_offset(
    function: &naga::Function,
    pointer: naga::Handle<naga::Expression>,
) -> (naga::Handle<naga::Expression>, u32) {
    let naga::Expression::Access { index, .. } = function.expressions[pointer] else {
        panic!("a store into the arena indexes it dynamically");
    };
    match function.expressions[index] {
        naga::Expression::Binary {
            op: naga::BinaryOperator::Add,
            left,
            right,
        } => (
            left,
            u32_literal(function, right).expect("the word offset is a constant"),
        ),
        _ => (index, 0),
    }
}

/// For `bitcast<u32>(base.component)`: the vector expression and the component.
fn bitcast_component(
    function: &naga::Function,
    value: naga::Handle<naga::Expression>,
) -> (naga::Handle<naga::Expression>, u32) {
    let naga::Expression::As {
        expr,
        kind: naga::ScalarKind::Uint,
        convert: None,
    } = function.expressions[value]
    else {
        panic!("a stored word is a bitcast to u32");
    };
    let naga::Expression::AccessIndex { base, index } = function.expressions[expr] else {
        panic!("a stored word is one vector component");
    };
    (base, index)
}

#[test]
fn skin_shader_validates_and_rewrites_only_position_and_normal() {
    let source = moho_renderer::pipeline::skin_shader_source();

    let module = validate("skin shader", &source, Capabilities::empty());

    assert_eq!(module.entry_points.len(), 1, "one compute entry point");
    let entry = &module.entry_points[0];
    assert_eq!(entry.stage, naga::ShaderStage::Compute);
    assert_eq!(entry.workgroup_size, [64, 1, 1], "64 threads per group");
    let (words, declared) = module
        .overrides
        .iter()
        .find(|(_, o)| o.name.as_deref() == Some("VERTEX_WORDS"))
        .expect("skin.wgsl declares `override VERTEX_WORDS`");
    assert_eq!(
        module.types[declared.ty].inner,
        naga::TypeInner::Scalar(naga::Scalar::U32),
        "VERTEX_WORDS is a u32"
    );
    assert!(
        declared.init.is_none(),
        "the pipeline sets it from size_of::<Vertex>() / 4"
    );
    let function = &entry.function;
    assert!(
        function
            .expressions
            .iter()
            .any(|(_, e)| matches!(e, naga::Expression::Override(h) if *h == words)),
        "the word copy is sized by VERTEX_WORDS"
    );
    let normal_to_world = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("normal_to_world"))
        .map(|(handle, _)| handle)
        .expect("the skin shader holds the shared normal_to_world");
    assert!(
        contains_statement(
            &function.body,
            &|s| matches!(s, naga::Statement::Call { function, .. } if *function == normal_to_world)
        ),
        "the entry point blends normals through normal_to_world"
    );
    let writable: Vec<_> = module
        .global_variables
        .iter()
        .filter(|(_, g)| {
            g.space
                == naga::AddressSpace::Storage {
                    access: naga::StorageAccess::LOAD | naga::StorageAccess::STORE,
                }
        })
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(writable.len(), 1, "the arena is the only writable buffer");
    let mut counts = (0, 0);
    stores_into(function, &function.body, writable[0], false, &mut counts);
    assert_eq!(
        counts,
        (6, 1),
        "six stores for position and normal words 0-5, and one word copy in a loop for the rest"
    );

    let mut straight = Vec::new();
    straight_line_stores(function, &function.body, writable[0], &mut straight);
    let offsets: Vec<(naga::Handle<naga::Expression>, u32)> = straight
        .iter()
        .map(|(pointer, _)| arena_word_offset(function, pointer.to_owned()))
        .collect();
    let words: Vec<u32> = offsets.iter().map(|(_, n)| *n).collect();
    assert_eq!(
        words,
        [0, 1, 2, 3, 4, 5],
        "the six stores land on destination words 0-5, once each, in order"
    );
    assert!(
        offsets.iter().all(|(dst, _)| *dst == offsets[0].0),
        "every store is relative to the same destination base"
    );
    let components: Vec<_> = straight
        .iter()
        .map(|(_, value)| bitcast_component(function, *value))
        .collect();
    assert_eq!(
        components.iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        [0, 1, 2, 0, 1, 2],
        "position x y z then normal x y z"
    );
    assert!(
        components[..3].iter().all(|(b, _)| *b == components[0].0),
        "words 0-2 come from one vector, the skinned position"
    );
    assert!(
        components[3..].iter().all(|(b, _)| *b == components[3].0),
        "words 3-5 come from one vector, the skinned normal"
    );
    assert_ne!(
        components[0].0, components[3].0,
        "position and normal are different vectors"
    );
    assert!(
        matches!(
            function.expressions[components[3].0],
            naga::Expression::CallResult(f) if f == normal_to_world
        ),
        "words 3-5 are the normal_to_world result"
    );
    let mut counters: Vec<u32> = function
        .local_variables
        .iter()
        .filter_map(|(_, local)| local.init)
        .filter_map(|init| u32_literal(function, init))
        .collect();
    counters.sort_unstable();
    assert_eq!(
        counters,
        [0, 6],
        "the blend loop counts from 0 and the word copy loop starts after word 5"
    );
}

#[test]
fn main_and_skin_shaders_share_normal_to_world() {
    let normal = read_shader("normal.wgsl");
    let vertex = read_shader("vertex.wgsl");
    let main = moho_renderer::pipeline::main_shader_source();
    let skin = moho_renderer::pipeline::skin_shader_source();

    assert!(
        normal.contains("fn normal_to_world"),
        "normal.wgsl holds it"
    );
    assert!(
        !vertex.contains("normal_to_world(m:"),
        "vertex.wgsl calls it but no longer defines it"
    );
    for (label, source) in [("main", &main), ("skin", &skin)] {
        assert!(
            source.contains(normal.trim()),
            "{label} shader source embeds normal.wgsl unchanged"
        );
        let module = validate(label, source, Capabilities::empty());
        let definitions = module
            .functions
            .iter()
            .filter(|(_, f)| f.name.as_deref() == Some("normal_to_world"))
            .count();
        assert_eq!(definitions, 1, "{label}: normal_to_world defined once");
    }
}

/// The formula `normal_to_world` encodes in `shaders/normal.wgsl`.
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

#[test]
fn bloom_shader_declares_threshold_and_knee_overrides() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/bloom.wgsl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", path.display()));

    let module = validate("bloom.wgsl", &source, Capabilities::empty());

    let entry_points: Vec<&str> = module
        .entry_points
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    for expected in ["vs_main", "fs_prefilter", "fs_downsample", "fs_upsample"] {
        assert!(
            entry_points.contains(&expected),
            "{expected} missing: {entry_points:?}"
        );
    }
    let prefilter = module
        .entry_points
        .iter()
        .find(|e| e.name == "fs_prefilter")
        .expect("fs_prefilter");
    for name in ["bloom_threshold", "bloom_knee"] {
        let (handle, declared) = module
            .overrides
            .iter()
            .find(|(_, o)| o.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("bloom.wgsl declares `override {name}`"));
        assert_eq!(
            module.types[declared.ty].inner,
            naga::TypeInner::Scalar(naga::Scalar::F32),
            "{name} is an f32"
        );
        assert!(declared.init.is_none(), "the pipeline supplies {name}");
        assert!(
            std::iter::once(&prefilter.function)
                .chain(module.functions.iter().map(|(_, f)| f))
                .any(|f| f
                    .expressions
                    .iter()
                    .any(|(_, e)| matches!(e, naga::Expression::Override(h) if *h == handle))),
            "the prefilter reads {name}"
        );
    }
}

/// The WGSL type a shader input must have to read an attribute of `format`.
fn wgsl_type_of(format: wgpu::VertexFormat) -> naga::TypeInner {
    use naga::{Scalar, TypeInner, VectorSize};
    match format {
        wgpu::VertexFormat::Float32 => TypeInner::Scalar(Scalar::F32),
        wgpu::VertexFormat::Uint32 => TypeInner::Scalar(Scalar::U32),
        wgpu::VertexFormat::Float32x2 => TypeInner::Vector {
            size: VectorSize::Bi,
            scalar: Scalar::F32,
        },
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

/// True when `handle` satisfies `is_target` or is a swizzle, component access, splat or
/// composition of something that does.
fn derives_from(
    expressions: &naga::Arena<naga::Expression>,
    handle: naga::Handle<naga::Expression>,
    is_target: &dyn Fn(naga::Handle<naga::Expression>) -> bool,
) -> bool {
    if is_target(handle) {
        return true;
    }
    match &expressions[handle] {
        naga::Expression::Swizzle { vector, .. } => derives_from(expressions, *vector, is_target),
        naga::Expression::AccessIndex { base, .. } => derives_from(expressions, *base, is_target),
        naga::Expression::Splat { value, .. } => derives_from(expressions, *value, is_target),
        naga::Expression::Compose { components, .. } => components
            .iter()
            .any(|c| derives_from(expressions, *c, is_target)),
        _ => false,
    }
}

/// True when `handle` reads the `@location(location)` input of `function`, either as a
/// direct argument or as a member of a struct argument.
fn is_input_at(
    module: &naga::Module,
    function: &naga::Function,
    handle: naga::Handle<naga::Expression>,
    location: u32,
) -> bool {
    let at = |binding: &Option<naga::Binding>| matches!(binding, Some(naga::Binding::Location { location: l, .. }) if *l == location);
    match &function.expressions[handle] {
        naga::Expression::FunctionArgument(i) => at(&function.arguments[*i as usize].binding),
        naga::Expression::AccessIndex { base, index } => {
            let naga::Expression::FunctionArgument(i) = function.expressions[*base] else {
                return false;
            };
            let argument = &function.arguments[i as usize];
            let naga::TypeInner::Struct { members, .. } = &module.types[argument.ty].inner else {
                return false;
            };
            at(&members[*index as usize].binding)
        }
        _ => false,
    }
}

/// The binding index within group 2 of the global `handle` names, if it is one.
fn group_2_binding(
    module: &naga::Module,
    function: &naga::Function,
    handle: naga::Handle<naga::Expression>,
) -> Option<u32> {
    let naga::Expression::GlobalVariable(global) = function.expressions[handle] else {
        return None;
    };
    module.global_variables[global]
        .binding
        .as_ref()
        .filter(|b| b.group == 2)
        .map(|b| b.binding)
}

#[test]
fn vertex_stage_passes_the_uv_input_to_the_location_10_output() {
    let source = moho_renderer::pipeline::main_shader_source();
    let module = validate("main shader", &source, Capabilities::empty());
    let vs_main = &module
        .entry_points
        .iter()
        .find(|e| e.name == "vs_main")
        .expect("vs_main")
        .function;
    let result = vs_main.result.as_ref().expect("vs_main returns VsOut");
    let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
        panic!("vs_main returns a struct");
    };
    let uv_member = members
        .iter()
        .position(|m| {
            matches!(
                m.binding,
                Some(naga::Binding::Location { location: 10, .. })
            )
        })
        .expect("VsOut has a member at @location(10)");

    let stored_uvs: Vec<_> = vs_main
        .body
        .iter()
        .filter_map(|statement| match statement {
            naga::Statement::Store { pointer, value } => match vs_main.expressions[*pointer] {
                naga::Expression::AccessIndex { base, index }
                    if index as usize == uv_member
                        && matches!(
                            vs_main.expressions[base],
                            naga::Expression::LocalVariable(_)
                        ) =>
                {
                    Some(*value)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();

    assert_eq!(
        stored_uvs.len(),
        1,
        "vs_main writes VsOut's uv exactly once"
    );
    assert!(
        derives_from(&vs_main.expressions, stored_uvs[0], &|h| is_input_at(
            &module, vs_main, h, 14
        )),
        "the location-10 output comes from the location-14 vertex input"
    );
}

#[test]
fn fragment_multiplies_the_material_colour_by_the_base_colour_texel() {
    let source = moho_renderer::pipeline::main_shader_source();
    let module = validate("main shader", &source, Capabilities::empty());

    let mut group_2: Vec<(u32, &naga::TypeInner)> = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| {
            let binding = global.binding.as_ref().filter(|b| b.group == 2)?;
            Some((binding.binding, &module.types[global.ty].inner))
        })
        .collect();
    group_2.sort_by_key(|(binding, _)| *binding);
    let fs_main = &module
        .entry_points
        .iter()
        .find(|e| e.name == "fs_main")
        .expect("fs_main")
        .function;
    let samples: Vec<_> = fs_main
        .expressions
        .iter()
        .filter(|(_, e)| matches!(e, naga::Expression::ImageSample { .. }))
        .map(|(handle, _)| handle)
        .collect();

    assert_eq!(group_2.len(), 2, "group 2 is the texture and its sampler");
    assert_eq!(group_2[0].0, 0);
    assert!(
        matches!(
            group_2[0].1,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed: false,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false
                },
            }
        ),
        "binding 0 is a 2D float texture, got {:?}",
        group_2[0].1
    );
    assert_eq!(group_2[1].0, 1);
    assert!(
        matches!(group_2[1].1, naga::TypeInner::Sampler { comparison: false }),
        "binding 1 is a filtering sampler, got {:?}",
        group_2[1].1
    );
    assert_eq!(samples.len(), 1, "fs_main samples the texture once");
    let sample = samples[0];
    let naga::Expression::ImageSample {
        image,
        sampler,
        coordinate,
        ..
    } = fs_main.expressions[sample]
    else {
        unreachable!("filtered to image samples");
    };
    assert_eq!(
        group_2_binding(&module, fs_main, image),
        Some(0),
        "the sampled image is the group-2 texture"
    );
    assert_eq!(
        group_2_binding(&module, fs_main, sampler),
        Some(1),
        "the sampler is the group-2 sampler"
    );
    assert!(
        derives_from(&fs_main.expressions, coordinate, &|h| is_input_at(
            &module, fs_main, h, 10
        )),
        "the sample coordinate comes from the @location(10) member of fs_main's input"
    );
    let sampled_at = fs_main
        .body
        .iter()
        .position(|s| matches!(s, naga::Statement::Emit(r) if r.clone().any(|h| h == sample)))
        .expect("the sample is emitted in fs_main's body");
    let first_branch = fs_main.body.iter().position(|s| {
        matches!(
            s,
            naga::Statement::If { .. }
                | naga::Statement::Loop { .. }
                | naga::Statement::Switch { .. }
        )
    });
    assert!(
        first_branch.is_none_or(|branch| sampled_at < branch),
        "the sample stays in uniform control flow, ahead of every branch"
    );
    let is_texel_rgb = |operand: naga::Handle<naga::Expression>| {
        matches!(
            fs_main.expressions[operand],
            naga::Expression::Swizzle { size: naga::VectorSize::Tri, vector, pattern }
                if pattern[..3] == [
                    naga::SwizzleComponent::X,
                    naga::SwizzleComponent::Y,
                    naga::SwizzleComponent::Z,
                ] && derives_from(&fs_main.expressions, vector, &|h| h == sample)
        )
    };
    let multiplied_by_texel_rgb = fs_main.expressions.iter().any(|(_, e)| {
        matches!(
            e,
            naga::Expression::Binary { op: naga::BinaryOperator::Multiply, left, right }
                if is_texel_rgb(*left) || is_texel_rgb(*right)
        )
    });
    assert!(
        multiplied_by_texel_rgb,
        "a product takes the texel's rgb, not its alpha"
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

/// Offsets and total size of `Shadow` as naga lays it out in `source`.
fn shadow_layout(
    label: &str,
    source: &str,
    capabilities: Capabilities,
) -> (Vec<(String, u32)>, u32) {
    let module = validate(label, source, capabilities);
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, t)| t.name.as_deref() == Some("Shadow"))
        .unwrap_or_else(|| panic!("{label} declares struct Shadow"));
    let naga::TypeInner::Struct { members, span } = &ty.inner else {
        panic!("{label}: Shadow is not a struct");
    };
    let members = members
        .iter()
        .map(|m| (m.name.clone().unwrap_or_default(), m.offset))
        .collect();
    (members, *span)
}

#[test]
fn shadow_struct_matches_shadow_gpu_in_both_shaders() {
    use moho_renderer::ShadowGpu;
    use std::mem::{offset_of, size_of};

    let main = moho_renderer::pipeline::main_shader_source();
    let shadow_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shaders/shadow.wgsl");
    let shadow = std::fs::read_to_string(&shadow_path)
        .unwrap_or_else(|e| panic!("failed to read shader {}: {e}", shadow_path.display()));
    let expected = [
        ("matrices", offset_of!(ShadowGpu, matrices)),
        ("split_far", offset_of!(ShadowGpu, split_far)),
        ("texel_world", offset_of!(ShadowGpu, texel_world)),
        ("camera_forward", offset_of!(ShadowGpu, camera_forward)),
        (
            "light_intensities",
            offset_of!(ShadowGpu, light_intensities),
        ),
        ("metadata", offset_of!(ShadowGpu, metadata)),
    ];

    for (label, source, capabilities) in [
        ("main shader", main.as_str(), Capabilities::empty()),
        ("shadow shader", shadow.as_str(), Capabilities::IMMEDIATES),
    ] {
        let (members, span) = shadow_layout(label, source, capabilities);

        assert_eq!(span as usize, size_of::<ShadowGpu>(), "{label}: span");
        let found: Vec<(&str, usize)> = members
            .iter()
            .map(|(name, offset)| (name.as_str(), *offset as usize))
            .collect();
        assert_eq!(found, expected, "{label}: members and offsets");
    }
}
