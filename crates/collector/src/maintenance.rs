use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

const RECEIPT_RETENTION: i64 = 30 * 24 * 60 * 60;

pub fn migrate(db: &mut Connection, now: i64) -> Result<()> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let names = {
        let mut columns = tx.prepare("PRAGMA table_info(queue)")?;
        let names = columns
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        names
    };
    if !names.iter().any(|name| name == "finished_at") {
        tx.execute("ALTER TABLE queue ADD COLUMN finished_at INTEGER", [])?;
    }
    if !names.iter().any(|name| name == "created_at") {
        tx.execute("ALTER TABLE queue ADD COLUMN created_at INTEGER", [])?;
        tx.execute(
            "UPDATE queue SET created_at=? WHERE created_at IS NULL",
            [now],
        )?;
    }
    tx.execute(
        "CREATE INDEX IF NOT EXISTS queue_cleanup ON queue(state,finished_at)",
        [],
    )?;
    // Legacy terminal receipts get a full retention period after migration.
    tx.execute("UPDATE queue SET finished_at=? WHERE state IN ('accepted','discarded','rejected') AND finished_at IS NULL", [now])?;
    tx.commit()?;
    Ok(())
}

pub fn prune(db: &Connection, now: i64) -> Result<usize> {
    Ok(db.execute("DELETE FROM queue WHERE state IN ('accepted','discarded') AND payload IS NULL AND finished_at<=?", [now.saturating_sub(RECEIPT_RETENTION)])?)
}

pub fn list_rejections(db: &Connection, limit: u32) -> Result<Vec<serde_json::Value>> {
    if !(1..=500).contains(&limit) {
        bail!("limit must be between 1 and 500");
    }
    let mut query = db.prepare("SELECT id,checksum,length(payload),error FROM queue WHERE state='rejected' AND payload IS NOT NULL ORDER BY rowid LIMIT ?")?;
    let rows = query
        .query_map([limit], |row| {
            Ok(serde_json::json!({
                "upload_id": row.get::<_,String>(0)?, "sha256": row.get::<_,String>(1)?,
                "bytes": row.get::<_,i64>(2)?, "error": row.get::<_,Option<String>>(3)?
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn discard(
    db: &mut Connection,
    id: &str,
    checksum: &str,
    confirmed: bool,
    now: i64,
) -> Result<()> {
    if !confirmed {
        bail!("review the rejection, then add --confirm to discard its payload");
    }
    let tx = db.transaction()?;
    let record: Option<(String, Vec<u8>)> = tx.query_row(
        "SELECT checksum,payload FROM queue WHERE id=? AND state='rejected' AND payload IS NOT NULL",
        [id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let Some((stored, bytes)) = record else {
        bail!(
            "no retained rejected payload has this upload ID; pending uploads cannot be discarded"
        );
    };
    if checksum != stored || crate::hash(&bytes) != stored {
        bail!("checksum does not match the retained payload; nothing was discarded");
    }
    tx.execute("UPDATE queue SET state='discarded',payload=NULL,finished_at=? WHERE id=? AND state='rejected'", params![now,id])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_discard_and_retention_never_remove_pending_or_unreviewed_bytes() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE queue(id TEXT PRIMARY KEY,checksum TEXT,payload BLOB,state TEXT,error TEXT); CREATE TABLE sources(source TEXT PRIMARY KEY,checksum TEXT);")?;
        let checksum = crate::hash(b"payload");
        for (id, state) in [
            ("queued", "queued"),
            ("received", "received"),
            ("rejected", "rejected"),
            ("accepted", "accepted"),
        ] {
            db.execute(
                "INSERT INTO queue VALUES(?,?,?,?,NULL)",
                params![id, checksum, b"payload", state],
            )?;
        }
        db.execute("INSERT INTO sources VALUES('source',?)", [&checksum])?;
        migrate(&mut db, 100)?;
        assert_eq!(list_rejections(&db, 10)?.len(), 1);
        assert!(discard(&mut db, "rejected", &checksum, false, 100).is_err());
        assert!(discard(&mut db, "rejected", "wrong", true, 100).is_err());
        for id in ["queued", "received", "accepted"] {
            assert!(discard(&mut db, id, &checksum, true, 100).is_err());
        }
        assert_eq!(
            prune(&db, RECEIPT_RETENTION + 101)?,
            0,
            "payload-bearing rows must not be pruned"
        );
        discard(&mut db, "rejected", &checksum, true, 100)?;
        assert_eq!(prune(&db, RECEIPT_RETENTION + 99)?, 0);
        assert_eq!(prune(&db, RECEIPT_RETENTION + 101)?, 1);
        db.execute("UPDATE queue SET payload=NULL WHERE id='accepted'", [])?;
        assert_eq!(prune(&db, RECEIPT_RETENTION + 101)?, 1);
        let pending: i64 = db.query_row(
            "SELECT count(*) FROM queue WHERE payload IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(pending, 2);
        assert_eq!(
            db.query_row(
                "SELECT checksum FROM sources WHERE source='source'",
                [],
                |row| row.get::<_, String>(0)
            )?,
            checksum
        );
        migrate(&mut db, RECEIPT_RETENTION + 500)?;
        Ok(())
    }

    #[test]
    fn corrupted_payload_is_kept_for_review() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE queue(id TEXT PRIMARY KEY,checksum TEXT,payload BLOB,state TEXT,error TEXT);")?;
        migrate(&mut db, 100)?;
        let checksum = crate::hash(b"original");
        db.execute(
            "INSERT INTO queue(id,checksum,payload,state) VALUES('bad',?,?,'rejected')",
            params![checksum, b"changed"],
        )?;
        assert!(discard(&mut db, "bad", &checksum, true, 100).is_err());
        assert_eq!(list_rejections(&db, 10)?.len(), 1);
        Ok(())
    }
}
