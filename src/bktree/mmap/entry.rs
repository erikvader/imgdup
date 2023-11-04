use derivative::Derivative;
use rkyv::{Archive, Serialize};

use crate::imghash::hamming::Distance;

use super::{bktree::BKNode, file_array::Ref};

// TODO: static assert that this is outside of Hamming::min_distance and
// hamming::max_distance. Also greater than hamming::max_distance to make sure it gets
// sorted last.
const ENTRY_KEY_UNUSED: Distance = Distance::MAX;

#[derive(Serialize, Archive, PartialEq, Eq, Derivative)]
#[derivative(Clone(bound = ""))]
#[archive(check_bytes)]
pub(super) struct Entry {
    pub key: Distance,
    pub value: Ref<BKNode>,
}

pub(super) fn entry_get(
    entries: &[ArchivedEntry],
    key: Distance,
) -> Option<&ArchivedEntry> {
    assert_ne!(key, ENTRY_KEY_UNUSED);
    entries
        .binary_search_by(|probe| probe.key.cmp(&key))
        .ok()
        .map(|i| &entries[i])
}

pub(super) fn entry_add(entries: &mut [ArchivedEntry], entry: Entry) -> Option<usize> {
    if entry_is_full(entries) {
        return None;
    }

    match entries.binary_search_by(|probe| probe.key.cmp(&entry.key)) {
        Err(i) if (..entries.len()).contains(&i) => {
            entries[i..].rotate_right(1);
            let target = &mut entries[i];
            assert_eq!(ENTRY_KEY_UNUSED, target.key);
            *target = entry.into();
            Some(i)
        }
        _ => None,
    }
}

pub(super) fn entry_is_full(entries: &[ArchivedEntry]) -> bool {
    entries
        .last()
        .map(|ent| ent.key != ENTRY_KEY_UNUSED)
        .unwrap_or(true)
}

pub(super) fn entry_create(limit: usize) -> Vec<Entry> {
    let mut children = Vec::new();
    children.resize_with(limit, Default::default);
    children
}

pub(super) fn entry_used(entries: &[ArchivedEntry]) -> &[ArchivedEntry] {
    const SEARCH_KEY: Distance = ENTRY_KEY_UNUSED - 1;
    match entries.binary_search_by(|probe| probe.key.cmp(&SEARCH_KEY)) {
        Ok(_) => panic!("the search key should not exist"),
        Err(i) if (..entries.len()).contains(&i) => {
            let target = &entries[i];
            assert_eq!(ENTRY_KEY_UNUSED, target.key);
            &entries[..i]
        }
        Err(_) => entries,
    }
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            key: ENTRY_KEY_UNUSED,
            value: Ref::null(),
        }
    }
}

impl From<Entry> for ArchivedEntry {
    fn from(value: Entry) -> Self {
        Self {
            key: value.key,
            value: value.value.into(),
        }
    }
}

impl From<&ArchivedEntry> for Entry {
    fn from(value: &ArchivedEntry) -> Self {
        Self {
            key: value.key,
            value: value.value,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn entry(key: Distance) -> Entry {
        Entry {
            key,
            value: Ref::null(),
        }
    }

    #[test]
    fn entries() {
        let entries = entry_create(5);
        assert_eq!(5, entries.len());
        assert!(entries.iter().all(|e| e == &Entry::default()));

        let mut archived: Vec<ArchivedEntry> =
            entries.into_iter().map(Into::into).collect();
        assert!(!entry_is_full(&archived));
        assert!(entry_get(&archived, 2).is_none());
        assert_eq!(0, entry_used(&archived).len());

        assert_eq!(Some(0), entry_add(&mut archived, entry(2)));
        assert_eq!(Some(1), entry_add(&mut archived, entry(4)));
        assert_eq!(Some(0), entry_add(&mut archived, entry(1)));
        assert_eq!(Some(2), entry_add(&mut archived, entry(3)));

        assert_eq!(None, entry_add(&mut archived, entry(3)));

        assert!(!entry_is_full(&archived));
        assert_eq!(4, entry_used(&archived).len());
        assert_eq!(Some(4), entry_add(&mut archived, entry(7)));
        assert!(entry_is_full(&archived));
        assert_eq!(5, entry_used(&archived).len());
        assert_eq!(None, entry_add(&mut archived, entry(8)));

        assert!(entry_get(&archived, 7).is_some());
        assert!(entry_get(&archived, 8).is_none());
    }
}
