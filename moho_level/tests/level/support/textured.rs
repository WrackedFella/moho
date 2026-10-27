//! Writes one textured quad as a `.gltf` (image files beside it) or a `.glb` (images
//! embedded in the buffer), and encodes the test images in code.

use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::pad4;

/// Texture coordinates of the quad's four corners, none of them symmetric so a swapped
/// or flipped channel shows.
pub const QUAD_UVS: [[f32; 2]; 4] = [[0.25, 0.75], [0.0, 1.0], [1.0, 0.5], [0.5, 0.0]];

/// A 2x2 image in row-major order from the top-left; the last pixel is half transparent
/// with straight (not premultiplied) colour.
pub const CHECKER: [[u8; 4]; 4] = [
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [200, 100, 50, 128],
];

/// Encode `raw` (laid out as `colour` says, native-endian for 16-bit) as a PNG.
pub fn png(size: [u32; 2], colour: ExtendedColorType, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(raw, size[0], size[1], colour)
        .expect("encode png");
    out
}

/// Encode straight-alpha RGBA8 pixels as a PNG.
pub fn png_rgba(size: [u32; 2], pixels: &[[u8; 4]]) -> Vec<u8> {
    png(size, ExtendedColorType::Rgba8, pixels.as_flattened())
}

/// A JPEG of one flat colour.
pub fn jpeg_solid(size: [u32; 2], rgb: [u8; 3]) -> Vec<u8> {
    let raw = rgb.repeat((size[0] * size[1]) as usize);
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 100)
        .write_image(&raw, size[0], size[1], ExtendedColorType::Rgb8)
        .expect("encode jpeg");
    out
}

/// How a glTF `images` entry reaches its pixels.
pub enum ImageData {
    /// A `uri` entry. `written` is the file name saved beside the model and its bytes;
    /// `None` leaves the uri dangling.
    File {
        uri: String,
        written: Option<(String, Vec<u8>)>,
    },
    /// A `bufferView` entry; the bytes live in the model's buffer and `mimeType` follows
    /// them (JPEG magic or PNG).
    Embedded(Vec<u8>),
    /// A `bufferView` entry declaring `mime` whatever the bytes are.
    EmbeddedAs { bytes: Vec<u8>, mime: String },
    /// A `bufferView` entry whose slice runs far past the end of its buffer.
    EmbeddedOutOfRange,
    /// A `bufferView` entry whose `byteOffset + byteLength` overflows `u64`.
    EmbeddedOffsetOverflow,
}

impl ImageData {
    /// A file saved beside the model and referenced by its own name.
    pub fn file(name: &str, bytes: Vec<u8>) -> Self {
        Self::File {
            uri: name.to_owned(),
            written: Some((name.to_owned(), bytes)),
        }
    }

    /// A uri nothing is saved under.
    pub fn dangling(uri: &str) -> Self {
        Self::File {
            uri: uri.to_owned(),
            written: None,
        }
    }
}

/// One quad (four vertices, two triangles) and the materials and images it refers to.
/// Image `i` is named `image{i}`; texture `i` shows image `i` unless `texture_sources` says
/// otherwise.
#[derive(Default)]
pub struct Textured {
    /// `TEXCOORD_0`, one per vertex; `None` writes no texture coordinates.
    pub uvs: Option<Vec<[f32; 2]>>,
    pub images: Vec<ImageData>,
    /// The image index each texture shows; empty means texture `i` shows image `i`.
    pub texture_sources: Vec<usize>,
    /// glTF material objects; the quad uses material 0 when there is one.
    pub materials: Vec<Value>,
}

/// A material tinted by `factor` that takes its base colour from texture `texture`.
pub fn textured_material(texture: usize, factor: [f32; 4]) -> Value {
    json!({
        "name": "skin",
        "pbrMetallicRoughness": {
            "baseColorFactor": factor,
            "baseColorTexture": { "index": texture },
        },
    })
}

