use std::path::Path;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use self::{priority_queue::PriorityQueue, sql::Sql};

mod priority_queue;
mod sql;

type Uuid = i64;
const UUID_FIRST: Uuid = 0;
const UUID_NULL: Uuid = Uuid::min_value();

pub type Result<T> = std::result::Result<T, HeapError>;

#[derive(thiserror::Error, Debug)]
pub enum HeapError {
    #[error("SQlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Ref does not exist: {0:?}")]
    RefNotExists(Ref),
}

pub struct Heap<T> {
    cache: PriorityQueue<Uuid, Block<T>>,
    dirty_changes: usize,
    cache_age: usize,
    sql: Sql,
    config: Config,
    // -- saved in db --
    next_id: Uuid,
    root: Ref,
}

struct Config {
    cache_capacity: usize,
    dirtyness_limit: usize,
    maximum_block_size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockState {
    Clean,
    Dirty,
}

struct Block<T> {
    state: BlockState,
    access_count: usize,
    data: Vec<(Uuid, T)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ref {
    block_id: Uuid,
    sub_id: Uuid,
}

pub struct HeapBuilder {
    config: Config,
}

impl HeapBuilder {
    pub fn new() -> Self {
        Self {
            config: Config {
                // TODO: make these tweakable from CLI arguments
                cache_capacity: 2048,
                dirtyness_limit: 128,
                maximum_block_size: 10,
            },
        }
    }

    pub fn cache_capacity(mut self, cache_capacity: usize) -> Self {
        assert!(cache_capacity >= 1);
        self.config.cache_capacity = cache_capacity;
        self
    }

    pub fn dirtyness_limit(mut self, dirtyness_limit: usize) -> Self {
        assert!(dirtyness_limit >= 1);
        self.config.dirtyness_limit = dirtyness_limit;
        self
    }

    pub fn maximum_block_size(mut self, maximum_block_size: usize) -> Self {
        assert!(maximum_block_size >= 1);
        self.config.maximum_block_size = maximum_block_size;
        self
    }

    pub fn in_memory<T>(self) -> Result<Heap<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        Heap::new(Sql::new_in_memory()?, self.config)
    }

