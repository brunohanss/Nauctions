//! Embedded SQL without SQLx's unused database-driver macro dependency graph.
use sqlx::migrate::{Migration, MigrationType, Migrator};
use std::borrow::Cow;
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
        ]),
        ..Migrator::DEFAULT
    }
}
#[cfg(test)]
mod tests {
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
