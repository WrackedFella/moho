use std::collections::HashMap;

/// A resource amount produced by a mining action — the currency between "a
/// mine happened" and "inventory grew." `amount` stays a data field even
/// though every caller passes `1` today: the long-term sub-grid mining pivot
/// will yield fractional/variable amounts, and deposit code must not assume
/// "1 hit = 1 unit."
#[derive(Debug, Clone, Copy)]
pub struct ResourceYield {
    pub resource_id: u32,
    pub amount: u32,
}

/// Per-resource integer counters. Keyed on raw `resource_id`, matching the
/// existing per-block resource data — no separate item-id model yet.
#[derive(Debug, Default)]
pub struct Inventory {
    counts: HashMap<u32, u32>,
}

impl Inventory {
    pub fn add(&mut self, resource_id: u32, amount: u32) {
        *self.counts.entry(resource_id).or_insert(0) += amount;
    }

    pub fn count(&self, resource_id: u32) -> u32 {
        self.counts.get(&resource_id).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.counts.iter().map(|(&id, &amount)| (id, amount))
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_when_resource_never_added_returns_zero() {
        let inventory = Inventory::default();

        assert_eq!(inventory.count(42), 0);
    }

    #[test]
    fn add_accumulates_across_multiple_calls() {
        let mut inventory = Inventory::default();
        inventory.add(1, 3);

        inventory.add(1, 2);

        assert_eq!(inventory.count(1), 5);
    }

    #[test]
    fn add_keeps_different_resources_independent() {
        let mut inventory = Inventory::default();

        inventory.add(1, 5);
        inventory.add(2, 10);

        assert_eq!(inventory.count(1), 5);
        assert_eq!(inventory.count(2), 10);
    }

    #[test]
    fn is_empty_true_until_first_add() {
        let mut inventory = Inventory::default();
        assert!(inventory.is_empty());

        inventory.add(1, 1);

        assert!(!inventory.is_empty());
    }
}
