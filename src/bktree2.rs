use std::ops::RangeInclusive;
use std::path::Path;
use std::pin::Pin;

use self::entry::*;
use derivative::Derivative;
use rkyv::validation::validators::DefaultValidator;
use rkyv::vec::ArchivedVec;
use rkyv::{Archive, CheckBytes, Serialize};

use crate::{
    file_array::{self, FileArray, FileArraySerializer, Ref},
    imghash::hamming::{Distance, Hamming},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    FileArray(#[from] file_array::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct AnyValue(str); // TODO: panic if its archived value is read or something cool

#[derive(Serialize, Archive, Derivative)]
#[archive(check_bytes)]
#[derivative(Default(bound = ""))]
struct Meta<S> {
    #[derivative(Default(value = "Ref::null()"))]
    root: Ref<BKNode<S>>,
    // TODO: save some identifier for S
}

impl<S> ArchivedMeta<S> {
    fn root(self: Pin<&mut Self>) -> Pin<&mut Ref<BKNode<S>>> {
        unsafe { self.map_unchecked_mut(|m| &mut m.root) }
    }
}

const DEFAULT_CHILDREN_LIMIT: usize = 20;

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
struct BKNode<S> {
    hash: Hamming,
    value: S, // TODO: RefBox? DynRef? Utilise the trait object support?
    removed: bool,
    children: Ref<Children<S>>,
}

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
struct Children<S> {
    entries: Vec<Entry<S>>,
    next_sibling: Ref<Children<S>>,
}

impl<S> Children<S> {
    fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self {
            entries: entry_create(limit),
            next_sibling: Ref::null(),
        }
    }

    fn new_initial(limit: usize, initial_element: Entry<S>) -> Self {
        let mut selff = Self::new(limit);
        *selff.entries.first_mut().expect("the vec is not empty") = initial_element;
        selff
    }
}

impl<S> BKNode<S> {
    fn new(hash: Hamming, value: S) -> Self {
        Self {
            hash,
            value,
            children: Ref::null(),
            removed: false,
        }
    }
}

impl<S> ArchivedChildren<S>
where
    S: Archive,
{
    fn pin_mut_entries(self: Pin<&mut Self>) -> Pin<&mut ArchivedVec<ArchivedEntry<S>>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.entries) }
    }

    fn mut_next_sibling(self: Pin<&mut Self>) -> &mut Ref<Children<S>> {
        unsafe { &mut self.get_unchecked_mut().next_sibling }
    }
}

impl<S> ArchivedBKNode<S>
where
    S: Archive,
{
    fn mut_children(self: Pin<&mut Self>) -> &mut Ref<Children<S>> {
        unsafe { &mut self.get_unchecked_mut().children }
    }
}

pub struct BKTree<S> {
    db: FileArray,
    _src: std::marker::PhantomData<S>,
}

impl<S> BKTree<S> {
    // TODO: Somehow allow opening without caring what S is. Handy for rebuilding or
    // collecting stats.
    pub fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        let db = FileArray::new(file)?;
        Self::new(db)
    }

    fn new(mut db: FileArray) -> Result<Self> {
        if db.is_empty() {
            db.add_one::<Meta<S>>(&Meta::default())?;
        }

        Ok(Self {
            db,
            _src: std::marker::PhantomData,
        })
    }

    fn root(&self) -> Result<Ref<BKNode<S>>> {
        let meta_ref = FileArray::ref_to_first::<Meta<S>>();
        let meta = self.db.get::<Meta<S>>(meta_ref)?;
        Ok(meta.root)
    }

    fn set_root(&mut self, new_root: Ref<BKNode<S>>) -> Result<()> {
        let meta_ref = FileArray::ref_to_first::<Meta<S>>();
        let meta = self.db.get_mut::<Meta<S>>(meta_ref)?;
        meta.root().set(new_root);
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        Ok(self.db.flush()?)
    }
}

