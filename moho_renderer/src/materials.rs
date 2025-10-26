use std::collections::HashMap;

use crate::MaterialGpu;

/// Small helper type used to create a hashable, byte-stable key for
/// `MaterialTable` deduplication. We compare materials by their bitwise
/// f32 representation (via `to_bits`) to avoid floating-point equality
/// pitfalls while retaining reproducible hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MaterialKey {
    variant: u8,
    albedo_bits: [u32; 3],
    fuzz_bits: u32,
    ref_idx_bits: u32,
}

impl MaterialKey {
    fn from_material(m: &moho_core::materials::MaterialType) -> Self {
        match m {
            moho_core::materials::MaterialType::Lambertian { albedo } => MaterialKey {
                variant: 0,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: 0,
                ref_idx_bits: 0,
            },
            moho_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialKey {
                variant: 1,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: fuzz.to_bits(),
                ref_idx_bits: 0,
            },
            moho_core::materials::MaterialType::Dielectric { ref_indx } => MaterialKey {
                variant: 2,
                albedo_bits: [1u32, 1u32, 1u32],
                fuzz_bits: 0,
                ref_idx_bits: ref_indx.to_bits(),
            },
        }
    }
}

/// MaterialTable deduplicates application-level material descriptors and
/// produces a compact, GPU-ready array of `MaterialGpu` values suitable for
/// upload as a storage buffer. It is intentionally simple and owned by the
/// renderer crate so the graphics backend controls GPU resource lifetime and
/// binding layout.
///
/// Usage:
/// - Create with `MaterialTable::new()`.
/// - Call `find_or_push(&MaterialType)` for each instance to get a compact
///   material index used by instance descriptors.
/// - When `is_dirty()` returns true upload `as_slice()` with
///   `RendererBackend::set_materials(...)` and then call `clear_dirty()`.
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
    pub fn find_or_push(&mut self, m: &moho_core::materials::MaterialType) -> u32 {
        let key = MaterialKey::from_material(m);
        if let Some(&idx) = self.map.get(&key) {
            return idx;
        }
        let idx = self.list.len() as u32;
        let mg = match m {
            moho_core::materials::MaterialType::Lambertian { albedo } => MaterialGpu {
                // params: [fuzz, ref_idx, is_transparent, unused]
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [0.0, 0.0, 0.0, 0.0],
            },
            moho_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialGpu {
                // Metals are opaque; store fuzz in params.x
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [*fuzz, 0.0, 0.0, 0.0],
            },
            moho_core::materials::MaterialType::Dielectric { ref_indx } => MaterialGpu {
                // Dielectrics are considered potentially transparent. We store
                // the refraction index in params.y and set params.z=1.0 to
                // signal transparency to CPU-side ordering logic.
                albedo: [1.0, 1.0, 1.0, 0.0],
                params: [0.0, *ref_indx, 1.0, 0.0],
            },
        };
        self.map.insert(key, idx);
        self.list.push(mg);
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
    use moho_core::materials::MaterialType;
    use glam::Vec3;

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

