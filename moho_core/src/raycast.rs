use crate::voxel::{BlockPos, VoxelGrid};
use glam::{IVec3, Vec3};

/// Result of a raycast operation
#[derive(Debug, Clone, Copy)]
pub struct RaycastResult {
    /// The position of the block that was hit
    pub block_pos: BlockPos,
    /// The face of the block that was hit (normal)
    pub normal: IVec3,
    /// The exact world position of the intersection
    pub position: Vec3,
    /// Distance from ray origin
    pub distance: f32,
}

/// Perform a raycast against the voxel grid
///
/// Uses a fast voxel traversal algorithm (Amanatides & Woo)
pub fn raycast(
    grid: &VoxelGrid,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<RaycastResult> {
    // Normalize direction
    let dir = direction.normalize();

    // Current voxel position
    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    // Step direction
    let step_x = if dir.x > 0.0 { 1 } else { -1 };
    let step_y = if dir.y > 0.0 { 1 } else { -1 };
    let step_z = if dir.z > 0.0 { 1 } else { -1 };

    // Distance to next voxel boundary
    let mut t_max_x = if step_x > 0 {
        (x as f32 + 1.0 - origin.x) / dir.x
    } else {
        (x as f32 - origin.x) / dir.x
    };

    let mut t_max_y = if step_y > 0 {
        (y as f32 + 1.0 - origin.y) / dir.y
    } else {
        (y as f32 - origin.y) / dir.y
    };

    let mut t_max_z = if step_z > 0 {
        (z as f32 + 1.0 - origin.z) / dir.z
    } else {
        (z as f32 - origin.z) / dir.z
    };

    // Distance to traverse one voxel
    let t_delta_x = (1.0 / dir.x).abs();
    let t_delta_y = (1.0 / dir.y).abs();
    let t_delta_z = (1.0 / dir.z).abs();

    // Face normal of the last entered voxel
    let mut normal = IVec3::ZERO;

    // Traversal loop
    let mut distance = 0.0;

    while distance <= max_distance {
        // Check if current voxel is solid
        let pos = IVec3::new(x, y, z);
        if let Some(block) = grid.get_block(&pos) {
            if block.material_id != 0 {
                // Assuming 0 is air/empty
                return Some(RaycastResult {
                    block_pos: pos,
                    normal,
                    position: origin + dir * distance,
                    distance,
                });
            }
        }

        // Move to next voxel
        if t_max_x < t_max_y {
            if t_max_x < t_max_z {
                x += step_x;
                distance = t_max_x;
                t_max_x += t_delta_x;
                normal = IVec3::new(-step_x, 0, 0);
            } else {
                z += step_z;
                distance = t_max_z;
                t_max_z += t_delta_z;
                normal = IVec3::new(0, 0, -step_z);
            }
        } else {
            if t_max_y < t_max_z {
                y += step_y;
                distance = t_max_y;
                t_max_y += t_delta_y;
                normal = IVec3::new(0, -step_y, 0);
            } else {
                z += step_z;
                distance = t_max_z;
                t_max_z += t_delta_z;
                normal = IVec3::new(0, 0, -step_z);
            }
        }
    }

    None
}
