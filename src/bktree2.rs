use std::path::Path;
use std::pin::Pin;

use self::entry::*;
use rkyv::validation::validators::DefaultValidator;
use rkyv::vec::ArchivedVec;
use rkyv::with::AsBox;
use rkyv::{Archive, CheckBytes, Serialize};

use crate::{
    file_array::{self, ArchivedRef, FileArray, FileArraySerializer, Ref},
    imghash::hamming::{Distance, Hamming},
};

pub struct AnyValue; // TODO: panic if its archived value is read or something cool

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
struct Meta {
    root: Ref,
    // TODO: save some identifier for S
}

impl Default for Meta {
    fn default() -> Self {
        Self { root: Ref::null() }
    }
}

impl ArchivedMeta {
    fn root(self: Pin<&mut Self>) -> Pin<&mut ArchivedRef> {
        unsafe { self.map_unchecked_mut(|m| &mut m.root) }
    }
}

const DEFAULT_CHILDREN_LIMIT: usize = 20;

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
struct BKNode<S> {
    hash: Hamming,
    #[with(AsBox)]
    value: S,
    removed: bool,
    children: Vec<Entry>,
    next_sibling: Ref,
}

impl<S> BKNode<S> {
    fn new(hash: Hamming, value: S, children_limit: usize) -> Self {
        Self {
            hash,
            value,
            children: entry_create(children_limit),
            removed: false,
            next_sibling: Ref::null(),
        }
    }
}

impl<S> ArchivedBKNode<S>
where
    S: Archive,
{
    fn pin_mut_children(self: Pin<&mut Self>) -> Pin<&mut ArchivedVec<ArchivedEntry>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.children) }
    }
}

pub struct BKTree<S> {
    db: FileArray,
    _src: std::marker::PhantomData<S>,
}

impl<S> BKTree<S> {
    // TODO: Somehow allow opening without caring what S is. Handy for rebuilding or
    // collecting stats.
    pub fn from_file(file: impl AsRef<Path>) -> file_array::Result<Self> {
        let db = FileArray::new(file)?;
        Self::new(db)
    }

    fn new(mut db: FileArray) -> file_array::Result<Self> {
        if db.is_empty() {
            db.add_one(&Meta::default())?;
        }

        Ok(Self {
            db,
            _src: std::marker::PhantomData,
        })
    }

    fn root(&self) -> file_array::Result<Ref> {
        let meta_ref = FileArray::ref_to_first::<Meta>();
        let meta = self.db.get::<Meta>(meta_ref)?;
        Ok(meta.root.to_unarchived())
    }

    fn set_root(&mut self, new_root: Ref) -> file_array::Result<()> {
        let meta_ref = FileArray::ref_to_first::<Meta>();
        let meta = self.db.get_mut::<Meta>(meta_ref)?;
        meta.root().set(new_root);
        Ok(())
    }
}

