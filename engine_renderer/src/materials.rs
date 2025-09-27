use std::collections::HashMap;

use crate::MaterialGpu;

/// A compact, hashable key for material deduplication. We use the bit
/// patterns of f32 values (via to_bits) so the key implements `Eq` and
/// `Hash` safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MaterialKey {
    variant: u8,
    albedo_bits: [u32; 3],
    fuzz_bits: u32,
    ref_idx_bits: u32,
}

impl MaterialKey {
    fn from_material(m: &engine_core::materials::MaterialType) -> Self {
        match m {
            engine_core::materials::MaterialType::Lambertian { albedo } => MaterialKey {
                variant: 0,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: 0,
                ref_idx_bits: 0,
            },
            engine_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialKey {
                variant: 1,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: fuzz.to_bits(),
                ref_idx_bits: 0,
            },
            engine_core::materials::MaterialType::Dielectric { ref_indx } => MaterialKey {
                variant: 2,
                albedo_bits: [1u32, 1u32, 1u32],
                fuzz_bits: 0,
                ref_idx_bits: ref_indx.to_bits(),
            },
        }
    }
}

/// Incremental material table that assigns compact indices to distinct
/// materials and stores GPU-ready `MaterialGpu` values. It avoids re-creating
/// the GPU buffer unless a new material is added.
pub struct MaterialTable {
    map: HashMap<MaterialKey, u32>,
    list: Vec<MaterialGpu>,
    dirty: bool,
}

impl MaterialTable {
    pub fn new() -> Self {
        MaterialTable {
            map: HashMap::new(),
            list: Vec::new(),
            dirty: false,
        }
    }

    /// Return the index for the given material, inserting a new entry if
    /// necessary. Marks the table as dirty when a new material is added.
    pub fn find_or_push(&mut self, m: &engine_core::materials::MaterialType) -> u32 {
        let key = MaterialKey::from_material(m);
        if let Some(&idx) = self.map.get(&key) {
            return idx;
        }
        let idx = self.list.len() as u32;
        let mg = match m {
            engine_core::materials::MaterialType::Lambertian { albedo } => MaterialGpu {
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [0.0, 0.0, 0.0, 0.0],
            },
            engine_core::materials::MaterialType::Metal { albedo, fuzz } => MaterialGpu {
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [*fuzz, 0.0, 0.0, 0.0],
            },
            engine_core::materials::MaterialType::Dielectric { ref_indx } => MaterialGpu {
                albedo: [1.0, 1.0, 1.0, 0.0],
                params: [0.0, *ref_indx, 0.0, 0.0],
            },
        };
        self.map.insert(key, idx);
        self.list.push(mg);
        self.dirty = true;
        idx
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Return the GPU-ready slice of materials for upload.
    pub fn as_slice(&self) -> &[MaterialGpu] {
        &self.list
    }

    #[allow(dead_code)]
    pub fn reset_for_tests(&mut self) {
        self.map.clear();
        self.list.clear();
        self.dirty = false;
    }
}
