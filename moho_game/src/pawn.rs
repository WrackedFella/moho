use crate::inventory::{Inventory, ResourceYield};

/// A gameplay component owning an inventory and (later) the capability to
/// act on the world — mining, movement intent, etc. A player OR an
/// AI-controlled entity owns a `Pawn`; this is intentionally distinct from
/// `PlayerController`, which is camera/movement-only.
#[derive(Debug, Default)]
pub struct Pawn {
    pub inventory: Inventory,
}

impl Pawn {
    pub fn deposit(&mut self, yield_: ResourceYield) {
        self.inventory.add(yield_.resource_id, yield_.amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposit_updates_matching_inventory_counter() {
        let mut pawn = Pawn::default();

        pawn.deposit(ResourceYield {
            resource_id: 3,
            amount: 1,
        });

        assert_eq!(pawn.inventory.count(3), 1);
    }
}