    pub fn from_file<T>(self, file: impl AsRef<Path>) -> Result<Heap<T>>
    where
        T: Serialize + DeserializeOwned,
    {
        Heap::new(Sql::new_from_file(file)?, self.config)
    }
}

impl<T> Heap<T>
where
    T: Serialize + DeserializeOwned,
{
    fn new(sql: Sql, config: Config) -> Result<Self> {
        let next_id = sql.get_meta::<Uuid>("next_id")?.unwrap_or(UUID_FIRST);
        let root = sql.get_meta::<Ref>("root")?.unwrap_or(Ref::null());

        sql.begin()?;

        Ok(Self {
            cache: PriorityQueue::with_capacity(config.cache_capacity),
            dirty_changes: 0,
            cache_age: 0,
            next_id,
            root,
            config,
            sql,
        })
    }

    pub fn new_in_memory() -> Result<Self> {
        HeapBuilder::new().in_memory()
    }

    pub fn new_from_file(file: impl AsRef<Path>) -> Result<Self> {
        HeapBuilder::new().from_file(file)
    }

    pub fn allocate(&mut self, initial_data: T) -> Result<Ref> {
        let r = Ref::new(self.next_id, self.next_id);
        self.handle_overflow()?;
        let oldval = self.cache.push(
            r.block_id,
            Block::new_dirty(r.sub_id, initial_data, self.cache_age),
        );
        self.dirty_changes += 1;
        self.next_id += 1;
        assert!(oldval.is_none());
        Ok(r)
    }

    pub fn allocate_local(&mut self, other: Ref, initial_data: T) -> Result<Ref> {
        if other.is_null() {
            return self.allocate(initial_data);
        }

        self.load_block(other)?;
        // NOTE: does not modify the block's ordering
        match self.cache.get_mut_unchecked(other.block_id()) {
            None => self.allocate(initial_data),
            Some(block) if block.data.len() >= self.config.maximum_block_size => {
                self.allocate(initial_data)
            }
            Some(block) => {
                let r = Ref::new(other.block_id, self.next_id);
                block.data.push((r.sub_id, initial_data));
                self.next_id += 1;
                self.dirty_changes += 1;
                block.state = BlockState::Dirty;
                Ok(r)
            }
        }
    }

    pub fn root(&self) -> Ref {
        self.root
    }

    pub fn set_root(&mut self, root: Ref) {
        self.root = root;
    }

    pub fn clear_root(&mut self) {
        self.root = Ref::null();
    }

    // NOTE: for testing
    pub fn count_refs(&mut self) -> Result<usize> {
        self.flush()?;
        self.sql.count_refs()
    }

    pub fn set(&mut self, r: Ref, data: T) -> Result<()> {
        match self.deref_mut(r)? {
            None => Err(HeapError::RefNotExists(r)),
            Some(place) => {
                *place = data;
                Ok(())
            }
        }
    }

    pub fn remove(&mut self, r: Ref) -> Result<()> {
        self.load_block(r)?;
        // NOTE: does not modify the block's ordering
        match self.cache.get_mut_unchecked(r.block_id()) {
            None => Err(HeapError::RefNotExists(r)),
            Some(block) => {
                let len_before = block.data.len();
                block.data.retain(|(sub, _)| *sub != r.sub_id);
                if block.data.len() == len_before {
                    return Err(HeapError::RefNotExists(r));
                }
                assert_eq!(block.data.len(), len_before - 1);
                block.state = BlockState::Dirty;
                self.dirty_changes += 1;
                Ok(())
            }
        }
    }

    pub fn has_value(&mut self, r: Ref) -> Result<bool> {
        Ok(self.deref(r)?.is_some())
    }

    pub fn deref(&mut self, r: Ref) -> Result<Option<&T>> {
        self.load_block(r)?;
        self.cache
            .modify(r.block_id(), |data| data.access_count += 1);
        Ok(self.cache.get(r.block_id()).and_then(|block| {
            block
                .data
                .binary_search_by_key(r.sub_id(), |(sub_id, _)| *sub_id)
                .ok()
                .map(|i| &block.data[i].1)
        }))
    }

    pub fn deref_mut(&mut self, r: Ref) -> Result<Option<&mut T>> {
        self.load_block(r)?;
        self.cache.modify(r.block_id(), |data| {
            data.state = BlockState::Dirty;
            data.access_count += 1;
            self.dirty_changes += 1;
        });
        // NOTE: does not modify the block's ordering
        Ok(self
            .cache
            .get_mut_unchecked(r.block_id())
            .and_then(|block| {
                block
                    .data
                    .binary_search_by_key(r.sub_id(), |(sub_id, _)| *sub_id)
                    .ok()
                    .map(|i| &mut block.data[i].1)
            }))
    }

    fn load_block(&mut self, r: Ref) -> Result<()> {
        if !r.is_null() && !self.cache.contains_key(r.block_id()) {
            if let Some(val) = self.sql.get_refs::<Vec<(Uuid, T)>>(r.block_id)? {
                self.handle_overflow()?;
                let oldval = self
                    .cache
                    .push(r.block_id, Block::new_clean(val, self.cache_age));
                assert!(oldval.is_none());
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.sql.put_meta("next_id", self.next_id)?;
        self.sql.put_meta("root", self.root)?;

        for (&id, block) in self.cache.iter() {
            match block.state {
                BlockState::Clean => {
                    assert!(!block.data.is_empty());
                }
                BlockState::Dirty => {
                    if block.data.is_empty() {
                        self.sql.remove_refs(id)?;
                    } else {
                        self.sql.put_refs(id, &block.data)?;
                    }
                }
            }
        }

        self.sql.commit()?;
        self.sql.begin()?;

        self.cache.retain(|block| {
            block.state = BlockState::Clean;
            !block.data.is_empty()
        });
        self.dirty_changes = 0;

        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        if self.dirty_changes >= self.config.dirtyness_limit {
            self.flush()?;
        }
        Ok(())
    }

    /// Saving is not done on drop because the db should not accidentally be saved in an
    /// invalid state from e.g. panics.
    pub fn close(mut self) -> Result<()> {
        self.flush()?;
        self.sql.close()?;
        Ok(())
    }

    /// Clears space in the cache to make sure that at least one new element can be added
    /// without becoming bigger than the maximum size.
    fn handle_overflow(&mut self) -> Result<()> {
        assert!(self.config.cache_capacity >= 1);
        while self.cache.len() >= self.config.cache_capacity {
            // TODO: if the ref count overflows, then halve (or something) all
            // access_counts in the cache. But that will probably never happen since a
            // usize is pretty big.
            let (id, min) = self.cache.pop().expect("the cache is not empty");
            self.cache_age = min.access_count;

            match min.state {
                BlockState::Clean => {
                    assert!(!min.data.is_empty());
                }
                BlockState::Dirty => {
                    if min.data.is_empty() {
                        self.sql.remove_refs(id)?;
                    } else {
                        self.sql.put_refs(id, min.data)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl Ref {
    const fn new(block_id: Uuid, sub_id: Uuid) -> Self {
        Self { block_id, sub_id }
    }

    fn block_id(&self) -> &Uuid {
        &self.block_id
    }

    fn sub_id(&self) -> &Uuid {
        &self.sub_id
    }

    pub const fn null() -> Self {
        Self::new(UUID_NULL, UUID_FIRST)
    }

    pub fn is_null(&self) -> bool {
        self == &Self::null()
    }
}

impl<T> Block<T> {
    fn new_dirty(sub_id: Uuid, init_data: T, access_count: usize) -> Self {
        Self {
            data: vec![(sub_id, init_data)],
            state: BlockState::Dirty,
            access_count,
        }
    }

    fn new_clean(data: Vec<(Uuid, T)>, access_count: usize) -> Self {
        Self {
            data,
            state: BlockState::Clean,
            access_count,
        }
    }
}

impl<T> PartialEq for Block<T> {
    fn eq(&self, other: &Self) -> bool {
        self.access_count.eq(&other.access_count)
    }
}
impl<T> Eq for Block<T> {}
impl<T> PartialOrd for Block<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.access_count.partial_cmp(&other.access_count)
    }
}
impl<T> Ord for Block<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.access_count.cmp(&other.access_count)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl<T> Heap<T>
    where
        T: Serialize + DeserializeOwned,
    {
        pub fn reset(&mut self) -> Result<()> {
            self.flush()?;
            assert!(self
                .cache
                .iter()
                .all(|(_, block)| block.state == BlockState::Clean
                    && !block.data.is_empty()));
            self.cache.clear();
            Ok(())
        }

        fn state_of(&self, r: Ref) -> Option<BlockState> {
            self.cache.get(r.block_id()).map(|d| d.state)
        }

        fn block_data_of(&self, r: Ref) -> Option<&[(Uuid, T)]> {
            self.cache.get(r.block_id()).map(|d| d.data.as_slice())
        }
    }

    #[test]
    fn test_insert() -> Result<()> {
        let mut db = Heap::<i32>::new_in_memory()?;
        let r = db.allocate(5)?;
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(BlockState::Dirty), db.state_of(r));

        db.reset()?;
        assert_eq!(None, db.state_of(r));
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(BlockState::Clean), db.state_of(r));

        assert_eq!(Some(&mut 5), db.deref_mut(r)?);
        assert_eq!(Some(BlockState::Dirty), db.state_of(r));

        assert_eq!(UUID_FIRST + 1, db.next_id);
        Ok(())
    }

    #[test]
    fn test_insert_blocks() -> Result<()> {
        let mut db = HeapBuilder::new()
            .maximum_block_size(2)
            .in_memory::<i32>()?;

        let first = db.allocate(1)?;
        let second = db.allocate(2)?;
        assert_eq!(2, db.cache.len());
        assert_ne!(first.block_id, second.block_id);

        let third = db.allocate_local(first, 3)?;
        assert_eq!(2, db.cache.len());
        assert_eq!(first.block_id, third.block_id);
        let block = db.block_data_of(first).unwrap();
        assert!(block[0].0 < block[1].0);

        let fourth = db.allocate_local(first, 4)?;
        assert_eq!(3, db.cache.len());
        assert_ne!(first.block_id, fourth.block_id);

        Ok(())
    }

    #[test]
    fn test_remove() -> Result<()> {
        let mut db = HeapBuilder::new()
            .maximum_block_size(2)
            .in_memory::<i32>()?;

        let r1 = db.allocate(3)?;
        assert_eq!(Some(&3), db.deref(r1)?);

        db.remove(r1)?;
        assert_eq!(Some(BlockState::Dirty), db.state_of(r1));
        assert_eq!(Some(&[] as &[(_, _)]), db.block_data_of(r1));
        assert_eq!(None, db.deref(r1)?);

        let r2 = db.allocate(6)?;
        assert_eq!(Some(BlockState::Dirty), db.state_of(r1));
        assert_eq!(Some(&[] as &[(_, _)]), db.block_data_of(r1));
        assert_eq!(Some(BlockState::Dirty), db.state_of(r2));
        assert_eq!(1, db.block_data_of(r2).unwrap().len());

        db.reset()?;
        assert_eq!(None, db.state_of(r1));
        assert_eq!(None, db.state_of(r2));

        assert_eq!(Some(&6), db.deref(r2)?);
        assert_eq!(None, db.deref(r1)?);

        assert_eq!(None, db.state_of(r1));
        assert_eq!(Some(BlockState::Clean), db.state_of(r2));
        assert_eq!(1, db.count_refs()?);

        db.reset()?;
        db.remove(r2)?;
        assert!(matches!(db.remove(r2), Err(HeapError::RefNotExists(_))));

        assert_eq!(Some(BlockState::Dirty), db.state_of(r2));
        assert_eq!(None, db.deref(r2)?);

        db.reset()?;
        assert_eq!(None, db.deref(r2)?);
        assert_eq!(None, db.state_of(r2));

        assert_eq!(0, db.count_refs()?);

        Ok(())
    }
}
