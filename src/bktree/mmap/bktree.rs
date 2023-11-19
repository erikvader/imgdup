use std::borrow::Borrow;
use std::fs::File;
use std::ops::RangeInclusive;
use std::path::Path;
use std::pin::Pin;

use super::deferred_box::{self, DeferredBox, DeferredBoxSerializer};
use super::entry::*;
use rkyv::validation::validators::DefaultValidator;
use rkyv::vec::ArchivedVec;
use rkyv::{Archive, CheckBytes, Serialize};

use super::file_array::{self, FileArray, Ref};
use crate::bktree::source_types::any_source::AnySource;
use crate::bktree::source_types::{PartialSource, Source};
use crate::imghash::hamming::{Distance, Hamming};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("file array: {0}")]
    FileArray(#[from] file_array::Error),
    #[error("deferred box: {0}")]
    DeferredBox(#[from] deferred_box::Error),
    #[error(
        "source mismatch: trying to open as {opening_as}, but it is stored as {stored_as}"
    )]
    SourceMismatch {
        opening_as: String,
        stored_as: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
struct Meta {
    root: Ref<BKNode>,
    source_ident: String,
    // TODO: somehow store the version of this struct itself? Need two layers of headers?
    // The first layer has the version and points to the other header (this one)? Or use
    // repr(C) and store the version as the first field?
}

impl ArchivedMeta {
    fn root(self: Pin<&mut Self>) -> Pin<&mut Ref<BKNode>> {
        unsafe { self.map_unchecked_mut(|m| &mut m.root) }
    }
}

impl Meta {
    fn new(source_ident: String) -> Self {
        Self {
            root: Ref::null(),
            source_ident,
        }
    }
}

const DEFAULT_CHILDREN_LIMIT: usize = 20;

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
pub(super) struct BKNode {
    hash: Hamming,
    value: DeferredBox,
    removed: bool,
    children: Ref<Children>,
}

#[derive(Serialize, Archive)]
#[archive(check_bytes)]
pub(super) struct Children {
    entries: Vec<Entry>,
    next_sibling: Ref<Children>,
}

impl Children {
    fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self {
            entries: entry_create(limit),
            next_sibling: Ref::null(),
        }
    }

    fn new_initial(limit: usize, initial_element: Entry) -> Self {
        let mut selff = Self::new(limit);
        *selff.entries.first_mut().expect("the vec is not empty") = initial_element;
        selff
    }
}

impl BKNode {
    fn new(hash: Hamming, value: DeferredBox) -> Self {
        Self {
            hash,
            value,
            children: Ref::null(),
            removed: false,
        }
    }
}

impl ArchivedChildren {
    fn pin_mut_entries(self: Pin<&mut Self>) -> Pin<&mut ArchivedVec<ArchivedEntry>> {
        unsafe { self.map_unchecked_mut(|s| &mut s.entries) }
    }

    fn mut_next_sibling(self: Pin<&mut Self>) -> &mut Ref<Children> {
        unsafe { &mut self.get_unchecked_mut().next_sibling }
    }
}

impl ArchivedBKNode {
    fn mut_children(self: Pin<&mut Self>) -> &mut Ref<Children> {
        unsafe { &mut self.get_unchecked_mut().children }
    }

    fn mut_removed(self: Pin<&mut Self>) -> &mut bool {
        unsafe { &mut self.get_unchecked_mut().removed }
    }
}

pub struct BKTree<S>
where
    S: PartialSource,
{
    db: FileArray,
    _src: std::marker::PhantomData<S>,
}

