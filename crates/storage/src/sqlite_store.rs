use rusqlite::{Connection, params};
use journal::StateJournal;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS state (
                address BLOB NOT NULL,
                slot BLOB NOT NULL,
                value BLOB NOT NULL,
                PRIMARY KEY (address, slot)
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn flush(&self, journal: &StateJournal) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO state (address, slot, value) VALUES (?1, ?2, ?3)",
            )?;
            for (key, value) in journal.writes() {
                stmt.execute(params![&key.address[..], &key.slot[..], &value[..]])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get(&self, address: &[u8; 20], slot: &[u8; 32]) -> Result<Option<[u8; 32]>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM state WHERE address = ?1 AND slot = ?2",
        )?;
        let mut rows = stmt.query(params![&address[..], &slot[..]])?;
        if let Some(row) = rows.next()? {
            let val: Vec<u8> = row.get(0)?;
            let mut result = [0u8; 32];
            result.copy_from_slice(&val[..32.min(val.len())]);
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub fn flush_with_deductions(&self, journal: &StateJournal) -> Result<(), StorageError> {
        self.flush(journal)?;
        // Gas deductions are implicit — consumed during execution, no state write needed
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flush_and_read() {
        let store = SqliteStore::open(":memory:").unwrap();
        let mut journal = StateJournal::new();
        let addr = [1u8; 20];
        let slot = [0u8; 32];
        let val = [42u8; 32];
        journal.record_write(addr, slot, val);

        store.flush(&journal).unwrap();
        let got = store.get(&addr, &slot).unwrap().unwrap();
        assert_eq!(got, val);
    }
}
