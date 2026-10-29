//! PostgreSQL-only SQLx surface. Avoid resolving unused MySQL/RSA dependencies.
pub use sqlx_core::error::Error;
pub use sqlx_core::executor::Executor;
pub use sqlx_core::migrate;
pub use sqlx_core::query::query;
pub use sqlx_core::query_scalar::query_scalar;
pub use sqlx_core::row::Row;
pub use sqlx_core::transaction::Transaction;
pub use sqlx_postgres::{self as postgres, PgPool, Postgres};
