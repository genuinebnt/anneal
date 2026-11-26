//! `sqlx::migrate!` embeds the SQL files at compile time; without this, editing a migration alone
//! would not trigger a rebuild.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
