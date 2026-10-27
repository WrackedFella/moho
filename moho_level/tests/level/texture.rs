use std::fs;

use image::ExtendedColorType;
use moho_level::{LevelError, LevelErrorKind, load_image, load_model};

use crate::support::textured::{
    CHECKER, ImageData, QUAD_UVS, Textured, jpeg_solid, png, png_rgba, textured_material,
    write_glb, write_gltf, write_gltf_into,
};

const FACTOR: [f32; 4] = [0.5, 0.25, 1.0, 1.0];

fn checker_model() -> Textured {
    Textured {
        uvs: Some(QUAD_UVS.to_vec()),
        images: vec![ImageData::file("tex.png", png_rgba([2, 2], &CHECKER))],
        texture_sources: Vec::new(),
        materials: vec![textured_material(0, FACTOR)],
    }
}

fn bad_image_name(err: &LevelError) -> &str {
    match err.kind() {
        LevelErrorKind::BadImage { image, .. } => image,
        other => panic!("expected BadImage, got {other:?}"),
    }
}

#[test]
fn base_colour_texture_loads_with_uvs_and_its_material_names_it() {
    let (dir, path) = write_gltf(&checker_model());

    let model = load_model(&path).expect("model loads");

    assert_eq!(model.images().len(), 1);
    let image = &model.images()[0];
    assert_eq!(image.image.size(), [2, 2]);
    assert_eq!(image.image.pixels(), CHECKER);
    assert_eq!(image.name.as_deref(), Some("image0"));
    let tex = dir.path().join("tex.png").canonicalize().expect("exists");
    assert_eq!(image.source.as_deref(), Some(tex.as_path()));
    assert_eq!(model.materials().len(), 1);
    assert_eq!(model.materials()[0].base_colour_image, Some(0));
    assert_eq!(model.materials()[0].base_colour, FACTOR);
    assert_eq!(model.meshes()[0].mesh.uvs(), Some(&QUAD_UVS[..]));
}

#[test]
fn embedded_glb_texture_equals_the_gltf_file_texture() {
    let file = checker_model();
    let embedded = Textured {
        images: vec![ImageData::Embedded(png_rgba([2, 2], &CHECKER))],
        ..checker_model()
    };
    let (_gltf_dir, gltf_path) = write_gltf(&file);
    let (_glb_dir, glb_path) = write_glb(&embedded);

    let from_gltf = load_model(&gltf_path).expect("gltf loads");
    let from_glb = load_model(&glb_path).expect("glb loads");

    assert_eq!(from_glb.images().len(), 1);
    assert_eq!(from_gltf.images().len(), 1);
    let (a, b) = (&from_gltf.images()[0], &from_glb.images()[0]);
    assert_eq!(a.image, b.image);
    assert_eq!(a.image.pixels(), CHECKER);
    assert_eq!(a.name, b.name);
    assert!(a.source.is_some(), "a file image reports its path");
    assert!(b.source.is_none(), "an embedded image has no path");
    assert_eq!(from_gltf.materials(), from_glb.materials());
    assert_eq!(from_glb.materials()[0].base_colour_image, Some(0));
    assert_eq!(
        from_gltf.meshes()[0].mesh.uvs(),
        from_glb.meshes()[0].mesh.uvs()
    );
    assert_eq!(from_glb.meshes()[0].mesh.uvs(), Some(&QUAD_UVS[..]));
}

