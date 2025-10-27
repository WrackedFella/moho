use crate::events::Event;
use glam::{IVec3, Vec3};
use std::any::Any;

/// Physics simulation events
#[derive(Clone, Debug)]
pub enum PhysicsEvent {
    /// Player position/velocity updated
    PlayerMoved { position: Vec3, velocity: Vec3 },

    /// Player collided with surface
    PlayerCollided {
        surface_normal: Vec3,
        impact_force: f32,
    },

    /// Player jumped
    PlayerJumped { initial_velocity: f32 },

    /// Player landed on ground
    PlayerLanded { fall_distance: f32, damage: u32 },

    /// Voxel destroyed/modified
    VoxelDestroyed { position: IVec3, material_type: u8 },

    /// Physics collision detected
    CollisionDetected {
        entity_a: u64,
        entity_b: u64,
        impact_point: Vec3,
    },
}

impl Event for PhysicsEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn should_record(&self) -> bool {
        // Don't record frequent movement updates
        !matches!(self, PhysicsEvent::PlayerMoved { .. })
    }
}
