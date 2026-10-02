use bytemuck::{Pod, Zeroable};

/// GPU-visible material layout: two vec4-sized fields to satisfy WGSL
/// storage-buffer alignment and make the CPU representation match WGSL.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialGpu {
    pub albedo: [f32; 4],
    pub params: [f32; 4],
}

impl MaterialGpu {
    /// Return true if this material was marked as potentially
    /// transparent by the application (params[2] > 0.0).
    pub fn is_transparent(&self) -> bool {
        self.params[2] > 0.0
    }
}

/// Byte-stable, hashable key for `MaterialTable` deduplication. Compares
/// materials by their bitwise f32 representation (via `to_bits`) to avoid
/// floating-point equality pitfalls while retaining reproducible hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialKey {
    pub variant: u8,
    pub albedo_bits: [u32; 3],
    pub fuzz_bits: u32,
    pub ref_idx_bits: u32,
    /// Extra color / parameter bits used by multi-color materials
    /// (e.g. a terrain material's secondary albedo). Zero otherwise.
    pub extra_bits: [u32; 3],
}

/// Trait representing types that can be converted into a GPU-ready material
/// descriptor and a stable dedup key.
///
/// Implemented by game-domain material types (e.g. `MaterialType`) so the
/// renderer's `MaterialTable` can deduplicate and upload materials without
/// naming those concrete types.
pub trait RenderMaterial {
    fn to_gpu(&self) -> MaterialGpu;
    fn dedup_key(&self) -> MaterialKey;
}
