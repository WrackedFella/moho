use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};

/// A dynamic body in a `PhysicsWorld`, as returned by `add_dynamic_sphere` and
/// `add_dynamic_cuboid`. Valid only in the world that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(pub(crate) RigidBodyHandle);

/// A collider in a `PhysicsWorld`, as named by `Impact::collider`, `Ground::collider` and
/// `PhysicsWorld::world_mesh_collider`. Valid only in the world that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderId(pub(crate) ColliderHandle);

#[cfg(test)]
impl ColliderId {
    /// An id no world issues, for tests that need a collider but never look it up.
    pub(crate) fn invalid() -> Self {
        Self(ColliderHandle::invalid())
    }

    /// The id of collider slot `index`, generation 0.
    pub(crate) fn from_index(index: u32) -> Self {
        Self(ColliderHandle::from_raw_parts(index, 0))
    }

    /// The slot index this id names.
    pub(crate) fn index(self) -> u32 {
        self.0.into_raw_parts().0
    }
}
