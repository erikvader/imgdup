use std::path::Path;

use indexmap::IndexMap;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use self::{priority_queue::PriorityQueue, sql::Sql};

mod priority_queue;
mod sql;

type Uuid = i64;
pub type Result<T> = std::result::Result<T, HeapError>;

#[derive(thiserror::Error, Debug)]
pub enum HeapError {
    #[error("SQlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct Heap<T> {
    cache: PriorityQueue<Ref, Data<T>>,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataState {
    Clean,
    Dirty,
    Remove,
}

struct Data<T> {
    state: DataState,
    access_count: usize,
    data: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ref {
    id: Uuid,
}

pub struct HeapBuilder {
    config: Config,
}

impl HeapBuilder {
    pub fn new() -> Self {
        Self {
            config: Config {
                cache_capacity: 2048,
                dirtyness_limit: 128,
            },
        }
    }

    pub fn with_cache_capacity(mut self, cache_capacity: usize) -> Self {
        assert!(cache_capacity >= 1);
        self.config.cache_capacity = cache_capacity;
        self
    }

    pub fn with_dirtyness_limit(mut self, dirtyness_limit: usize) -> Self {
        assert!(dirtyness_limit >= 1);
        self.config.dirtyness_limit = dirtyness_limit;
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
        let next_id = sql
            .get_meta::<Uuid>("next_id")?
            .unwrap_or(Uuid::min_value());
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

    pub fn allocate(&mut self) -> Ref {
        let r = Ref::new(self.next_id);
        self.next_id += 1;
        r
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
        assert!(!r.is_null());
        self.handle_overflow()?;
        let oldval = self.cache.push(r, Data::new_dirty(data, self.cache_age));
        match oldval {
            None
            | Some(Data {
                state: DataState::Clean,
                ..
            }) => {
                self.dirty_changes += 1;
            }
            _ => (),
        }
        Ok(())
    }

    pub fn remove_entry(&mut self, r: Ref) -> Result<()> {
        self.load(r)?;
        // NOTE: not ordered by state
        self.cache.modify_unchecked(&r, |data| {
            data.state = DataState::Remove;
            self.dirty_changes += 1;
        });
        Ok(())
    }

    pub fn has_value(&mut self, r: Ref) -> Result<bool> {
        Ok(self.deref(r)?.is_some())
    }

    pub fn deref(&mut self, r: Ref) -> Result<Option<&T>> {
        self.load(r)?;
        self.cache.modify(&r, |data| data.access_count += 1);
        Ok(self
            .cache
            .get(&r)
            .filter(|data| data.state != DataState::Remove)
            .map(|data| &data.data))
    }

    pub fn deref_mut(&mut self, r: Ref) -> Result<Option<&mut T>> {
        self.load(r)?;
        self.cache.modify(&r, |data| {
            data.state = DataState::Dirty;
            data.access_count += 1;
            self.dirty_changes += 1;
        });
        Ok(self
            .cache
            .get_mut_unchecked(&r)
            .filter(|data| data.state != DataState::Remove)
            // NOTE: modifying data.data will not change the access_count, i.e., modify
            // the ordering of data and destroy the heap.
            .map(|data| &mut data.data))
    }

    fn load(&mut self, r: Ref) -> Result<()> {
        if !r.is_null() && !self.cache.contains_key(&r) {
            if let Some(val) = self.sql.get_refs::<T>(r.id)? {
                self.handle_overflow()?;
                let oldval = self.cache.push(r, Data::new_clean(val, self.cache_age));
                assert!(oldval.is_none());
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.sql.put_meta("next_id", self.next_id)?;
        self.sql.put_meta("root", self.root)?;

        for (r, data) in self.cache.iter() {
            match data.state {
                DataState::Clean => (),
                DataState::Dirty => {
                    self.sql.put_refs(r.id, &data.data)?;
                }
                DataState::Remove => {
                    self.sql.remove_refs(r.id)?;
                }
            }
        }

        self.sql.commit()?;
        self.sql.begin()?;

        self.cache.retain(|data| {
            if data.state == DataState::Dirty {
                data.state = DataState::Clean;
            }
            data.state != DataState::Remove
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
            let (r, min) = self.cache.pop().expect("the cache is not empty");
            self.cache_age = min.access_count;

            match min.state {
                DataState::Clean => (),
                DataState::Remove => {
                    self.sql.remove_refs(r.id)?;
                }
                DataState::Dirty => {
                    self.sql.put_refs(r.id, min.data)?;
                }
            }
        }
        Ok(())
    }
}

impl Ref {
    const fn new(id: Uuid) -> Self {
        Self { id }
    }

    pub const fn null() -> Self {
        Self::new(Uuid::max_value())
    }

    pub fn is_null(&self) -> bool {
        self == &Self::null()
    }
}

impl<T> Data<T> {
    fn new_dirty(data: T, access_count: usize) -> Self {
        Self {
            data,
            state: DataState::Dirty,
            access_count,
        }
    }

    fn new_clean(data: T, access_count: usize) -> Self {
        Self {
            data,
            state: DataState::Clean,
            access_count,
        }
    }
}

impl<T> PartialEq for Data<T> {
    fn eq(&self, other: &Self) -> bool {
        self.access_count.eq(&other.access_count)
    }
}
impl<T> Eq for Data<T> {}
impl<T> PartialOrd for Data<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.access_count.partial_cmp(&other.access_count)
    }
}
impl<T> Ord for Data<T> {
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
            self.cache.clear();
            Ok(())
        }

        fn state_of(&self, r: Ref) -> Option<DataState> {
            self.cache.get(&r).map(|d| d.state)
        }
    }

    #[test]
    fn test_insert() -> Result<()> {
        let mut db = Heap::<i32>::new_in_memory()?;
        let r = db.allocate();
        db.set(r, 5)?;
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(DataState::Dirty), db.state_of(r));

        db.reset()?;
        assert_eq!(None, db.state_of(r));
        assert_eq!(Some(&5), db.deref(r)?);
        assert_eq!(Some(DataState::Clean), db.state_of(r));

        assert_eq!(Some(&mut 5), db.deref_mut(r)?);
        assert_eq!(Some(DataState::Dirty), db.state_of(r));

        assert_eq!(Uuid::min_value() + 1, db.next_id);
        Ok(())
    }

    #[test]
    fn test_remove() -> Result<()> {
        let mut db = Heap::<i32>::new_in_memory()?;
        // TODO: testa att det inte går att hämta saker som är borttagna bland annat
        Ok(())
    }
}
