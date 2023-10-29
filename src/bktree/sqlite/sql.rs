#![allow(dead_code)]
use std::{io::Write, path::Path};

use super::heap::Result;
use const_format::concatcp;
use rusqlite::{blob::ZeroBlob, Connection, DatabaseName, OptionalExtension, ToSql};
use serde::{de::DeserializeOwned, Serialize};

pub(super) struct Sql {
    db: Connection,
}

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

macro_rules! put_query {
    ($tab:expr) => {
        concatcp!(
            "INSERT INTO ",
            $tab,
            "(key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value
             RETURNING rowid",
        )
    };
}

macro_rules! get_query {
    ($tab:expr) => {
        concatcp!("SELECT rowid FROM ", $tab, " WHERE key=?1")
    };
}

macro_rules! remove_query {
    ($tab:expr) => {
        concatcp!("DELETE FROM ", $tab, " WHERE key=?1")
    };
}

macro_rules! count_query {
    ($tab:expr) => {
        concatcp!("SELECT COUNT(*) FROM ", $tab)
    };
}

impl Sql {
    pub(super) fn new_in_memory() -> Result<Self> {
        let myself = Self {
            db: Connection::open_in_memory()?,
        };
        myself.init_db()?;
        Ok(myself)
    }

    pub(super) fn new_from_file(file: impl AsRef<Path>) -> Result<Self> {
        let myself = Self {
            db: Connection::open(file)?,
        };
        myself.init_db()?;
        Ok(myself)
    }

    fn init_db(&self) -> Result<()> {
        let refs = Table::Refs.str();
        let meta = Table::Meta.str();
        let query = format!(
            // NOTE: These pragmas will enable a write-ahead log without a shared memory
            // file (locking_mode=EXCLUSIVE) and no fsync on the wal-file
            // (synchronous=NORMAL). The writes will only be fsynced into the database
            // when the wal gets too big or when the connection is closed.
            "PRAGMA locking_mode=EXCLUSIVE;
             PRAGMA synchronous=NORMAL;
             PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS {refs}(key INTEGER PRIMARY KEY, value BLOB NOT NULL) STRICT;
             CREATE TABLE IF NOT EXISTS {meta}(key TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;"
            );
        Ok(self.db.execute_batch(&query)?)
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

        let mut stmt = self.db.prepare_cached(put_query)?;
        let rowid: i64 = stmt.query_row((key, ZeroBlob(len)), |row| row.get(0))?;

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
        let mut stmt = self.db.prepare_cached(get_query)?;
        let Some(rowid) = stmt.query_row(
            (key,),
            |row| row.get(0),
        ).optional()? else {
            return Ok(None);
        };

        let blob =
            self.db
                .blob_open(DatabaseName::Main, table.str(), "value", rowid, true)?;
        Ok(Some(bincode::deserialize_from::<_, V>(blob)?))
    }

    fn remove_kv<K>(&self, remove_query: &str, key: K) -> Result<bool>
    where
        K: ToSql,
    {
        let mut stmt = self.db.prepare_cached(remove_query)?;
        let affected = stmt.execute((key,))?;
        Ok(affected > 0)
    }

    pub(super) fn put_meta<V>(&self, key: &str, value: V) -> Result<()>
    where
        V: Serialize,
    {
        self.put_kv(put_query!(Table::Meta.str()), Table::Meta, key, value)
    }

    pub(super) fn get_meta<V>(&self, key: &str) -> Result<Option<V>>
    where
        V: DeserializeOwned,
    {
        self.get_kv(get_query!(Table::Meta.str()), Table::Meta, key)
    }

    pub(super) fn remove_meta(&self, key: &str) -> Result<bool> {
        self.remove_kv(remove_query!(Table::Meta.str()), key)
    }

    pub(super) fn put_refs<V>(&self, key: i64, value: V) -> Result<()>
    where
        V: Serialize,
    {
        self.put_kv(put_query!(Table::Refs.str()), Table::Refs, key, value)
    }

    pub(super) fn get_refs<V>(&self, key: i64) -> Result<Option<V>>
    where
        V: DeserializeOwned,
    {
        self.get_kv(get_query!(Table::Refs.str()), Table::Refs, key)
    }

    pub(super) fn remove_refs(&self, key: i64) -> Result<bool> {
        self.remove_kv(remove_query!(Table::Refs.str()), key)
    }

    pub(super) fn count_refs(&self) -> Result<usize> {
        let count = self
            .db
            .query_row(count_query!(Table::Refs.str()), (), |row| row.get(0))?;
        Ok(count)
    }

    pub(super) fn begin(&self) -> Result<()> {
        self.db.execute("BEGIN", ())?;
        Ok(())
    }

    pub(super) fn rollback(&self) -> Result<()> {
        self.db.execute("ROLLBACK", ())?;
        Ok(())
    }

    pub(super) fn commit(&self) -> Result<()> {
        self.db.execute("COMMIT", ())?;
        Ok(())
    }

    pub(super) fn close(self) -> Result<()> {
        self.db.close().map_err(|(_, e)| e.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_meta() -> Result<()> {
        let sql = Sql::new_in_memory()?;
        sql.put_meta("asd", 42i32)?;
        assert_eq!(42i32, sql.get_meta::<i32>("asd")?.unwrap());

        sql.put_meta("omg", 3i32)?;
        sql.put_meta("asd", 69i32)?;

        assert_eq!(3i32, sql.get_meta::<i32>("omg")?.unwrap());
        assert_eq!(69i32, sql.get_meta::<i32>("asd")?.unwrap());

        assert!(sql.remove_meta("omg")?);
        assert_eq!(None::<i32>, sql.get_meta("omg")?);
        assert!(!sql.remove_meta("omg")?);
        Ok(())
    }

    #[test]
    fn test_refs() -> Result<()> {
        let sql = Sql::new_in_memory()?;
        sql.put_refs(1, "omg")?;
        assert_eq!("omg", sql.get_refs::<String>(1)?.unwrap());

        sql.put_refs(5, "asd")?;
        sql.put_refs(1, "qwe")?;

        assert_eq!("asd", sql.get_refs::<String>(5)?.unwrap());
        assert_eq!("qwe", sql.get_refs::<String>(1)?.unwrap());

        assert_eq!(2, sql.count_refs()?);

        sql.remove_refs(5)?;
        assert_eq!(1, sql.count_refs()?);

        sql.remove_refs(1)?;
        assert_eq!(0, sql.count_refs()?);
        Ok(())
    }

    #[test]
    fn test_absent() -> Result<()> {
        let sql = Sql::new_in_memory()?;
        assert!(sql.get_meta::<()>("asd")?.is_none());
        Ok(())
    }
}