impl<S> BKTree<S>
where
    S: PartialSource,
{
    pub fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        let db = FileArray::new(file)?;
        Self::new(db)
    }

    fn new(mut db: FileArray) -> Result<Self> {
        let source_ident = S::identifier();

        if db.is_empty() {
            init_meta(
                &mut db,
                source_ident
                    .expect("cannot create a new BKTree without a source identifier")
                    .to_string(),
            )?;
        }

        let new_self = Self {
            db,
            _src: std::marker::PhantomData,
        };

        if let Some(ident) = source_ident {
            let meta_ident = new_self.source_ident()?;

            if ident != meta_ident {
                return Err(Error::SourceMismatch {
                    opening_as: ident.to_string(),
                    stored_as: meta_ident.to_string(),
                });
            }
        }

        Ok(new_self)
    }

    fn empty_copy_of(&self, mut new_db: FileArray) -> Result<Self> {
        assert!(new_db.is_empty());
        let ident = self.source_ident()?.to_string();
        init_meta(&mut new_db, ident)?;
        Self::new(new_db)
    }

    fn source_ident(&self) -> Result<&str> {
        let meta_ref = FileArray::ref_to_first::<Meta>();
        let meta = self.db.get::<Meta>(meta_ref)?;
        Ok(meta.source_ident.as_str())
    }

    fn root(&self) -> Result<Ref<BKNode>> {
        let meta_ref = FileArray::ref_to_first::<Meta>();
        let meta = self.db.get::<Meta>(meta_ref)?;
        Ok(meta.root)
    }

    fn set_root(&mut self, new_root: Ref<BKNode>) -> Result<()> {
        let meta_ref = FileArray::ref_to_first::<Meta>();
        let meta = self.db.get_mut::<Meta>(meta_ref)?;
        meta.root().set(new_root);
        Ok(())
    }

    pub fn sync_to_disk(&self) -> Result<()> {
        Ok(self.db.sync_to_disk()?)
    }
}

fn init_meta(db: &mut FileArray, source_ident: String) -> file_array::Result<()> {
    let meta_ref = db.add_one(Meta::new(source_ident))?;
    assert_eq!(
        meta_ref,
        FileArray::ref_to_first::<Meta>(),
        "The header is not reachable with `ref_to_first`"
    );
    Ok(())
}

impl BKTree<AnySource> {
    pub fn downcast<S>(self) -> Result<BKTree<S>>
    where
        // NOTE: this should maybe be `Source`, but having it as partial allows
        // `AnySource` to downcast to itself, which is nice maybe? Akin to `Into`
        S: PartialSource,
    {
        BKTree::new(self.db)
    }
}

impl<S> BKTree<S>
where
    // NOTE: this should maybe be `Source`, but having it as partial allows `AnySource` to
    // upcast to itself, which is nice maybe? Akin to `Into`
    S: PartialSource,
{
    pub fn upcast(self) -> BKTree<AnySource> {
        BKTree {
            db: self.db,
            _src: std::marker::PhantomData,
        }
    }
}

impl<S> BKTree<S>
where
    S: Serialize<DeferredBoxSerializer> + Source,
    S::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    pub fn add<B>(&mut self, hash: Hamming, value: B) -> Result<()>
    where
        B: Borrow<S>,
    {
        self.add_all([(hash, value)])
    }

    pub fn add_all<B>(
        &mut self,
        items: impl IntoIterator<Item = (Hamming, B)>,
    ) -> Result<()>
    where
        B: Borrow<S>,
    {
        let mut root = self.root()?;
        let mut items = items.into_iter();

        if let Some((hash, value)) = items.next() {
            let value_box = DeferredBox::new(value)?;
            if root.is_null() {
                root = self.db.add_one(BKNode::new(hash, value_box))?;
                self.set_root(root)?;
            } else {
                self.add_internal(root, hash, value_box)?;
            }
        }

        for (hash, value) in items {
            let value_box = DeferredBox::new(value)?;
            self.add_internal(root, hash, value_box)?;
        }
        Ok(())
    }
}

