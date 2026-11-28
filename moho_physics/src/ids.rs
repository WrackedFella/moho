use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};

/// A dynamic body in a `PhysicsWorld`, as returned by `add_dynamic_sphere` and
/// `add_dynamic_cuboid`. Valid only in the world that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub(crate) RigidBodyHandle);

/// A collider in a `PhysicsWorld`, as named by `Impact::collider`, `Ground::collider` and
/// `PhysicsWorld::world_mesh_collider`. Valid only in the world that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderId(pub(crate) ColliderHandle);
