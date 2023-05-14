use std::io::Write;

use once_cell::sync::Lazy;
use rusqlite::{blob::ZeroBlob, DatabaseName, OptionalExtension, ToSql, Transaction};
use serde::de::DeserializeOwned;

use super::*;

pub(super) struct Sql {
    db: Connection,
}

pub(super) struct Trans<'a> {
    db: Transaction<'a>,
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

// TODO: prepare
fn put_query(table: Table) -> String {
    format!(
        "INSERT INTO {}(key, value)
         VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value
         RETURNING rowid",
        table.str()
    )
}

// TODO: prepare
fn get_query(table: Table) -> String {
    format!("SELECT rowid FROM {} WHERE key=?1", table.str())
}

// TODO: prepare
fn remove_query(table: Table) -> String {
    format!("DELETE FROM {} WHERE key=?1", table.str())
}

impl Sql {
    pub(super) fn new_in_memory() -> Result<Self> {
        let myself = Self {
            db: Connection::open_in_memory()?,
        };
        myself.create_tables()?;
        Ok(myself)
    }

    pub(super) fn new_from_file(file: impl AsRef<Path>) -> Result<Self> {
        let myself = Self {
            db: Connection::open(file)?,
        };
        myself.create_tables()?;
        Ok(myself)
    }

    fn create_tables(&self) -> Result<()> {
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

    pub(super) fn transaction(&mut self) -> Result<Trans<'_>> {
        Ok(Trans {
            db: self.db.transaction()?,
        })
    }
}

impl Trans<'_> {
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
        Ok(Some(bincode::deserialize_from::<_, V>(blob)?))
    }

    fn remove_kv<K>(&self, remove_query: &str, key: K) -> Result<bool>
    where
        K: ToSql,
    {
        let affected = self.db.execute(remove_query, (key,))?;
        Ok(affected > 0)
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

    pub(super) fn remove_meta(&self, key: &str) -> Result<bool> {
        static REMOVE_QUERY: Lazy<String> = Lazy::new(|| remove_query(Table::Meta));
        self.remove_kv(&REMOVE_QUERY, key)
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

    pub(super) fn remove_refs(&self, key: i64) -> Result<bool> {
        static REMOVE_QUERY: Lazy<String> = Lazy::new(|| remove_query(Table::Refs));
        self.remove_kv(&REMOVE_QUERY, key)
    }

    pub(super) fn commit(self) -> Result<()> {
        Ok(self.db.commit()?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_meta() -> Result<()> {
        let mut sql = Sql::new_in_memory()?;
        let trans = sql.transaction()?;
        trans.put_meta("asd", 42)?;
        assert_eq!(42, trans.get_meta("asd")?.unwrap());

        trans.put_meta("omg", 3)?;
        trans.put_meta("asd", 69)?;

        assert_eq!(3, trans.get_meta("omg")?.unwrap());
        assert_eq!(69, trans.get_meta("asd")?.unwrap());

        assert!(trans.remove_meta("omg")?);
        assert_eq!(None::<i32>, trans.get_meta("omg")?);
        assert!(!trans.remove_meta("omg")?);
        Ok(())
    }

    #[test]
    fn test_refs() -> Result<()> {
        let mut sql = Sql::new_in_memory()?;
        let trans = sql.transaction()?;
        trans.put_refs(1, "omg")?;
        assert_eq!("omg", trans.get_refs::<String>(1)?.unwrap());

        trans.put_refs(5, "asd")?;
        trans.put_refs(1, "qwe")?;

        assert_eq!("asd", trans.get_refs::<String>(5)?.unwrap());
        assert_eq!("qwe", trans.get_refs::<String>(1)?.unwrap());
        Ok(())
    }

    #[test]
    fn test_absent() -> Result<()> {
        let mut sql = Sql::new_in_memory()?;
        let trans = sql.transaction()?;
        assert!(trans.get_meta::<()>("asd")?.is_none());
        Ok(())
    }
}
