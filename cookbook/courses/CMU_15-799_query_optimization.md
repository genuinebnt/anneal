# CMU 15-799 Special Topics: Query Optimization (Spring 2025 schedule)

Source: https://15799.courses.cs.cmu.edu/spring2025/schedule.html

Coverage: 22 of 22 recipe slots exist.

| Lecture / topic | Recipes |
|---|---|
| 01 Intro to Query Optimization | [`query_optimizer`](../examples/capstones/query_optimizer.rs) ✅ |
| 02 IBM System R (Selinger DP) | [`query_optimizer`](../examples/capstones/query_optimizer.rs) ✅ |
| 03 IBM Starburst (rule-based rewrite) | [`query_rewrite_rules_magic_sets`](../examples/database/query_rewrite_rules_magic_sets.rs) ✅ |
| 04 Volcano optimizer generator | [`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅ |
| 05 Cascades | [`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅ |
| 06 Transformations | [`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅<br>[`query_rewrite_rules_magic_sets`](../examples/database/query_rewrite_rules_magic_sets.rs) ✅ |
| 07 Join Ordering: Bottom-Up (DPsize, DPhyp, adaptive for large joins) | [`join_enumeration_dpccp_dphyp`](../examples/database/join_enumeration_dpccp_dphyp.rs) ✅ |
| 08 Join Ordering: Top-Down (MinCut) | [`join_enumeration_dpccp_dphyp`](../examples/database/join_enumeration_dpccp_dphyp.rs) ✅ |
| 09-10 Parallelisation of the optimizer | [`join_enumeration_dpccp_dphyp`](../examples/database/join_enumeration_dpccp_dphyp.rs) ✅ |
| 11 Unnesting Queries | [`subquery_unnesting_decorrelation`](../examples/database/subquery_unnesting_decorrelation.rs) ✅ |
| 12 Cost Models: Statistics | [`statistics_histograms_sketches`](../examples/database/statistics_histograms_sketches.rs) ✅<br>[`hyperloglog`](../examples/data_structures/hyperloglog.rs) ✅<br>[`count_min_sketch`](../examples/data_structures/count_min_sketch.rs) ✅ |
| 13-14 Cost Models: Cardinality Estimation | [`cardinality_estimation_lab`](../examples/database/cardinality_estimation_lab.rs) ✅ |
| 15 Cost Models: Learned Approaches | [`cardinality_estimation_lab`](../examples/database/cardinality_estimation_lab.rs) ✅ |
| 16 Parametric Query Optimization | [`parametric_query_optimization`](../examples/database/parametric_query_optimization.rs) ✅ |
| 17-18 Adaptivity: Feedback and Runtime Processing | [`adaptive_feedback_reoptimization`](../examples/database/adaptive_feedback_reoptimization.rs) ✅<br>[`adaptive_query_processing_eddies_sip`](../examples/database/adaptive_query_processing_eddies_sip.rs) ✅ |
| 19 User-Defined Function Optimizations | [`plan_cache_multiplan_trial`](../examples/database/plan_cache_multiplan_trial.rs) ✅ |
| 20-23 Implementations: Calcite, Orca, MongoDB, Microsoft Fabric | [`cascades_memo_optimizer`](../examples/database/cascades_memo_optimizer.rs) ✅<br>[`plan_cache_multiplan_trial`](../examples/database/plan_cache_multiplan_trial.rs) ✅ |

How to use: read the lecture's papers, attempt the recipe's asserts yourself (see ../HOW_TO_STUDY.md), then compare.