impl<S> BKTree<S>
where
    S: Serialize<FileArraySerializer>,
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

    pub fn add(&mut self, hash: Hamming, value: S) -> Result<()> {
        self.add_all([(hash, value)])
    }

    pub fn add_all(
        &mut self,
        items: impl IntoIterator<Item = (Hamming, S)>,
    ) -> Result<()> {
        let mut root = self.root()?;
        let mut items = items.into_iter();

        if let Some((hash, value)) = items.next() {
            if root.is_null() {
                root = self.db.add_one(&BKNode::new(hash, value))?;
                self.set_root(root)?;
            } else {
                self.add_internal(root, hash, value)?;
            }
        }

        for (hash, value) in items {
            self.add_internal(root, hash, value)?;
        }
        Ok(())
    }

    fn add_internal(
        &mut self,
        mut cur_node_ref: Ref<BKNode<S>>,
        hash: Hamming,
        value: S,
    ) -> Result<()> {
        assert!(cur_node_ref.is_not_null());

        let new_node = BKNode::new(hash, value);
        let new_node_ref = self.db.add_one(&new_node)?;

        'nodes: loop {
            let cur_node = self.db.get(cur_node_ref)?;
            let dist = cur_node.hash.distance_to(hash);

            let new_entry = Entry {
                key: dist,
                value: new_node_ref,
            };

            if cur_node.children.is_null() {
                let new_children =
                    Children::<S>::new_initial(DEFAULT_CHILDREN_LIMIT, new_entry);
                let new_children_ref = self.db.add_one(&new_children)?;
                let cur_node = self.db.get_mut(cur_node_ref)?;
                assert_eq!(Ref::null(), cur_node.children);
                *cur_node.mut_children() = new_children_ref;
                break 'nodes;
            }

            let mut cur_children_ref = cur_node.children;
            'children: loop {
                let cur_children = self.db.get(cur_children_ref)?;
                match entry_get(&cur_children.entries, dist) {
                    Some(entry) => {
                        cur_node_ref = entry.value;
                        break 'children;
                    }
                    None if !cur_children.next_sibling.is_null() => {
                        cur_children_ref = cur_children.next_sibling;
                    }
                    None => {
                        let cur_children = self.db.get_mut(cur_children_ref)?;
                        let mut entries = cur_children.pin_mut_entries().pin_mut_slice();
                        match entry_add(&mut entries, new_entry.clone()) {
                            Some(_) => (),
                            None => {
                                let new_sibling = Children::<S>::new_initial(
                                    DEFAULT_CHILDREN_LIMIT,
                                    new_entry,
                                );
                                let new_sibling_ref = self.db.add_one(&new_sibling)?;

                                let cur_children = self.db.get_mut(cur_children_ref)?;
                                assert_eq!(Ref::null(), cur_children.next_sibling);
                                *cur_children.mut_next_sibling() = new_sibling_ref;
                            }
                        }
                        break 'nodes;
                    }
                }
            }
        }

        Ok(())
    }

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
}

#[derive(Derivative, Debug, Clone, PartialEq, Eq)]
#[derivative(Default)]
enum IterateCmd {
    #[derivative(Default)]
    Continue,
    WithinRange(RangeInclusive<Distance>),
    Stop,
}

macro_rules! impl_walk {
    ($fun_name:ident, $self_type:ty, $visit_arg:ty, $db_get:ident, $visit_prep:expr) => {
        fn $fun_name<F>(self: $self_type, mut visit: F) -> Result<()>
        where
            F: FnMut($visit_arg) -> IterateCmd,
        {
            let mut stack = Vec::new();
            {
                let root = self.root()?;
                if root.is_not_null() {
                    stack.push(root);
                }
            }

            while let Some(cur_ref) = stack.pop() {
                let mut cur_node = self.db.$db_get(cur_ref)?;
                let dist_range = match visit($visit_prep(&mut cur_node)) {
                    IterateCmd::Continue => Distance::MIN..=Distance::MAX,
                    IterateCmd::WithinRange(range) => range,
                    IterateCmd::Stop => break,
                };

                let mut children_ref = cur_node.children;
                while children_ref.is_not_null() {
                    let children_node = self.db.get(children_ref)?;
                    stack.extend(
                        entry_used(&children_node.entries)
                            .iter()
                            .filter(|entry| dist_range.contains(&entry.key))
                            .map(|entry| entry.value),
                    );
                    children_ref = children_node.next_sibling;
                }
            }

            Ok(())
        }
    };
}