impl<S> BKTree<S>
where
    // TODO: are both needed for all operations? Getters should only need Archive
    S: Serialize<FileArraySerializer> + Archive,
    S::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    // pub fn count_nodes(&mut self) -> heap::Result<(usize, usize)> {
    //     let mut alive = 0;
    //     let mut dead = 0;
    //     self.for_each_internal(
    //         |_, node| {
    //             if node.value.is_some() {
    //                 alive += 1;
    //             } else {
    //                 dead += 1;
    //             }
    //             false
    //         },
    //         |_, _| (),
    //     )?;
    //     Ok((alive, dead))
    // }

    // // TODO: this is just add_all with a list of size 1
    // pub fn add(&mut self, hash: Hamming, value: S) -> heap::Result<()> {
    //     if self.root().is_null() {
    //         let root = self.db.allocate(BKNode::new(hash, value))?;
    //         self.set_root(root);
    //     } else {
    //         self.add_internal_new(hash, value)?;
    //     }
    //     self.db.checkpoint()?;
    //     Ok(())
    // }

    // pub fn add_all(
    //     &mut self,
    //     items: impl IntoIterator<Item = (Hamming, S)>,
    // ) -> heap::Result<()> {
    //     let mut items = items.into_iter();
    //     if let Some((hash, value)) = items.next() {
    //         if self.root().is_null() {
    //             let root = self.db.allocate(BKNode::new(hash, value))?;
    //             self.set_root(root);
    //         } else {
    //             self.add_internal_new(hash, value)?;
    //         }
    //     }

    //     for (hash, value) in items {
    //         self.add_internal_new(hash, value)?;
    //     }
    //     self.db.checkpoint()?;
    //     Ok(())
    // }

    fn add_internal(
        &mut self,
        mut cur_ref: Ref,
        hash: Hamming,
        value: S,
    ) -> file_array::Result<()> {
        assert!(!cur_ref.is_null());
        loop {
            // let cur_node = self.db.get::<BKNode<S>>(cur_ref)?;
            let cur_node = self.db.get_mut::<BKNode<S>>(cur_ref)?;
            let dist = cur_node.hash.to_unarchived().distance_to(hash);
            let mut children = cur_node.pin_mut_children().pin_mut_slice();

            // match entry_get(&children, dist) {
            //     Some(entry) => cur_ref = entry.value.to_unarchived(),
            //     None if !cur_node.next_sibling.is_null() => {
            //         cur_ref = cur_node.next_sibling.to_unarchived()
            //     }
            //     None => {
            //         let new_node = BKNode::new(hash, value, DEFAULT_CHILDREN_LIMIT);
            //         // let new_node = self.db.add_one(&new_node)?;
            //         // match entry_add(children, Entry {key: dist, value: })
            //     }
            // }
            // if let Some(&child_ref) = cur_node.children.get(&dist) {
            //     cur_ref = child_ref;
            // } else {
            //     let new_ref = allocator(self, cur_ref)?;
            //     let cur_node = self
            //         .db
            //         .deref_mut(cur_ref)?
            //         .expect("the previous deref worked");

            //     cur_node.children.insert(dist, new_ref);
            //     break;
            // }
        }

        Ok(())
    }

    // fn add_internal_new(&mut self, hash: Hamming, value: S) -> heap::Result<()> {
    //     self.add_internal(self.root(), hash, |selff, parent_ref| {
    //         selff
    //             .db
    //             .allocate_local(parent_ref, BKNode::new(hash, value))
    //     })
    // }

    // // TODO: is it possible to somehow make this use `for_each_internal`?
    // pub fn find_within<F>(
    //     &mut self,
    //     hash: Hamming,
    //     within: Distance,
    //     mut visit: F,
    // ) -> heap::Result<()>
    // where
    //     F: FnMut(Hamming, &S),
    // {
    //     if self.root().is_null() {
    //         return Ok(());
    //     }

    //     let mut stack = vec![self.root()];
    //     while let Some(cur_ref) = stack.pop() {
    //         let cur_node = self.db.deref(cur_ref)?.expect("should have a value");
    //         let dist = cur_node.hash.distance_to(hash);
    //         if dist <= within {
    //             if let Some(value) = &cur_node.value {
    //                 visit(cur_node.hash, value);
    //             }
    //         }

    //         for i in dist.saturating_sub(within)..=dist.saturating_add(within) {
    //             if let Some(child_ref) = cur_node.children.get(&i) {
    //                 stack.push(*child_ref);
    //             }
    //         }
    //     }

    //     Ok(())
    // }

    // pub fn remove_any_of<P>(&mut self, mut predicate: P) -> heap::Result<()>
    // where
    //     P: FnMut(Hamming, &S) -> bool,
    // {
    //     self.for_each_internal(
    //         |_, node| {
    //             node.value
    //                 .as_ref()
    //                 .is_some_and(|value| predicate(node.hash, value))
    //         },
    //         |_, node| node.value = None,
    //     )?;
    //     self.db.checkpoint()?;
    //     Ok(())
    // }

    // pub fn for_each<F>(&mut self, mut visit: F) -> heap::Result<()>
    // where
    //     F: FnMut(Hamming, &S),
    // {
    //     self.for_each_internal(
    //         |_, node| {
    //             if let Some(value) = &node.value {
    //                 visit(node.hash, value);
    //             }
    //             false
    //         },
    //         |_, _| (),
    //     )
    // }

    // fn for_each_internal<F, M>(
    //     &mut self,
    //     mut filter: F,
    //     mut modifier: M,
    // ) -> heap::Result<()>
    // where
    //     F: FnMut(Ref, &BKNode<S>) -> bool,
    //     M: FnMut(Ref, &mut BKNode<S>),
    // {
    //     let mut stack = Vec::new();
    //     if !self.root().is_null() {
    //         stack.push(self.root());
    //     }

    //     while let Some(cur_ref) = stack.pop() {
    //         let cur_node = self.db.deref(cur_ref)?.expect("should have a value");
    //         stack.extend(cur_node.children.values());

    //         if filter(cur_ref, &cur_node) {
    //             let cur_node =
    //                 self.db.deref_mut(cur_ref)?.expect("previous deref worked");
    //             modifier(cur_ref, cur_node);
    //         }
    //     }

    //     Ok(())
    // }

    // pub fn rebuild(&mut self) -> heap::Result<(usize, usize)> {
    //     let mut dead = 0;
    //     let mut alive = 0;
    //     if self.root().is_null() {
    //         return Ok((alive, dead));
    //     }

    //     // NOTE: make sure everything is on disk to make a reversal possible, in case
    //     // anything fails
    //     self.db.flush()?;

    //     let mut stack: Vec<Ref> = vec![self.root()];
    //     self.set_root(Ref::null());

    //     while let Some(cur_ref) = stack.pop() {
    //         let cur_node = self.db.deref_mut(cur_ref)?.expect("should exist");
    //         stack.extend(cur_node.children.drain().map(|(_, child_ref)| child_ref));

    //         if cur_node.value.is_some() {
    //             alive += 1;
    //             let hash = cur_node.hash;
    //             let root = self.root();
    //             if root.is_null() {
    //                 self.set_root(cur_ref);
    //             } else {
    //                 self.add_internal(root, hash, |_, _| Ok(cur_ref))?;
    //             }
    //         } else {
    //             dead += 1;
    //             self.db.remove(cur_ref)?;
    //         }
    //     }

    //     self.db.checkpoint()?;

    //     Ok((alive, dead))
    // }
}