fn push_vec<const N: usize>(bytes: &mut Vec<u8>, values: &[[f32; N]]) {
    for v in values {
        for c in v {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
}

fn add_view(blob: &mut Vec<u8>, views: &mut Vec<Value>, bytes: &[u8]) -> usize {
    pad4(blob, 0);
    views.push(json!({ "buffer": 0, "byteOffset": blob.len(), "byteLength": bytes.len() }));
    blob.extend_from_slice(bytes);
    views.len() - 1
}

/// The glTF JSON without a buffer uri, and the binary blob.
fn build(model: &Textured) -> (Value, Vec<u8>) {
    let positions = [
        [-1.0, 0.0, -1.0],
        [-1.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, -1.0],
    ];
    let normals = [[0.0, 1.0, 0.0]; 4];
    let mut blob = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();

    let mut bytes = Vec::new();
    push_vec(&mut bytes, &positions);
    let view = add_view(&mut blob, &mut views, &bytes);
    accessors.push(json!({
        "bufferView": view, "componentType": 5126, "count": 4, "type": "VEC3",
        "min": [-1.0, 0.0, -1.0], "max": [1.0, 0.0, 1.0],
    }));
    let mut attributes = json!({ "POSITION": 0 });

    bytes.clear();
    push_vec(&mut bytes, &normals);
    let view = add_view(&mut blob, &mut views, &bytes);
    accessors.push(json!({
        "bufferView": view, "componentType": 5126, "count": 4, "type": "VEC3",
    }));
    attributes["NORMAL"] = json!(1);

    if let Some(uvs) = &model.uvs {
        bytes.clear();
        push_vec(&mut bytes, uvs);
        let view = add_view(&mut blob, &mut views, &bytes);
        accessors.push(json!({
            "bufferView": view, "componentType": 5126, "count": uvs.len(), "type": "VEC2",
        }));
        attributes["TEXCOORD_0"] = json!(accessors.len() - 1);
    }

    let indices: Vec<u8> = [0u32, 1, 2, 0, 2, 3]
        .iter()
        .flat_map(|i| i.to_le_bytes())
        .collect();
    let view = add_view(&mut blob, &mut views, &indices);
    accessors.push(json!({
        "bufferView": view, "componentType": 5125, "count": 6, "type": "SCALAR",
    }));
    let mut primitive = json!({
        "attributes": attributes,
        "indices": accessors.len() - 1,
        "mode": 4,
    });
    if !model.materials.is_empty() {
        primitive["material"] = json!(0);
    }

    let images: Vec<Value> = model
        .images
        .iter()
        .enumerate()
        .map(|(i, image)| {
            let name = format!("image{i}");
            match image {
                ImageData::File { uri, .. } => json!({ "name": name, "uri": uri }),
                ImageData::Embedded(bytes) => {
                    let view = add_view(&mut blob, &mut views, bytes);
                    let mime = if bytes.starts_with(&[0xFF, 0xD8]) {
                        "image/jpeg"
                    } else {
                        "image/png"
                    };
                    json!({ "name": name, "bufferView": view, "mimeType": mime })
                }
                ImageData::EmbeddedAs { bytes, mime } => {
                    let view = add_view(&mut blob, &mut views, bytes);
                    json!({ "name": name, "bufferView": view, "mimeType": mime })
                }
                ImageData::EmbeddedOffsetOverflow => {
                    views.push(json!({
                        "buffer": 0,
                        "byteOffset": u64::MAX - 1,
                        "byteLength": 4,
                    }));
                    json!({ "name": name, "bufferView": views.len() - 1, "mimeType": "image/png" })
                }
                ImageData::EmbeddedOutOfRange => {
                    views.push(json!({ "buffer": 0, "byteOffset": 0, "byteLength": 1_000_000 }));
                    json!({ "name": name, "bufferView": views.len() - 1, "mimeType": "image/png" })
                }
            }
        })
        .collect();
    pad4(&mut blob, 0);

    let mut doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": [primitive] }],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": blob.len() }],
    });
    if !model.materials.is_empty() {
        doc["materials"] = json!(model.materials);
    }
    if !images.is_empty() {
        let sources: Vec<usize> = if model.texture_sources.is_empty() {
            (0..images.len()).collect()
        } else {
            model.texture_sources.clone()
        };
        let textures: Vec<Value> = sources.iter().map(|s| json!({ "source": s })).collect();
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
    }
    (doc, blob)
}

fn write_image_files(dir: &Path, model: &Textured) {
    for image in &model.images {
        if let ImageData::File {
            written: Some((name, bytes)),
            ..
        } = image
        {
            fs::write(dir.join(name), bytes).expect("write image file");
        }
    }
}

/// Write `model.gltf`, `model.gltf.bin` and the model's image files into a fresh temp dir.
pub fn write_gltf(model: &Textured) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = write_gltf_into(dir.path(), "model.gltf", model);
    (dir, path)
}

/// Like [`write_gltf`], into an existing `dir` under `file_name`, so several models can
/// share image files.
pub fn write_gltf_into(dir: &Path, file_name: &str, model: &Textured) -> PathBuf {
    let (mut doc, blob) = build(model);
    let bin = format!("{file_name}.bin");
    doc["buffers"][0]["uri"] = json!(bin);
    let path = dir.join(file_name);
    fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).expect("write gltf");
    fs::write(dir.join(bin), blob).expect("write bin");
    write_image_files(dir, model);
    path
}

/// Write `model.glb` (JSON chunk plus BIN chunk holding the embedded images) and the
/// model's image files into a fresh temp dir.
pub fn write_glb(model: &Textured) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (doc, mut blob) = build(model);
    let mut json_chunk = serde_json::to_vec(&doc).expect("json");
    pad4(&mut json_chunk, b' ');
    pad4(&mut blob, 0);

    let total = 12 + 8 + json_chunk.len() + 8 + blob.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&blob);

    let path = dir.path().join("model.glb");
    fs::write(&path, glb).expect("write glb");
    write_image_files(dir.path(), model);
    (dir, path)
}
