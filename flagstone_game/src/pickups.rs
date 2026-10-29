//! Recent pickups, kept so the HUD can show a fading popup for each.

use crate::inventory::ResourceYield;

/// How many ticks a pickup stays listed.
pub const PICKUP_POPUP_TICKS: u32 = 2 * crate::TICK_HZ;

/// One listed pickup and how long it has left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickupEntry {
    pub resource_id: u32,
    pub amount: u32,
    pub ticks_left: u32,
}

/// The pickups still worth showing, oldest first.
#[derive(Debug, Default)]
pub struct PickupFeed {}

impl PickupFeed {
    pub fn push(&mut self, _yield_: ResourceYield) {}

    pub fn tick(&mut self) {}

    pub fn entries(&self) -> &[PickupEntry] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yield_of(resource_id: u32, amount: u32) -> ResourceYield {
        ResourceYield {
            resource_id,
            amount,
        }
    }

    #[test]
    fn a_pushed_pickup_is_listed_then_expires_after_its_life() {
        let mut feed = PickupFeed::default();

        feed.push(yield_of(7, 1));

        assert_eq!(
            feed.entries(),
            [PickupEntry {
                resource_id: 7,
                amount: 1,
                ticks_left: PICKUP_POPUP_TICKS
            }]
        );
        for _ in 0..PICKUP_POPUP_TICKS - 1 {
            feed.tick();
        }
        assert_eq!(
            feed.entries().len(),
            1,
            "still listed one tick before expiry"
        );
        assert_eq!(feed.entries()[0].ticks_left, 1);
        feed.tick();
        assert!(feed.entries().is_empty(), "gone once its life is spent");
    }

    #[test]
    fn two_pickups_are_listed_together() {
        let mut feed = PickupFeed::default();

        feed.push(yield_of(7, 1));
        feed.tick();
        feed.push(yield_of(7, 1));

        let listed: Vec<(u32, u32, u32)> = feed
            .entries()
            .iter()
            .map(|e| (e.resource_id, e.amount, e.ticks_left))
            .collect();
        assert_eq!(
            listed,
            [(7, 1, PICKUP_POPUP_TICKS - 1), (7, 1, PICKUP_POPUP_TICKS)],
            "two separate entries, not merged, oldest first"
        );
    }

    #[test]
    fn a_tick_expires_only_the_entries_whose_life_ran_out() {
        let mut feed = PickupFeed::default();

        feed.push(yield_of(7, 1));
        feed.tick();
        feed.push(yield_of(8, 2));
        for _ in 0..PICKUP_POPUP_TICKS - 1 {
            feed.tick();
        }

        assert_eq!(
            feed.entries(),
            [PickupEntry {
                resource_id: 8,
                amount: 2,
                ticks_left: 1
            }]
        );
    }

    #[test]
    fn the_oldest_pickup_is_dropped_past_the_cap() {
        let mut feed = PickupFeed::default();

        for id in 1..=9 {
            feed.push(yield_of(id, 1));
        }

        let ids: Vec<u32> = feed.entries().iter().map(|e| e.resource_id).collect();
        assert_eq!(ids, [2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
