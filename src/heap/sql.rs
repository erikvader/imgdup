use std::io::Write;

use rusqlite::{blob::ZeroBlob, DatabaseName, ToSql, OptionalExtension};
use serde::de::DeserializeOwned;

use super::*;

impl<T> Heap<T> {
    pub(super) fn create_tables(&self) -> Result<()> {
        Ok(self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS keys(key INTEGER PRIMARY KEY, value BLOB NOT NULL) STRICT;
             CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;"
        )?)
    }

    fn put_kv<K, V>(&self, table: &str, key: K, value: V) -> Result<()>
    where V: Serialize,
    K: ToSql,
    {
        let value = bincode::serialize(&value)?;
        let len: i32 = value.len().try_into().expect("A blob should not be this big anyway");

        let rowid: i64 = self.db.query_row(
            &format!("INSERT INTO {}(key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value
             RETURNING rowid", table),
            (key, ZeroBlob(len)),
            |row| row.get(0))?;

        let mut blob = self.db.blob_open(DatabaseName::Main, table, "value", rowid, false)?;

        let written = blob.write(&value)?;
        assert_eq!(written, value.len());

        Ok(())
    }

    fn get_kv<K, V>(&self, table: &str, key: K) -> Result<Option<V>>
    where V: DeserializeOwned,
    K: ToSql,
    {
        let Some(rowid) = self.db.query_row(
            &format!("SELECT rowid FROM {} WHERE key=?1", table),
            (key,),
            |row| row.get(0),
        ).optional()? else {
            return Ok(None);
        };

        let blob = self.db.blob_open(DatabaseName::Main, table, "value", rowid, true)?;
        Ok(Some(bincode::deserialize_from(blob)?))
    }

    pub(super) fn put_meta<V>(&self, key: &str, value: V) -> Result<()>
    where V: Serialize,
    {
        self.put_kv("meta", key, value)
    }

    pub(super) fn get_meta<V>(&self, key: &str) -> Result<Option<V>>
    where V: DeserializeOwned,
    {
        self.get_kv("meta", key)
    }

    pub(super) fn put_keys<V>(&self, key: i64, value: V) -> Result<()>
    where V: Serialize,
    {
        self.put_kv("keys", key, value)
    }

    pub(super) fn get_keys<V>(&self, key: i64) -> Result<Option<V>>
    where V: DeserializeOwned,
    {
        self.get_kv("keys", key)
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
    fn test_keys() {
        let heap = Heap::<()>::new_in_memory().unwrap();
        heap.put_keys(1, "omg").unwrap();
        assert_eq!("omg", heap.get_keys::<String>(1).unwrap().unwrap());

        heap.put_keys(5, "asd").unwrap();
        heap.put_keys(1, "qwe").unwrap();

        assert_eq!("asd", heap.get_keys::<String>(5).unwrap().unwrap());
        assert_eq!("qwe", heap.get_keys::<String>(1).unwrap().unwrap());
    }

    #[test]
    fn test_absent() {
        let heap = Heap::<()>::new_in_memory().unwrap();
        assert!(heap.get_meta::<()>("asd").unwrap().is_none());
    }
}