impl<S> BKTree<S>
where
    S: Archive,
    S::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    impl_walk!(
        walk_mut,
        &mut Self,
        Pin<&mut ArchivedBKNode<S>>,
        get_mut,
        Pin::as_mut
    );
    impl_walk!(walk, &Self, &ArchivedBKNode<S>, get, std::convert::identity);

    pub fn for_each<F>(&self, mut visit: F) -> Result<()>
    where
        F: FnMut(Hamming, &S::Archived),
    {
        self.walk(|node| {
            if !node.removed {
                visit(node.hash, &node.value);
            }
            IterateCmd::default()
        })
    }

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
    use tempfile::tempfile;

    use super::*;

    type Source = String;
    fn value(path: impl Into<Source>) -> Source {
        path.into()
    }

    fn create_bktree_tempfile<S>() -> Result<BKTree<S>> {
        let arr = FileArray::new_tempfile()?;
        BKTree::new(arr)
    }

    fn contents(tree: &BKTree<Source>) -> Result<Vec<(Hamming, String)>> {
        let mut all = Vec::new();
        tree.for_each(|ham, val| all.push((ham, val.as_str().to_owned())))?;
        all.sort();
        Ok(all)
    }

    #[test]
    fn add() -> Result<()> {
        let mut tree: BKTree<Source> = create_bktree_tempfile()?;
        tree.add(Hamming(0b101), value("5_1"))?;
        tree.add(Hamming(0b101), value("5_2"))?;
        tree.add(Hamming(0b100), value("4"))?;

        let all = contents(&tree)?;

        assert_eq!(
            vec![
                (Hamming(0b100), "4".to_string()),
                (Hamming(0b101), "5_1".to_string()),
                (Hamming(0b101), "5_2".to_string()),
            ],
            all
        );

        // let mut closest = Vec::new();
        // tree.find_within(Hamming(0b101), 0, |_, val| closest.push(val.clone()))?;
        // closest.sort();
        // let answer: Vec<PathBuf> = vec!["5_1".into(), "5_2".into()];
        // assert_eq!(answer, closest);

        Ok(())
    }

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

    // TODO: static assert that this is outside of Hamming::min_distance and
    // hamming::max_distance. Also greater than hamming::max_distance to make sure it gets
    // sorted last.
    const ENTRY_KEY_UNUSED: Distance = Distance::MAX;

    #[derive(Serialize, Archive, PartialEq, Eq, Derivative)]
    #[derivative(Clone(bound = ""))]
    #[archive(check_bytes)]
    pub(super) struct Entry<S> {
        pub key: Distance,
        pub value: Ref<BKNode<S>>,
    }

    pub(super) fn entry_get<S>(
        entries: &[ArchivedEntry<S>],
        key: Distance,
    ) -> Option<&ArchivedEntry<S>> {
        assert_ne!(key, ENTRY_KEY_UNUSED);
        entries
            .binary_search_by(|probe| probe.key.cmp(&key))
            .ok()
            .map(|i| &entries[i])
    }

    pub(super) fn entry_add<S>(
        entries: &mut [ArchivedEntry<S>],
        entry: Entry<S>,
    ) -> Option<usize> {
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

    pub(super) fn entry_is_full<S>(entries: &[ArchivedEntry<S>]) -> bool {
        entries
            .last()
            .map(|ent| ent.key != ENTRY_KEY_UNUSED)
            .unwrap_or(true)
    }

    pub(super) fn entry_create<S>(limit: usize) -> Vec<Entry<S>> {
        let mut children = Vec::new();
        children.resize_with(limit, Default::default);
        children
    }

    pub(super) fn entry_used<S>(entries: &[ArchivedEntry<S>]) -> &[ArchivedEntry<S>] {
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

    impl<S> Default for Entry<S> {
        fn default() -> Self {
            Self {
                key: ENTRY_KEY_UNUSED,
                value: Ref::null(),
            }
        }
    }

    impl<S> From<Entry<S>> for ArchivedEntry<S> {
        fn from(value: Entry<S>) -> Self {
            Self {
                key: value.key,
                value: value.value.into(),
            }
        }
    }

    impl<S> From<&ArchivedEntry<S>> for Entry<S> {
        fn from(value: &ArchivedEntry<S>) -> Self {
            Self {
                key: value.key,
                value: value.value,
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        fn entry<S>(key: Distance) -> Entry<S> {
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

            let mut archived: Vec<ArchivedEntry<()>> =
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
}