#[cfg(test)]
mod test {
    use std::{collections::HashSet, path::PathBuf};

    use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};

    use super::*;

    //     fn value(path: impl Into<PathBuf>) -> PathBuf {
    //         path.into()
    //     }

    //     fn contents(tree: &mut BKTree<PathBuf>) -> heap::Result<Vec<(Hamming, String)>> {
    //         let mut all = Vec::new();
    //         tree.for_each(|ham, val| {
    //             all.push((ham, val.clone().into_os_string().into_string().unwrap()))
    //         })?;
    //         all.sort();
    //         Ok(all)
    //     }

    //     #[test]
    //     fn add() -> heap::Result<()> {
    //         let mut tree = BKTree::in_memory()?;
    //         tree.add(Hamming(0b101), value("5_1"))?;
    //         tree.add(Hamming(0b101), value("5_2"))?;
    //         tree.add(Hamming(0b100), value("4"))?;

    //         let all = contents(&mut tree)?;

    //         assert_eq!(
    //             vec![
    //                 (Hamming(0b100), "4".to_string()),
    //                 (Hamming(0b101), "5_1".to_string()),
    //                 (Hamming(0b101), "5_2".to_string()),
    //             ],
    //             all
    //         );

    //         let mut closest = Vec::new();
    //         tree.find_within(Hamming(0b101), 0, |_, val| closest.push(val.clone()))?;
    //         closest.sort();
    //         let answer: Vec<PathBuf> = vec!["5_1".into(), "5_2".into()];
    //         assert_eq!(answer, closest);

    //         Ok(())
    //     }

    //     #[test]
    //     fn remove() -> heap::Result<()> {
    //         let mut tree = BKTree::in_memory()?;
    //         tree.add(Hamming(0b101), value("5_1"))?;
    //         tree.add(Hamming(0b101), value("5_2"))?;
    //         tree.add(Hamming(0b100), value("4"))?;

    //         let rem: HashSet<PathBuf> = HashSet::from(["5_1".into()]);
    //         tree.remove_any_of(|_, p| rem.contains(p))?;

    //         let all = contents(&mut tree)?;

    //         assert_eq!(
    //             vec![
    //                 (Hamming(0b100), "4".to_string()),
    //                 (Hamming(0b101), "5_2".to_string()),
    //             ],
    //             all
    //         );

    //         assert_eq!((2, 1), tree.count_nodes()?);
    //         tree.rebuild()?;
    //         assert_eq!((2, 0), tree.count_nodes()?);

    //         assert_eq!(
    //             vec![
    //                 (Hamming(0b100), "4".to_string()),
    //                 (Hamming(0b101), "5_2".to_string()),
    //             ],
    //             all
    //         );

    //         Ok(())
    //     }

    //     #[test]
    //     fn find_within_large() -> heap::Result<()> {
    //         let seed: u64 = rand::random();
    //         println!("Using seed: {}", seed);
    //         let mut rng = SmallRng::seed_from_u64(seed);

    //         let within = 5;
    //         let num_within = 100;
    //         let num_dups = 30;
    //         let total = 1_000;

    //         let search_hash: Hamming = rng.gen();
    //         let indices_within: HashSet<usize> =
    //             rand::seq::index::sample(&mut rng, total, num_within)
    //                 .into_iter()
    //                 .collect();

    //         let mut tree = BKTree::in_memory()?;
    //         let mut key = Vec::new();
    //         for i in 0..total {
    //             let hash = if indices_within.contains(&i) {
    //                 search_hash.random_within(&mut rng, within)
    //             } else {
    //                 search_hash.random_outside(&mut rng, within)
    //             };

    //             tree.add(hash, value(i.to_string()))?;

    //             if search_hash.distance_to(hash) <= within {
    //                 key.push(hash);
    //             }
    //         }

    //         {
    //             let mut dups = Vec::with_capacity(num_dups);
    //             for hash in key.choose_multiple(&mut rng, num_dups) {
    //                 tree.add(*hash, value("dup"))?;
    //                 dups.push(*hash);
    //             }
    //             key.extend(dups);
    //         }

    //         assert_eq!(num_dups + num_within, key.len());

    //         let mut closest = Vec::new();
    //         tree.find_within(search_hash, within, |hash, _| closest.push(hash))?;

    //         assert_eq!(key.len(), closest.len());

    //         closest.sort();
    //         key.sort();
    //         assert_eq!(key, closest);

    //         Ok(())
    //     }
}

