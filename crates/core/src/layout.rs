//! Keeping an app in the slot you learned it in.
//!
//! Running apps only ever land in slots marked as unpinned app slots. Nothing auto-fills
//! a blank slot, and nothing can displace a pinned one. An app that is placed keeps its
//! slot until it closes, at which point the slot is free for the next app to arrive.

use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

use crate::item::Position;
use crate::slot::{Slot, Tab};

/// Every slot on every tab, in reading order.
pub fn positions(tab_count: usize) -> impl Iterator<Item = Position> {
    (0..tab_count.min(Tab::MAX_COUNT))
        .filter_map(Tab::from_index)
        .flat_map(|tab| Slot::all().map(move |slot| Position::new(tab, slot)))
}

/// Which app is sitting in each unpinned app slot.
#[derive(Debug)]
pub struct SlotMemory<K> {
    placed: HashMap<K, Position>,
}

impl<K> Default for SlotMemory<K> {
    fn default() -> Self {
        Self {
            placed: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> SlotMemory<K> {
    /// Brings the memory in line with what is running now.
    ///
    /// `slots` are the unpinned app slots, in the order they should fill. Positions
    /// outside that list are never touched, so a pinned item cannot be displaced and a
    /// blank slot is never filled behind your back.
    ///
    /// Returns how many running apps did not fit.
    pub fn reconcile(&mut self, running: &[K], slots: &[Position]) -> usize {
        let still_running: HashSet<&K> = running.iter().collect();
        let open: HashSet<Position> = slots.iter().copied().collect();

        // an app that closed, or a slot that stopped being an unpinned one, lets go
        self.placed
            .retain(|key, position| still_running.contains(key) && open.contains(position));

        let taken: HashSet<Position> = self.placed.values().copied().collect();
        let mut free = slots.iter().filter(|position| !taken.contains(position));
        let mut overflow = 0;

        for key in running {
            if self.placed.contains_key(key) {
                continue;
            }
            match free.next() {
                Some(position) => {
                    self.placed.insert(key.clone(), *position);
                }
                None => overflow += 1,
            }
        }

        overflow
    }

    pub fn position_of(&self, key: &K) -> Option<Position> {
        self.placed.get(key).copied()
    }

    pub fn at(&self, position: Position) -> Option<&K> {
        self.placed
            .iter()
            .find(|(_, placed)| **placed == position)
            .map(|(key, _)| key)
    }

    pub fn len(&self) -> usize {
        self.placed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_TAB: usize = 1;
    const TWO_TABS: usize = 2;

    fn position(tab: usize, slot: usize) -> Position {
        Position::new(
            Tab::from_number(tab).expect("tab in range"),
            Slot::new(slot).expect("slot in range"),
        )
    }

    /// Three unpinned app slots, deliberately not the first three, so a test that
    /// accidentally auto-fills shows up.
    fn scattered_slots() -> Vec<Position> {
        vec![position(1, 1), position(1, 4), position(1, 6)]
    }

    #[test]
    fn apps_fill_the_unpinned_slots_in_order() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();

        let overflow = memory.reconcile(&["a", "b", "c"], &scattered_slots());

        assert_eq!(overflow, 0);
        assert_eq!(memory.position_of(&"a"), Some(position(1, 1)));
        assert_eq!(memory.position_of(&"b"), Some(position(1, 4)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 6)));
    }

    /// The whole point of the model change: a blank slot stays blank.
    #[test]
    fn nothing_lands_anywhere_without_an_unpinned_slot() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();

        let overflow = memory.reconcile(&["a", "b"], &[]);

        assert_eq!(overflow, 2);
        assert!(memory.is_empty());
    }

    #[test]
    fn an_app_closing_never_moves_the_others() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b", "c"], &scattered_slots());

        memory.reconcile(&["a", "c"], &scattered_slots());

        assert_eq!(memory.position_of(&"a"), Some(position(1, 1)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 6)));
        assert_eq!(memory.position_of(&"b"), None);
    }

    #[test]
    fn a_freed_slot_goes_to_the_next_app_to_appear() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b", "c"], &scattered_slots());
        memory.reconcile(&["a", "c"], &scattered_slots());

        memory.reconcile(&["a", "c", "d"], &scattered_slots());

        assert_eq!(memory.position_of(&"d"), Some(position(1, 4)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 6)));
    }

    #[test]
    fn a_reopened_app_is_treated_as_new() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b"], &scattered_slots());
        memory.reconcile(&["a"], &scattered_slots());

        memory.reconcile(&["a", "b"], &scattered_slots());

        assert_eq!(memory.position_of(&"b"), Some(position(1, 4)));
    }

    /// Turning an unpinned slot into something else has to evict whatever sat there, but
    /// an app whose slot is still unpinned keeps it.
    #[test]
    fn an_app_lets_go_of_a_slot_that_stops_being_unpinned() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b"], &scattered_slots());

        let fewer = vec![position(1, 4), position(1, 6)];
        memory.reconcile(&["a", "b"], &fewer);

        assert_eq!(memory.position_of(&"b"), Some(position(1, 4)));
        assert_eq!(memory.position_of(&"a"), Some(position(1, 6)));
    }

    #[test]
    fn apps_past_the_last_unpinned_slot_are_counted() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();

        let overflow = memory.reconcile(&["a", "b", "c", "d", "e"], &scattered_slots());

        assert_eq!(overflow, 2);
        assert_eq!(memory.len(), 3);
    }

    #[test]
    fn unpinned_slots_can_span_tabs() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        let slots = vec![position(1, 0), position(2, 7)];

        memory.reconcile(&["a", "b"], &slots);

        assert_eq!(memory.position_of(&"b"), Some(position(2, 7)));
    }

    #[test]
    fn positions_walk_tabs_then_slots() {
        let walked: Vec<Position> = positions(TWO_TABS).collect();

        assert_eq!(walked.len(), Slot::COUNT * TWO_TABS);
        assert_eq!(walked[0], position(1, 0));
        assert_eq!(walked[Slot::COUNT - 1], position(1, Slot::COUNT - 1));
        assert_eq!(walked[Slot::COUNT], position(2, 0));
    }

    #[test]
    fn positions_never_run_past_the_number_keys() {
        let walked: Vec<Position> = positions(Tab::MAX_COUNT + 5).collect();

        assert_eq!(walked.len(), Slot::COUNT * Tab::MAX_COUNT);
    }

    #[test]
    fn at_finds_whatever_claimed_a_slot() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b"], &scattered_slots());

        assert_eq!(memory.at(position(1, 4)), Some(&"b"));
        assert_eq!(memory.at(position(1, 7)), None);
        assert_eq!(memory.at(position(1, 0)), None);
    }

    #[test]
    fn one_tab_of_positions_is_eight_slots() {
        assert_eq!(positions(ONE_TAB).count(), Slot::COUNT);
    }
}
