use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};
use gltf::json::{Value, deserialize};

/// A named, mesh-less glTF node: a point in the level that game code gives meaning to.
///
/// Scale is dropped. A zero-scale world matrix decomposes to a NaN rotation (not
/// guarded); a sheared one gives the closest-fit rotation.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelMarker {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    /// The node's `extras` object, one entry per key. Strings are kept as is; any
    /// other JSON value is its compact JSON text.
    pub properties: BTreeMap<String, String>,
}

/// The marker for `node` at `world`, or `None` if it has a mesh or no name.
pub(crate) fn from_node(node: &gltf::Node<'_>, world: Mat4) -> Option<LevelMarker> {
    if node.mesh().is_some() {
        return None;
    }
    let name = node.name()?;
    let (_, rotation, position) = world.to_scale_rotation_translation();
    Some(LevelMarker {
        name: name.to_owned(),
        position,
        rotation,
        properties: properties(node, name),
    })
}

fn properties(node: &gltf::Node<'_>, name: &str) -> BTreeMap<String, String> {
    let Some(raw) = node.extras() else {
        return BTreeMap::new();
    };
    match deserialize::from_str::<Value>(raw.get()) {
        Ok(Value::Object(entries)) => entries
            .into_iter()
            .map(|(key, value)| match value {
                Value::String(text) => (key, text),
                other => (key, other.to_string()),
            })
            .collect(),
        Ok(_) => {
            tracing::warn!(marker = name, "marker extras are not an object; ignoring");
            BTreeMap::new()
        }
        Err(error) => {
            tracing::warn!(marker = name, %error, "marker extras are unreadable; ignoring");
            BTreeMap::new()
        }
    }
}