// TODO: extract into its own file and fix imports
mod entry {
    use super::*;

    // TODO: static assert that this is outside of Hamming::min_distance and hamming max
    const ENTRY_KEY_UNUSED: Distance = Distance::MAX;

    #[derive(Serialize, Archive, PartialEq, Eq)]
    #[archive(check_bytes)]
    pub struct Entry {
        pub key: Distance,
        pub value: Ref,
    }

    pub fn entry_get(entries: &[ArchivedEntry], key: Distance) -> Option<&ArchivedEntry> {
        debug_assert_ne!(key, ENTRY_KEY_UNUSED);
        entries
            .binary_search_by(|probe| probe.key.cmp(&key))
            .ok()
            .map(|i| &entries[i])
    }

    pub fn entry_add(entries: &mut [ArchivedEntry], entry: Entry) -> Option<usize> {
        if entry_is_full(entries) {
            return None;
        }

        match entries.binary_search_by(|probe| probe.key.cmp(&entry.key)) {
            Err(i) if (..entries.len()).contains(&i) => {
                entries[i..].rotate_right(1);
                let target = &mut entries[i];
                debug_assert_eq!(ENTRY_KEY_UNUSED, target.key);
                *target = entry.into();
                Some(i)
            }
            _ => None,
        }
    }

    pub fn entry_is_full(entries: &[ArchivedEntry]) -> bool {
        entries
            .last()
            .map(|ent| ent.key != ENTRY_KEY_UNUSED)
            .unwrap_or(true)
    }

    pub fn entry_create(limit: usize) -> Vec<Entry> {
        let mut children = Vec::new();
        children.resize_with(limit, Default::default);
        children
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
                value: value.value.to_unarchived(),
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

            assert_eq!(Some(0), entry_add(&mut archived, entry(2)));
            assert_eq!(Some(1), entry_add(&mut archived, entry(4)));
            assert_eq!(Some(0), entry_add(&mut archived, entry(1)));
            assert_eq!(Some(2), entry_add(&mut archived, entry(3)));

            assert_eq!(None, entry_add(&mut archived, entry(3)));

            assert!(!entry_is_full(&archived));
            assert_eq!(Some(4), entry_add(&mut archived, entry(7)));
            assert!(entry_is_full(&archived));
            assert_eq!(None, entry_add(&mut archived, entry(8)));

            assert!(entry_get(&archived, 7).is_some());
            assert!(entry_get(&archived, 8).is_none());
        }
    }
}
