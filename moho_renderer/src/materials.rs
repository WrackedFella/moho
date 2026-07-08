use moho_render_api::{MaterialGpu, MaterialKey, RenderMaterial};
use std::collections::HashMap;

/// MaterialTable deduplicates application-level material descriptors and
/// produces a compact, GPU-ready array of `MaterialGpu` values suitable for
/// upload as a storage buffer. It is intentionally simple and owned by the
/// renderer crate so the graphics backend controls GPU resource lifetime and
/// binding layout.
///
/// Usage:
/// - Create with `MaterialTable::new()`.
/// - Call `find_or_push(&impl RenderMaterial)` for each instance to get a
///   compact material index used by instance descriptors.
/// - When `is_dirty()` returns true upload `as_slice()` with
///   `RendererBackend::set_materials(...)` and then call `clear_dirty()`.
#[derive(Debug)]
pub struct MaterialTable {
    map: HashMap<MaterialKey, u32>,
    list: Vec<MaterialGpu>,
    dirty: bool,
}

impl MaterialTable {
    /// Create an empty `MaterialTable` ready to accept materials.
    pub fn new() -> Self {
        MaterialTable {
            map: HashMap::new(),
            list: Vec::new(),
            dirty: false,
        }
    }

    /// Return the index for the given material, inserting a new entry if
    /// necessary. Marks the table as dirty when a new material is added.
    /// The returned index is stable for the lifetime of the table unless the
    /// table is cleared.
    pub fn find_or_push<M: RenderMaterial>(&mut self, m: &M) -> u32 {
        let key = m.dedup_key();
        if let Some(&idx) = self.map.get(&key) {
            return idx;
        }
        let idx = self.list.len() as u32;
        self.map.insert(key, idx);
        self.list.push(m.to_gpu());
        self.dirty = true;
        idx
    }

    /// Return true if the table gained new entries since the last
    /// `clear_dirty()` call. Callers should upload `as_slice()` when this is
    /// true.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag after the caller has uploaded materials to the
    /// GPU.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Return the GPU-ready slice of materials for upload.
    pub fn as_slice(&self) -> &[MaterialGpu] {
        &self.list
    }

    /// Reset internal state (used by tests). Kept small to avoid exposing
    /// unnecessary API surface in the public renderer.
    #[allow(dead_code)]
    pub fn reset_for_tests(&mut self) {
        self.map.clear();
        self.list.clear();
        self.dirty = false;
    }
}

impl Default for MaterialTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use moho_core::materials::MaterialType;

    #[test]
    fn dedup_materials_basic() {
        let mut mt = MaterialTable::new();
        let m1 = MaterialType::Lambertian {
            albedo: Vec3::new(0.5, 0.25, 0.125),
        };
        let m2 = MaterialType::Lambertian {
            albedo: Vec3::new(0.5, 0.25, 0.125),
        };
        let m3 = MaterialType::Metal {
            albedo: Vec3::new(0.5, 0.25, 0.125),
            fuzz: 0.3,
        };

        let i1 = mt.find_or_push(&m1);
        let i2 = mt.find_or_push(&m2);
        let i3 = mt.find_or_push(&m3);

        assert_eq!(i1, i2, "identical lambertian materials should dedupe");
        assert_ne!(i1, i3, "metal with different fuzz should be distinct");
        assert!(mt.is_dirty());
        assert_eq!(mt.as_slice().len(), 2);
    }
}
