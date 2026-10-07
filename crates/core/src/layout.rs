//! Keeping an app in the slot you learned it in.
//!
//! Slots never reshuffle. An app that is still running keeps its slot, an app that has
//! gone releases it, and the next app to appear takes the first free slot in order.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::item::Position;
use crate::slot::{Slot, Tab};

/// Every slot on every tab, in the order they get filled.
pub fn positions(tab_count: usize) -> impl Iterator<Item = Position> {
    (0..tab_count.min(Tab::MAX_COUNT))
        .filter_map(Tab::from_index)
        .flat_map(|tab| Slot::all().map(move |slot| Position::new(tab, slot)))
}

/// The last slot of the last tab, which becomes "More..." when everything is full.
pub fn overflow_position(tab_count: usize) -> Option<Position> {
    positions(tab_count).last()
}

/// Where each unpinned app currently sits.
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
    /// `reserved` holds the positions pinned items have claimed, which an app must give
    /// up even if it was there first.
    ///
    /// Returns the keys that did not fit anywhere.
    pub fn reconcile(
        &mut self,
        running: &[K],
        reserved: &HashSet<Position>,
        tab_count: usize,
    ) -> Vec<K> {
        let still_running: HashSet<&K> = running.iter().collect();
        self.placed
            .retain(|key, position| still_running.contains(key) && !reserved.contains(position));

        let mut taken: HashSet<Position> = reserved.clone();
        taken.extend(self.placed.values().copied());

        let mut free = positions(tab_count).filter(|position| !taken.contains(position));
        let mut unplaced = Vec::new();

        for key in running {
            if self.placed.contains_key(key) {
                continue;
            }
            match free.next() {
                Some(position) => {
                    self.placed.insert(key.clone(), position);
                }
                None => unplaced.push(key.clone()),
            }
        }

        unplaced
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

    fn nothing_reserved() -> HashSet<Position> {
        HashSet::new()
    }

    #[test]
    fn apps_fill_slots_in_order() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();

        memory.reconcile(&["a", "b", "c"], &nothing_reserved(), ONE_TAB);

        assert_eq!(memory.position_of(&"a"), Some(position(1, 0)));
        assert_eq!(memory.position_of(&"b"), Some(position(1, 1)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 2)));
    }

    #[test]
    fn an_app_closing_never_moves_the_others() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b", "c"], &nothing_reserved(), ONE_TAB);

        memory.reconcile(&["a", "c"], &nothing_reserved(), ONE_TAB);

        assert_eq!(memory.position_of(&"a"), Some(position(1, 0)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 2)));
        assert_eq!(memory.position_of(&"b"), None);
    }

    #[test]
    fn a_freed_slot_goes_to_the_next_app_to_appear() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b", "c"], &nothing_reserved(), ONE_TAB);
        memory.reconcile(&["a", "c"], &nothing_reserved(), ONE_TAB);

        memory.reconcile(&["a", "c", "d"], &nothing_reserved(), ONE_TAB);

        assert_eq!(memory.position_of(&"d"), Some(position(1, 1)));
        assert_eq!(memory.position_of(&"c"), Some(position(1, 2)));
    }

    #[test]
    fn a_reopened_app_is_treated_as_new() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b"], &nothing_reserved(), ONE_TAB);
        memory.reconcile(&["a"], &nothing_reserved(), ONE_TAB);

        memory.reconcile(&["a", "b"], &nothing_reserved(), ONE_TAB);

        assert_eq!(memory.position_of(&"b"), Some(position(1, 1)));
    }

    #[test]
    fn pinned_slots_are_skipped() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        let reserved = HashSet::from([position(1, 0), position(1, 1)]);

        memory.reconcile(&["a"], &reserved, ONE_TAB);

        assert_eq!(memory.position_of(&"a"), Some(position(1, 2)));
    }

    #[test]
    fn an_app_gives_up_a_slot_that_becomes_pinned() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a"], &nothing_reserved(), ONE_TAB);

        memory.reconcile(&["a"], &HashSet::from([position(1, 0)]), ONE_TAB);

        assert_eq!(memory.position_of(&"a"), Some(position(1, 1)));
    }

    #[test]
    fn apps_overflow_onto_the_next_tab() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        let running: Vec<String> = (0..Slot::COUNT + 2).map(|n| n.to_string()).collect();
        let keys: Vec<&str> = running.iter().map(String::as_str).collect();

        memory.reconcile(&keys, &nothing_reserved(), TWO_TABS);

        assert_eq!(memory.position_of(&keys[Slot::COUNT]), Some(position(2, 0)));
        assert_eq!(
            memory.position_of(&keys[Slot::COUNT + 1]),
            Some(position(2, 1))
        );
    }

    #[test]
    fn apps_past_the_last_slot_are_reported_rather_than_dropped_silently() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        let running: Vec<String> = (0..Slot::COUNT + 2).map(|n| n.to_string()).collect();
        let keys: Vec<&str> = running.iter().map(String::as_str).collect();

        let unplaced = memory.reconcile(&keys, &nothing_reserved(), ONE_TAB);

        assert_eq!(unplaced, [keys[Slot::COUNT], keys[Slot::COUNT + 1]]);
        assert_eq!(memory.len(), Slot::COUNT);
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
    fn the_overflow_slot_is_the_last_one_of_the_last_tab() {
        assert_eq!(
            overflow_position(TWO_TABS),
            Some(position(2, Slot::COUNT - 1))
        );
    }

    #[test]
    fn positions_never_run_past_the_number_keys() {
        let walked: Vec<Position> = positions(Tab::MAX_COUNT + 5).collect();

        assert_eq!(walked.len(), Slot::COUNT * Tab::MAX_COUNT);
    }

    #[test]
    fn at_finds_whatever_claimed_a_slot() {
        let mut memory: SlotMemory<&str> = SlotMemory::default();
        memory.reconcile(&["a", "b"], &nothing_reserved(), ONE_TAB);

        assert_eq!(memory.at(position(1, 1)), Some(&"b"));
        assert_eq!(memory.at(position(1, 7)), None);
    }
}
