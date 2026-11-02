//! Embedded SQL without SQLx's unused database-driver macro dependency graph.
use sha2::{Digest, Sha384};
use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::Row;
use std::borrow::Cow;

// Preserve checksum validation across Windows checkouts and Linux archives.
// Only byte-for-byte LF/CRLF equivalents qualify; SQL changes still fail.
fn compatible_checksum(sql: &str, stored: &[u8]) -> Option<Vec<u8>> {
    let lf = sql.replace("\r\n", "\n");
    [lf.clone(), lf.replace('\n', "\r\n")]
        .into_iter()
        .map(|text| Sha384::digest(text.as_bytes()).to_vec())
        .find(|checksum| checksum == stored)
}

pub async fn run(db: &sqlx::PgPool) -> anyhow::Result<()> {
    let mut migrator = embedded();
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(db)
        .await?;
    if exists {
        let applied = sqlx::query("SELECT version,checksum FROM _sqlx_migrations WHERE success")
            .fetch_all(db)
            .await?;
        for migration in migrator.migrations.to_mut() {
            if let Some(row) = applied
                .iter()
                .find(|row| row.get::<i64, _>("version") == migration.version)
            {
                if let Some(checksum) =
                    compatible_checksum(&migration.sql, &row.get::<Vec<u8>, _>("checksum"))
                {
                    migration.checksum = Cow::Owned(checksum);
                }
            }
        }
    }
    migrator.run(db).await?;
    Ok(())
}
pub fn embedded() -> Migrator {
    Migrator {
        migrations: Cow::Owned(vec![
            Migration::new(
                1,
                "core".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0001_core.sql").into(),
                false,
            ),
            Migration::new(
                2,
                "rule retirement".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0002_rule_retirement.sql").into(),
                false,
            ),
            Migration::new(
                3,
                "history exports".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0003_history_exports.sql").into(),
                false,
            ),
            Migration::new(
                4,
                "remembered members".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0004_remembered_members.sql").into(),
                false,
            ),
            Migration::new(
                5,
                "professions".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0005_professions.sql").into(),
                false,
            ),
            Migration::new(
                6,
                "auction recipe discovery".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0006_auction_recipe_discovery.sql").into(),
                false,
            ),
            Migration::new(
                7,
                "price forecasts".into(),
                MigrationType::Simple,
                include_str!("../../../db/migrations/0007_forecasts.sql").into(),
                false,
            ),
        ]),
        ..Migrator::DEFAULT
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn line_endings_match_but_sql_changes_do_not() {
        use sha2::{Digest, Sha384};
        for migration in super::embedded().iter() {
            let lf = migration.sql.replace("\r\n", "\n");
            for sql in [&lf, &lf.replace('\n', "\r\n")] {
                let checksum = Sha384::digest(sql.as_bytes()).to_vec();
                assert_eq!(
                    super::compatible_checksum(&migration.sql, &checksum),
                    Some(checksum)
                );
            }
            let changed = Sha384::digest(format!("{lf}-- changed SQL").as_bytes());
            assert_eq!(super::compatible_checksum(&migration.sql, &changed), None);
        }
    }
    #[test]
    fn every_sql_migration_is_embedded() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../db/migrations");
        let count = std::fs::read_dir(directory)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|s| s == "sql")
            })
            .count();
        assert_eq!(
            count,
            super::embedded().iter().count(),
            "Register every new migration in migrations::embedded"
        );
    }
}