#[test]
fn two_models_using_one_texture_file_report_one_source() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).expect("sub dir");
    let reaching = |uri: &str| Textured {
        images: vec![ImageData::dangling(uri)],
        ..checker_model()
    };
    let a_path = write_gltf_into(dir.path(), "a.gltf", &checker_model());
    let b_path = write_gltf_into(&sub, "b.gltf", &reaching("../tex.png"));
    let c_path = write_gltf_into(dir.path(), "c.gltf", &reaching("./tex.png"));

    let a = load_model(&a_path).expect("a loads");
    let b = load_model(&b_path).expect("b loads through ../");
    let c = load_model(&c_path).expect("c loads through ./");

    let a_source = a.images()[0].source.as_ref().expect("file image");
    assert!(a_source.is_absolute(), "{}", a_source.display());
    assert_eq!(
        a_source,
        &dir.path().join("tex.png").canonicalize().expect("exists")
    );
    assert_eq!(Some(a_source), b.images()[0].source.as_ref(), "via ../");
    assert_eq!(Some(a_source), c.images()[0].source.as_ref(), "via ./");
}

#[test]
fn materials_sharing_an_image_report_it_once() {
    let model = Textured {
        materials: vec![
            textured_material(0, [1.0; 4]),
            textured_material(0, [0.5, 0.5, 0.5, 1.0]),
        ],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("model loads");

    assert_eq!(loaded.images().len(), 1);
    assert_eq!(loaded.materials().len(), 2);
    assert_eq!(loaded.materials()[0].base_colour_image, Some(0));
    assert_eq!(loaded.materials()[1].base_colour_image, Some(0));
}

#[test]
fn percent_encoded_texture_uri_resolves_to_the_decoded_file() {
    let model = Textured {
        images: vec![ImageData::File {
            uri: "my%20tex.png".into(),
            written: Some(("my tex.png".into(), png_rgba([2, 2], &CHECKER))),
        }],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("model loads");

    assert_eq!(loaded.images()[0].image.pixels(), CHECKER);
}

#[test]
fn missing_texture_file_fails_naming_the_model_and_the_uri() {
    let model = Textured {
        images: vec![ImageData::dangling("gone.png")],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let err = load_model(&path).expect_err("the texture file does not exist");

    assert_eq!(err.path(), path);
    assert!(
        matches!(err.kind(), LevelErrorKind::MissingImage { uri, .. } if uri == "gone.png"),
        "got {:?}",
        err.kind()
    );
    let message = err.to_string();
    assert!(message.contains("model.gltf"), "got {message}");
    assert!(message.contains("gone.png"), "got {message}");
    assert!(
        std::error::Error::source(&err).is_some(),
        "keeps the io cause"
    );
}

#[test]
fn undecodable_image_fails_naming_its_uri() {
    let cases: [(&str, &[u8]); 2] = [
        ("junk.png", b"this is not an image"),
        ("anim.gif", b"GIF89a\x01\x00\x01\x00"),
    ];
    for (uri, bytes) in cases {
        let model = Textured {
            images: vec![ImageData::file(uri, bytes.to_vec())],
            ..checker_model()
        };
        let (_dir, path) = write_gltf(&model);

        let err = load_model(&path).expect_err("these bytes do not decode as PNG or JPEG");

        assert_eq!(err.path(), path, "{uri}");
        assert_eq!(bad_image_name(&err), uri);
    }
}

#[test]
fn image_format_is_chosen_by_content_not_by_file_name() {
    let model = Textured {
        images: vec![ImageData::file("tex.jpg", png_rgba([2, 2], &CHECKER))],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("PNG bytes decode under a .jpg name");

    assert_eq!(loaded.images()[0].image.pixels(), CHECKER);
}

#[test]
fn embedded_image_format_is_chosen_by_content_not_by_mime_type() {
    let model = Textured {
        images: vec![ImageData::EmbeddedAs {
            bytes: png_rgba([2, 2], &CHECKER),
            mime: "image/jpeg".into(),
        }],
        ..checker_model()
    };
    let (_dir, path) = write_glb(&model);

    let loaded = load_model(&path).expect("PNG bytes decode under a jpeg mime type");

    assert_eq!(loaded.images()[0].image.pixels(), CHECKER);
}

#[test]
fn embedded_image_slice_outside_its_buffer_fails_naming_its_index() {
    let model = Textured {
        images: vec![ImageData::EmbeddedOutOfRange],
        ..checker_model()
    };
    let (_dir, path) = write_glb(&model);

    let err = load_model(&path).expect_err("the view runs past the buffer");

    assert_eq!(err.path(), path);
    assert_eq!(bad_image_name(&err), "#0");
}

#[test]
fn two_textures_sharing_one_image_decode_it_once() {
    let model = Textured {
        texture_sources: vec![0, 0],
        materials: vec![
            textured_material(0, [1.0; 4]),
            textured_material(1, [0.5, 0.5, 0.5, 1.0]),
        ],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("model loads");

    assert_eq!(loaded.images().len(), 1, "one image, however many textures");
    assert_eq!(loaded.materials().len(), 2);
    assert_eq!(loaded.materials()[0].base_colour_image, Some(0));
    assert_eq!(loaded.materials()[1].base_colour_image, Some(0));
}

#[test]
fn base_colour_texture_on_texcoord_set_1_loads_using_texcoord_0() {
    let mut material = textured_material(0, FACTOR);
    material["pbrMetallicRoughness"]["baseColorTexture"]["texCoord"] = serde_json::json!(1);
    let model = Textured {
        materials: vec![material],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("another texcoord set still loads");

    assert_eq!(loaded.materials()[0].base_colour_image, Some(0));
    assert_eq!(loaded.meshes()[0].mesh.uvs(), Some(&QUAD_UVS[..]));
}

#[test]
fn absolute_path_image_uri_fails_as_bad_image() {
    let elsewhere = tempfile::tempdir().expect("temp dir");
    let outside = elsewhere.path().join("outside.png");
    fs::write(&outside, png_rgba([2, 2], &CHECKER)).expect("write png");
    let uri = outside.to_str().expect("utf-8 temp path");
    let model = Textured {
        images: vec![ImageData::dangling(uri)],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let err = load_model(&path).expect_err("a model may not name a file by absolute path");

    assert_eq!(bad_image_name(&err), uri);
}

#[test]
fn http_image_uri_fails_as_bad_image() {
    let uri = "http://example.com/tex.png";
    let model = Textured {
        images: vec![ImageData::dangling(uri)],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let err = load_model(&path).expect_err("remote images are unsupported");

    assert_eq!(bad_image_name(&err), uri);
}

#[test]
fn undecodable_embedded_image_fails_naming_its_index() {
    let model = Textured {
        images: vec![ImageData::Embedded(b"this is not an image".to_vec())],
        ..checker_model()
    };
    let (_dir, path) = write_glb(&model);

    let err = load_model(&path).expect_err("junk bytes do not decode");

    assert_eq!(bad_image_name(&err), "#0");
}

#[test]
fn data_uri_image_fails_as_bad_image() {
    let uri = "data:image/png;base64,iVBORw0KGgo=";
    let model = Textured {
        images: vec![ImageData::dangling(uri)],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let err = load_model(&path).expect_err("data uris are unsupported");

    assert_eq!(bad_image_name(&err), uri);
}

#[test]
fn jpeg_base_colour_texture_decodes() {
    let model = Textured {
        images: vec![ImageData::file(
            "tex.jpg",
            jpeg_solid([8, 8], [200, 40, 40]),
        )],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("model loads");

    let image = &loaded.images()[0].image;
    assert_eq!(image.size(), [8, 8]);
    for pixel in image.pixels() {
        let [r, g, b, a] = *pixel;
        assert!(r.abs_diff(200) <= 6, "red {r}");
        assert!(g.abs_diff(40) <= 6, "green {g}");
        assert!(b.abs_diff(40) <= 6, "blue {b}");
        assert_eq!(a, 255);
    }
}

#[test]
fn an_image_used_only_as_a_normal_map_is_not_read() {
    let mut material = textured_material(1, FACTOR);
    material["normalTexture"] = serde_json::json!({ "index": 0 });
    let model = Textured {
        images: vec![
            ImageData::dangling("normals-missing.png"),
            ImageData::file("tex.png", png_rgba([2, 2], &CHECKER)),
        ],
        materials: vec![material],
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("the normal map is never read");

    assert_eq!(loaded.images().len(), 1);
    assert_eq!(loaded.images()[0].image.pixels(), CHECKER);
    assert_eq!(
        loaded.materials()[0].base_colour_image,
        Some(0),
        "indexes images(), not the file's images array"
    );
}

#[test]
fn textured_primitive_without_texcoord_0_fails_as_bad_mesh() {
    let model = Textured {
        uvs: None,
        ..checker_model()
    };
    let (_dir, path) = write_gltf(&model);

    let err = load_model(&path).expect_err("a textured mesh needs uvs");

    assert_eq!(err.path(), path);
    assert!(
        matches!(err.kind(), LevelErrorKind::BadMesh { mesh: 0, .. }),
        "got {:?}",
        err.kind()
    );
}

#[test]
fn untextured_primitive_without_texcoord_0_loads_without_uvs() {
    let model = Textured {
        uvs: None,
        images: Vec::new(),
        texture_sources: Vec::new(),
        materials: vec![serde_json::json!({ "name": "plain" })],
    };
    let (_dir, path) = write_gltf(&model);

    let loaded = load_model(&path).expect("model loads");

    assert!(loaded.meshes()[0].mesh.uvs().is_none());
    assert!(loaded.images().is_empty());
    assert_eq!(loaded.materials()[0].base_colour_image, None);
}

#[test]
fn load_image_decodes_png_colours_and_alpha_straight() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("icon.png");
    fs::write(&path, png_rgba([2, 2], &CHECKER)).expect("write png");

    let image = load_image(&path).expect("png decodes");

    assert_eq!(image.size(), [2, 2]);
    assert_eq!(image.pixels(), CHECKER);
}

#[test]
fn load_image_converts_other_colour_types_and_depths_to_rgba8() {
    let dir = tempfile::tempdir().expect("temp dir");
    let grey = dir.path().join("grey.png");
    let rgb = dir.path().join("rgb.png");
    let deep = dir.path().join("deep.png");
    fs::write(&grey, png([2, 1], ExtendedColorType::L8, &[0, 90])).expect("write");
    fs::write(&rgb, png([1, 1], ExtendedColorType::Rgb8, &[10, 20, 30])).expect("write");
    let deep_raw: Vec<u8> = [0u16, 65535, 65535, 0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    fs::write(&deep, png([1, 1], ExtendedColorType::Rgba16, &deep_raw)).expect("write");

    let grey = load_image(&grey).expect("grey decodes");
    let rgb = load_image(&rgb).expect("rgb decodes");
    let deep = load_image(&deep).expect("16-bit decodes");

    assert_eq!(grey.pixels(), [[0, 0, 0, 255], [90, 90, 90, 255]]);
    assert_eq!(rgb.pixels(), [[10, 20, 30, 255]]);
    assert_eq!(deep.pixels(), [[0, 255, 255, 0]]);
}

#[test]
fn load_image_of_a_missing_file_fails_naming_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("absent.png");

    let err = load_image(&path).expect_err("no such file");

    assert_eq!(err.path(), path);
    assert!(
        matches!(err.kind(), LevelErrorKind::Read(_)),
        "got {:?}",
        err.kind()
    );
    assert!(err.to_string().contains("absent.png"), "got {err}");
}

#[test]
fn load_image_of_a_non_image_fails_naming_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("notes.txt");
    fs::write(&path, "just some text").expect("write");

    let err = load_image(&path).expect_err("text is not an image");

    assert_eq!(err.path(), path);
    assert_eq!(bad_image_name(&err), "notes.txt");
    assert!(err.to_string().contains("notes.txt"), "got {err}");
}
