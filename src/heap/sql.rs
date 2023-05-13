use std::io::Write;

use once_cell::sync::Lazy;
use rusqlite::{blob::ZeroBlob, DatabaseName, OptionalExtension, ToSql};
use serde::de::DeserializeOwned;

use super::*;

#[derive(Clone, Copy)]
enum Table {
    Refs,
    Meta,
}

impl Table {
    const fn str(&self) -> &'static str {
        match self {
            Self::Refs => "refs",
            Self::Meta => "meta",
        }
    }
}

fn put_query(table: Table) -> String {
    format!(
        "INSERT INTO {}(key, value)
         VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value
         RETURNING rowid",
        table.str()
    )
}

fn get_query(table: Table) -> String {
    format!("SELECT rowid FROM {} WHERE key=?1", table.str())
}

impl<T> Heap<T> {
    pub(super) fn create_tables(&self) -> Result<()> {
        static CREATE_QUERY: Lazy<String> = Lazy::new(|| {
            let refs = Table::Refs.str();
            let meta = Table::Meta.str();
            format!(
                "CREATE TABLE IF NOT EXISTS {refs}(key INTEGER PRIMARY KEY, value BLOB NOT NULL) STRICT;
                 CREATE TABLE IF NOT EXISTS {meta}(key TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;"
            )
        });
        Ok(self.db.execute_batch(&CREATE_QUERY)?)
    }

    fn put_kv<K, V>(&self, put_query: &str, table: Table, key: K, value: V) -> Result<()>
    where
        V: Serialize,
        K: ToSql,
    {
        let value = bincode::serialize(&value)?;
        let len: i32 = value
            .len()
            .try_into()
            .expect("A blob should not be this big anyway");

        let rowid: i64 = self
            .db
            .query_row(put_query, (key, ZeroBlob(len)), |row| row.get(0))?;

        let mut blob =
            self.db
                .blob_open(DatabaseName::Main, table.str(), "value", rowid, false)?;

        let written = blob.write(&value)?;
        assert_eq!(written, value.len());

        Ok(())
    }

    fn get_kv<K, V>(&self, get_query: &str, table: Table, key: K) -> Result<Option<V>>
    where
        V: DeserializeOwned,
        K: ToSql,
    {
        let Some(rowid) = self.db.query_row(
            get_query,
            (key,),
            |row| row.get(0),
        ).optional()? else {
            return Ok(None);
        };

        let blob =
            self.db
                .blob_open(DatabaseName::Main, table.str(), "value", rowid, true)?;
        Ok(Some(bincode::deserialize_from(blob)?))
    }

    pub(super) fn put_meta<V>(&self, key: &str, value: V) -> Result<()>
    where
        V: Serialize,
    {
        static PUT_QUERY: Lazy<String> = Lazy::new(|| put_query(Table::Meta));
        self.put_kv(&PUT_QUERY, Table::Meta, key, value)
    }

    pub(super) fn get_meta<V>(&self, key: &str) -> Result<Option<V>>
    where
        V: DeserializeOwned,
    {
        static GET_QUERY: Lazy<String> = Lazy::new(|| get_query(Table::Meta));
        self.get_kv(&GET_QUERY, Table::Meta, key)
    }

    pub(super) fn put_refs<V>(&self, key: i64, value: V) -> Result<()>
    where
        V: Serialize,
    {
        static PUT_QUERY: Lazy<String> = Lazy::new(|| put_query(Table::Refs));
        self.put_kv(&PUT_QUERY, Table::Refs, key, value)
    }

    pub(super) fn get_refs<V>(&self, key: i64) -> Result<Option<V>>
    where
        V: DeserializeOwned,
    {
        static GET_QUERY: Lazy<String> = Lazy::new(|| get_query(Table::Refs));
        self.get_kv(&GET_QUERY, Table::Refs, key)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_meta() {
        let heap = Heap::<()>::new_in_memory().unwrap();
        heap.put_meta("asd", 42).unwrap();
        assert_eq!(42, heap.get_meta("asd").unwrap().unwrap());

        heap.put_meta("omg", 3).unwrap();
        heap.put_meta("asd", 69).unwrap();

        assert_eq!(3, heap.get_meta("omg").unwrap().unwrap());
        assert_eq!(69, heap.get_meta("asd").unwrap().unwrap());
    }

    #[test]
    fn test_refs() {
        let heap = Heap::<()>::new_in_memory().unwrap();
        heap.put_refs(1, "omg").unwrap();
        assert_eq!("omg", heap.get_refs::<String>(1).unwrap().unwrap());

        heap.put_refs(5, "asd").unwrap();
        heap.put_refs(1, "qwe").unwrap();

        assert_eq!("asd", heap.get_refs::<String>(5).unwrap().unwrap());
        assert_eq!("qwe", heap.get_refs::<String>(1).unwrap().unwrap());
    }

    #[test]
    fn test_absent() {
        let heap = Heap::<()>::new_in_memory().unwrap();
        assert!(heap.get_meta::<()>("asd").unwrap().is_none());
    }
}
