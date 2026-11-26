# Time-series databases

Source: CMU Database Group, Time Series Database Lectures (Fall 2017): InfluxDB storage engine, Two Sigma Smooth Storage, QuasarDB, TimescaleDB, Heron; https://db.cs.cmu.edu/seminars/fall2017/ . No dedicated CMU time-series course page was found; topics below follow that series plus standard literature (Gorilla, t-digest).

Coverage: 5 of 11 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| InfluxDB storage engine internals (WAL, cache, TSM files, series index) | `tsm_time_series_engine` 🔜 |
| Gorilla compression (delta-of-delta timestamps, XOR floats) | `timeseries_gorilla` 🔜 |
| TimescaleDB: hypertables, chunking, chunk exclusion, continuous aggregates | `hypertables_continuous_aggregates` 🔜 |
| Cardinality: series churn, HyperLogLog | [`hyperloglog`](../examples/data_structures/hyperloglog.rs) ✅ |
| Downsampling, quantile sketches, anomaly detection | `timeseries_sketches_downsampling_anomaly` 🔜 |
| As-of joins, gap filling, out-of-order ingestion | `asof_joins_gap_filling` 🔜 |
| Streaming: windows, watermarks, late data (Heron-style) | `stream_processing_windows` 🔜 |
| Columnar encodings for numeric series | [`columnar_encodings`](../examples/database/columnar_encodings.rs) ✅<br>[`record_batch_codec`](../examples/distributed/record_batch_codec.rs) ✅ |
| Retention, compaction, LSM foundations | [`mini_kv_store`](../examples/capstones/mini_kv_store.rs) ✅<br>[`lsm_compaction_strategies`](../examples/database/lsm_compaction_strategies.rs) ✅ |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
