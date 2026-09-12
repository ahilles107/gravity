//! Versioned SQLite schema migrations.
//!
//! Migrations are applied in order inside a transaction; the current version
//! is stored in `meta(key='schema_version')`. The index of a migration in
//! `MIGRATIONS` is its version, so entries are only ever appended.

mod base;
mod decisions;
mod history;

use base::MIGRATION_1;
use decisions::MIGRATION_12;
use history::{
    MIGRATION_10, MIGRATION_11, MIGRATION_2, MIGRATION_3, MIGRATION_4, MIGRATION_5, MIGRATION_6,
    MIGRATION_7, MIGRATION_8, MIGRATION_9,
};

pub const MIGRATIONS: &[&str] = &[
    MIGRATION_1,
    MIGRATION_2,
    MIGRATION_3,
    MIGRATION_4,
    MIGRATION_5,
    MIGRATION_6,
    MIGRATION_7,
    MIGRATION_8,
    MIGRATION_9,
    MIGRATION_10,
    MIGRATION_11,
    MIGRATION_12,
];