impl<S> BKTree<S>
where
    S: PartialSource,
{
    fn add_internal(
        &mut self,
        mut cur_node_ref: Ref<BKNode>,
        hash: Hamming,
        value: DeferredBox,
    ) -> Result<()> {
        assert!(cur_node_ref.is_not_null());

        let new_node_ref = {
            let new_node = BKNode::new(hash, value);
            self.db.add_one(new_node)?
        };

        'nodes: loop {
            let cur_node = self.db.get(cur_node_ref)?;
            let dist = cur_node.hash.distance_to(hash);

            let new_entry = Entry {
                key: dist,
                value: new_node_ref,
            };

            if cur_node.children.is_null() {
                let new_children =
                    Children::new_initial(DEFAULT_CHILDREN_LIMIT, new_entry);
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
                                let new_sibling = Children::new_initial(
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IterateCmd {
    Continue,
    WithinRange(RangeInclusive<Distance>),
    #[allow(unused)] // TODO: rebuild will need this in the future with restartable walk
    Stop,
}

macro_rules! impl_walk {
    ($fun_name:ident, $self_type:ty, $visit_arg:ty, $db_get:ident, $visit_prep:expr) => {
        fn $fun_name<'a, F>(self: $self_type, mut visit: F) -> Result<()>
        where
            F: FnMut($visit_arg) -> Result<IterateCmd>,
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
                let dist_range = match visit($visit_prep(&mut cur_node))? {
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
    S: PartialSource,
{
    impl_walk!(
        walk_mut,
        &'a mut Self,
        Pin<&mut ArchivedBKNode>,
        get_mut,
        Pin::as_mut
    );
    impl_walk!(
        walk,
        &'a Self,
        &'a ArchivedBKNode,
        get,
        std::convert::identity
    );
}

impl<S> BKTree<S>
where
    S: Archive + Source,
    S::Archived: for<'b> CheckBytes<DefaultValidator<'b>>,
{
    pub fn for_each<'a, F>(&'a self, mut visit: F) -> Result<()>
    where
        F: FnMut(Hamming, &'a S::Archived),
    {
        self.walk(|arch_node| {
            if !arch_node.removed {
                let value = arch_node.value.get::<S>()?;
                visit(arch_node.hash, value);
            }
            Ok(IterateCmd::Continue)
        })
    }

    pub fn find_within<'a, F>(
        &'a self,
        hash: Hamming,
        within: Distance,
        mut visit: F,
    ) -> Result<()>
    where
        F: FnMut(Hamming, &'a S::Archived),
    {
        self.walk(|arch_node| {
            let dist = arch_node.hash.distance_to(hash);
            if dist <= within && !arch_node.removed {
                let value = arch_node.value.get::<S>()?;
                visit(arch_node.hash, value);
            }
            Ok(IterateCmd::WithinRange(
                dist.saturating_sub(within)..=dist.saturating_add(within),
            ))
        })
    }

    pub fn remove_any_of<P>(&mut self, mut predicate: P) -> Result<()>
    where
        P: FnMut(Hamming, &S::Archived) -> bool,
    {
        self.walk_mut(|arch_node| {
            let value = arch_node.value.get::<S>()?;
            if !arch_node.removed && predicate(arch_node.hash, value) {
                *arch_node.mut_removed() = true;
            }
            Ok(IterateCmd::Continue)
        })
    }
}

impl<S> BKTree<S>
where
    S: PartialSource,
{
    pub fn count_nodes(&self) -> Result<(usize, usize)> {
        let mut alive = 0;
        let mut dead = 0;
        self.walk(|arch_node| {
            if arch_node.removed {
                dead += 1;
            } else {
                alive += 1;
            }
            Ok(IterateCmd::Continue)
        })?;

        Ok((alive, dead))
    }

    pub fn rebuild_to(&self, path: impl AsRef<Path>) -> Result<Self> {
        let db = FileArray::new(path)?;
        self.rebuild_to_internal(db)
    }

    fn rebuild_to_internal(&self, db: FileArray) -> Result<Self> {
        let mut new_tree = self.empty_copy_of(db)?;
        let mut new_root = Ref::null();

        self.walk(|arch_node| {
            if !arch_node.removed {
                let hash = arch_node.hash;
                let value = arch_node.value.deserialize();

                // TODO: Make walk restartable by returning the stack after a stop. There
                // could then be two walks, one that looks for the root and another that
                // does add_internal on that root.
                if new_root.is_null() {
                    new_root = new_tree.db.add_one(BKNode::new(hash, value))?;
                    new_tree.set_root(new_root)?;
                } else {
                    new_tree.add_internal(new_root, hash, value)?;
                }
            }

            Ok(IterateCmd::Continue)
        })?;

        Ok(new_tree)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};

    use crate::bktree::source_types::{
        any_source::AnySource, string_source::StringSource,
    };

    use super::*;

    type Source = StringSource;
    fn value(path: impl Into<String>) -> Source {
        StringSource(path.into())
    }

    fn create_bktree_tempfile<S: PartialSource>() -> Result<BKTree<S>> {
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

        let mut closest: Vec<String> = Vec::new();
        tree.find_within(Hamming(0b101), 0, |_, val| closest.push(val.to_string()))?;
        closest.sort();
        let answer: Vec<String> = vec!["5_1".into(), "5_2".into()];
        assert_eq!(answer, closest);

        Ok(())
    }

    #[test]
    fn remove() -> Result<()> {
        let mut tree: BKTree<Source> = create_bktree_tempfile()?;
        tree.add(Hamming(0b101), value("5_1"))?;
        tree.add(Hamming(0b101), value("5_2"))?;
        tree.add(Hamming(0b100), value("4"))?;

        let rem: HashSet<String> = HashSet::from(["5_1".into()]);
        tree.remove_any_of(|_, p| rem.contains(p.as_str()))?;

        let all = contents(&mut tree)?;

        assert_eq!(
            vec![
                (Hamming(0b100), "4".to_string()),
                (Hamming(0b101), "5_2".to_string()),
            ],
            all
        );

        assert_eq!((2, 1), tree.count_nodes()?);
        let tree = {
            let db_rebuilt = FileArray::new_tempfile()?;
            tree.rebuild_to_internal(db_rebuilt)?
        };
        assert_eq!((2, 0), tree.count_nodes()?);

        assert_eq!(
            vec![
                (Hamming(0b100), "4".to_string()),
                (Hamming(0b101), "5_2".to_string()),
            ],
            all
        );

        Ok(())
    }

    #[test]
    fn find_within_large() -> Result<()> {
        let seed: u64 = rand::random();
        println!("Using seed: {}", seed);
        let mut rng = SmallRng::seed_from_u64(seed);

        let within = 5;
        let num_within = 100;
        let num_dups = 30;
        let total = 1_000;

        let search_hash: Hamming = rng.gen();
        let indices_within: HashSet<usize> =
            rand::seq::index::sample(&mut rng, total, num_within)
                .into_iter()
                .collect();

        let mut tree: BKTree<Source> = create_bktree_tempfile()?;
        let mut key = Vec::new();
        for i in 0..total {
            let hash = if indices_within.contains(&i) {
                search_hash.random_within(&mut rng, within)
            } else {
                search_hash.random_outside(&mut rng, within)
            };

            tree.add(hash, value(i.to_string()))?;

            if search_hash.distance_to(hash) <= within {
                key.push(hash);
            }
        }

        {
            let mut dups = Vec::with_capacity(num_dups);
            for hash in key.choose_multiple(&mut rng, num_dups) {
                tree.add(*hash, value("dup"))?;
                dups.push(*hash);
            }
            key.extend(dups);
        }

        assert_eq!(num_dups + num_within, key.len());

        let mut closest = Vec::new();
        tree.find_within(search_hash, within, |hash, _| closest.push(hash))?;

        assert_eq!(key.len(), closest.len());

        closest.sort();
        key.sort();
        assert_eq!(key, closest);

        Ok(())
    }

    #[test]
    fn source_mismatch() -> Result<()> {
        let tree: BKTree<Source> = create_bktree_tempfile()?;
        let file_array = tree.db;
        let tree_unit = BKTree::<()>::new(file_array);
        assert!(matches!(tree_unit, Err(Error::SourceMismatch { .. })));

        let tree: BKTree<Source> = create_bktree_tempfile()?;
        let file_array = tree.db;
        let tree = BKTree::<Source>::new(file_array);
        assert!(tree.is_ok());

        let tree: BKTree<Source> = create_bktree_tempfile()?;
        let file_array = tree.db;
        let tree = BKTree::<AnySource>::new(file_array);
        assert!(tree.is_ok());
        Ok(())
    }
}
