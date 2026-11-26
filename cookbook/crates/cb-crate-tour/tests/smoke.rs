//! Runs every example's `main` (all of its assertions) as a test, so `cargo test -p cb-crate-tour` covers the whole chapter.
//! Each example is compiled here as a module. Its `#[cfg(test)]` items, if any, run too.
#![allow(dead_code)]
use std::sync::Mutex;

/// Some examples silence a deliberate panic by swapping the PROCESS-WIDE panic hook. Two doing it at once could leave the
/// silent hook installed, so they take turns.
static PANIC_HOOK: Mutex<()> = Mutex::new(());

#[path = "../examples/anyhow_thiserror_eyre_error_design.rs"]
mod anyhow_thiserror_eyre_error_design;
#[path = "../examples/bytes_buf_buf_mut_framing.rs"]
mod bytes_buf_buf_mut_framing;
#[path = "../examples/chrono_dates_times_zones.rs"]
mod chrono_dates_times_zones;
#[path = "../examples/clap_derive_cli_tool.rs"]
mod clap_derive_cli_tool;
#[path = "../examples/criterion_proptest_insta_testing.rs"]
mod criterion_proptest_insta_testing;
#[path = "../examples/indexmap_smallvec_arrayvec_collections.rs"]
mod indexmap_smallvec_arrayvec_collections;
#[path = "../examples/itertools_adaptors_cookbook.rs"]
mod itertools_adaptors_cookbook;
#[path = "../examples/regex_text_processing.rs"]
mod regex_text_processing;
#[path = "../examples/reqwest_axum_http_roundtrip.rs"]
mod reqwest_axum_http_roundtrip;
#[path = "../examples/serde_attributes_enum_representations.rs"]
mod serde_attributes_enum_representations;
#[path = "../examples/serde_formats_json_toml_yaml_bincode.rs"]
mod serde_formats_json_toml_yaml_bincode;
#[path = "../examples/sqlx_sqlite_queries_transactions.rs"]
mod sqlx_sqlite_queries_transactions;
#[path = "../examples/tempfile_walkdir_glob_ignore.rs"]
mod tempfile_walkdir_glob_ignore;
#[path = "../examples/time_crate_dates_formatting.rs"]
mod time_crate_dates_formatting;
#[path = "../examples/tokio_runtime_tasks_channels_timers.rs"]
mod tokio_runtime_tasks_channels_timers;
#[path = "../examples/tracing_spans_subscriber_layers.rs"]
mod tracing_spans_subscriber_layers;
#[path = "../examples/url_base64_hex_percent_encoding.rs"]
mod url_base64_hex_percent_encoding;
#[path = "../examples/uuid_rand_ids_tokens.rs"]
mod uuid_rand_ids_tokens;

macro_rules! smoke {
    ($($test:ident => $module:ident $(, $lock:ident)?;)*) => {$(
        #[test]
        fn $test() {
            $(let _turn = $lock.lock().unwrap_or_else(|e| e.into_inner());)?
            $module::main();
        }
    )*};
}

smoke! {
    runs_chrono_dates_times_zones => chrono_dates_times_zones;
    runs_time_crate_dates_formatting => time_crate_dates_formatting;
    runs_serde_attributes_enum_representations => serde_attributes_enum_representations;
    runs_serde_formats_json_toml_yaml_bincode => serde_formats_json_toml_yaml_bincode;
    runs_sqlx_sqlite_queries_transactions => sqlx_sqlite_queries_transactions;
    runs_itertools_adaptors_cookbook => itertools_adaptors_cookbook, PANIC_HOOK;
    runs_regex_text_processing => regex_text_processing;
    runs_uuid_rand_ids_tokens => uuid_rand_ids_tokens;
    runs_bytes_buf_buf_mut_framing => bytes_buf_buf_mut_framing, PANIC_HOOK;
    runs_indexmap_smallvec_arrayvec_collections => indexmap_smallvec_arrayvec_collections;
    runs_url_base64_hex_percent_encoding => url_base64_hex_percent_encoding;
    runs_anyhow_thiserror_eyre_error_design => anyhow_thiserror_eyre_error_design;
    runs_clap_derive_cli_tool => clap_derive_cli_tool;
    runs_tracing_spans_subscriber_layers => tracing_spans_subscriber_layers;
    runs_tokio_runtime_tasks_channels_timers => tokio_runtime_tasks_channels_timers, PANIC_HOOK;
    runs_reqwest_axum_http_roundtrip => reqwest_axum_http_roundtrip;
    runs_tempfile_walkdir_glob_ignore => tempfile_walkdir_glob_ignore;
    runs_criterion_proptest_insta_testing => criterion_proptest_insta_testing;
}
