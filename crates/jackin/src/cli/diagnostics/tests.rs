#[cfg(test)]
use super::{
    DiagnosticsCompareArgs, DiagnosticsCompareBaseline, DiagnosticsCompareFormat,
    comparison_json, comparison_names, docker_build_step_names, format_bytes, format_duration,
    format_startup_delta, last_prewarmed_dind_adoption, max_build_context_bytes,
    max_build_context_files, max_docker_build_step_duration, render_comparison_json,
    resolve_run_path, selected_launch_plan, skipped_timing_detail, skipped_timing_names,
    startup_baseline_duration, startup_spread_summary, truncate_name, validate_compare_args,
    write_compare_output,
    use jackin_core::JackinPaths;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn run_id_resolves_to_diagnostics_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());

        let path = resolve_run_path(&paths, "jk-run-abc123");

        assert_eq!(
            path,
            paths
                .data_dir
                .join("diagnostics")
                .join("runs")
                .join("jk-run-abc123.jsonl")
        );
    }

    #[test]
    fn duration_formatter_uses_seconds_after_one_second() {
        assert_eq!(format_duration(999), "999ms");
        assert_eq!(format_duration(1_250), "1.2s");
    }

    #[test]
    fn startup_delta_formatter_compares_to_fastest_run() {
        assert_eq!(format_startup_delta(Some(1_000), Some(1_000)), "baseline");
        assert_eq!(
            format_startup_delta(Some(3_000), Some(1_000)),
            "+2.0s, 3.0x slower"
        );
        assert_eq!(
            format_startup_delta(Some(1_000), Some(3_000)),
            "-2.0s, 3.0x faster"
        );
        assert_eq!(format_startup_delta(None, Some(1_000)), "no startup span");
    }

    #[test]
    fn startup_spread_summary_names_fastest_slowest_and_spread() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(6_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(1_250)),
        ];
        let spread = startup_spread_summary(&runs, &[]).unwrap();

        assert_eq!(spread.fastest_label, "warm");
        assert_eq!(spread.fastest_ms, 1_250);
        assert_eq!(spread.slowest_label, "cold");
        assert_eq!(spread.slowest_ms, 6_000);
        assert_eq!(spread.spread_ms, 4_750);
    }

    #[test]
    fn startup_baseline_supports_fastest_and_first_run() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(5_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(900)),
            (PathBuf::from("no-hardline.jsonl"), summary_with_stages([])),
        ];

        assert_eq!(
            startup_baseline_duration(&runs, DiagnosticsCompareBaseline::Fastest),
            Some(900)
        );
        assert_eq!(
            startup_baseline_duration(&runs, DiagnosticsCompareBaseline::First),
            Some(5_000)
        );
    }

    #[test]
    fn comparison_json_exports_startup_and_plan_rows() {
        let mut cold = summary_with_startup(5_000);
        cold.run_id = Some("jk-run-cold".to_owned());
        cold.event_count = 42;
        cold.stage_durations_ms
            .insert("derived image".to_owned(), vec![2_000]);
        cold.timing_durations_ms
            .insert("image/docker_build".to_owned(), vec![1_500]);
        cold.build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("workspace".to_owned()),
                files: 7,
                bytes: 2048,
                context_dir: Some("/tmp/context".to_owned()),
            });
        cold.image_build_sources
            .push(jackin_diagnostics::ImageBuildSourceSummary {
                source: Some("workspace_dockerfile".to_owned()),
                reason: Some("missing_local_image".to_owned()),
                base_image: None,
                pull_base_image: false,
            });
        cold.launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan".to_owned(),
                plan: Some("BuildAndCreate".to_owned()),
                reason: Some("missing_local_image".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });
        cold.cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_miss".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "missing".to_owned(),
                detail: Some("missing_local_image".to_owned()),
            });
        cold.cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_hit".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "sibling reused".to_owned(),
                detail: Some("recipe_hash_match".to_owned()),
            });
        cold.docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#46".to_owned(),
                label: "exporting to image".to_owned(),
                duration_ms: Some(76_500),
                cached: false,
            });
        cold.skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "manifest_env".to_owned(),
                detail: "no manifest env entries".to_owned(),
            });
        let mut warm = summary_with_startup(900);
        warm.prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "adopted".to_owned(),
                detail: Some(
                    "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34".to_owned(),
                ),
                reason: None,
                source: Some("state".to_owned()),
                ready_ms: Some(7),
                prewarm_ready_ms: Some(34),
                state_age_ms: Some(12),
            });
        let runs = vec![
            (PathBuf::from("cold.jsonl"), cold),
            (PathBuf::from("warm.jsonl"), warm),
        ];

        let json = comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &[]);

        assert_eq!(json["baseline"], "fastest");
        assert_eq!(json["startup_baseline_ms"], 900);
        assert_eq!(json["fastest_startup_run"]["label"], "warm");
        assert_eq!(json["fastest_startup_run"]["startup_ms"], 900);
        assert_eq!(json["slowest_startup_run"]["run_id"], "jk-run-cold");
        assert_eq!(json["slowest_startup_run"]["startup_ms"], 5_000);
        assert_eq!(json["startup_spread_ms"], 4_100);
        assert_eq!(json["selected_plan_counts"]["BuildAndCreate"], 1);
        assert_eq!(json["selected_plan_counts"]["none"], 1);
        assert_eq!(json["cache_decision_counts"]["image_cache_miss"], 1);
        assert_eq!(json["cache_decision_counts"]["none"], 1);
        assert_eq!(json["prewarmed_dind_adoption_counts"]["adopted"], 1);
        assert_eq!(json["prewarmed_dind_adoption_counts"]["none"], 1);
        assert_eq!(json["slowest_stage_ms"]["name"], "derived image");
        assert_eq!(json["slowest_stage_ms"]["label"], "jk-run-cold");
        assert_eq!(json["slowest_timing_ms"]["name"], "image/docker_build");
        assert_eq!(
            json["slowest_docker_build_step_ms"]["name"],
            "#46 exporting to image"
        );
        assert_eq!(json["runs"][0]["run_id"], "jk-run-cold");
        assert_eq!(json["runs"][0]["startup_ms"], 5_000);
        assert_eq!(json["runs"][0]["timeline_ms"], 6_000);
        assert_eq!(json["runs"][0]["startup_delta"], "+4.1s, 5.6x slower");
        assert_eq!(json["runs"][0]["startup_delta_ms"], 4_100);
        assert_eq!(json["runs"][0]["startup_saved_ms"], -4_100);
        assert_eq!(json["runs"][0]["startup_ratio"], 5_000.0 / 900.0);
        assert_eq!(json["runs"][0]["cache_misses"], 1);
        assert_eq!(json["runs"][0]["selected_plan"], "BuildAndCreate");
        assert_eq!(json["runs"][0]["selected_reason"], "missing_local_image");
        assert_eq!(
            json["runs"][0]["launch_plan_events"][0]["kind"],
            "launch_plan"
        );
        assert_eq!(json["runs"][0]["launch_plan_events"][0]["state"], "missing");
        assert_eq!(
            json["runs"][0]["build_context_snapshots"][0]["source"],
            "workspace"
        );
        assert_eq!(json["runs"][0]["build_context_snapshots"][0]["files"], 7);
        assert_eq!(
            json["runs"][0]["image_build_sources"][0]["source"],
            "workspace_dockerfile"
        );
        assert_eq!(
            json["runs"][0]["image_build_sources"][0]["pull_base_image"],
            false
        );
        assert_eq!(
            json["runs"][0]["build_context_snapshots"][0]["context_dir"],
            "/tmp/context"
        );
        assert_eq!(json["runs"][0]["max_build_context_bytes"], 2048);
        assert_eq!(
            json["runs"][0]["stage_durations_ms"]["derived image"][0],
            2_000
        );
        assert_eq!(
            json["runs"][0]["timing_durations_ms"]["image/docker_build"][0],
            1_500
        );
        assert_eq!(json["runs"][0]["slowest_stage_ms"]["name"], "derived image");
        assert_eq!(json["runs"][0]["slowest_timing_ms"]["duration_ms"], 1_500);
        assert_eq!(
            json["runs"][0]["slowest_docker_build_step_ms"]["name"],
            "#46 exporting to image"
        );
        assert_eq!(
            json["runs"][0]["slowest_docker_build_step_ms"]["duration_ms"],
            76_500
        );
        assert_eq!(json["runs"][0]["docker_build_steps"][0]["step"], "#46");
        assert_eq!(
            json["runs"][0]["docker_build_steps"][0]["name"],
            "#46 exporting to image"
        );
        assert_eq!(
            json["runs"][0]["docker_build_steps"][0]["duration_ms"],
            76_500
        );
        assert_eq!(
            json["runs"][0]["cache_decision"]["detail"],
            "missing_local_image"
        );
        assert_eq!(
            json["runs"][0]["cache_decisions"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            json["runs"][0]["cache_decisions"][1]["detail"],
            "recipe_hash_match"
        );
        assert_eq!(
            json["runs"][0]["skipped_timings"][0]["name"],
            "manifest_env"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["outcome"],
            "adopted"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["detail"],
            "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["source"],
            "state"
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["ready_ms"],
            7
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["prewarm_ready_ms"],
            34
        );
        assert_eq!(
            json["runs"][1]["prewarmed_dind_adoptions"][0]["state_age_ms"],
            12
        );
    }

    #[test]
    fn compare_output_requires_json_format() {
        let args = DiagnosticsCompareArgs {
            runs: vec!["cold".to_owned(), "warm".to_owned()],
            top: 10,
            baseline: DiagnosticsCompareBaseline::Fastest,
            format: DiagnosticsCompareFormat::Text,
            output: Some(PathBuf::from("compare.json")),
            labels: Vec::new(),
        };

        let error = validate_compare_args(&args).unwrap_err();

        assert_eq!(error.to_string(), "--output requires --format json");
    }

    #[test]
    fn compare_labels_must_match_run_count() {
        let args = DiagnosticsCompareArgs {
            runs: vec!["cold".to_owned(), "warm".to_owned()],
            top: 10,
            baseline: DiagnosticsCompareBaseline::Fastest,
            format: DiagnosticsCompareFormat::Json,
            output: None,
            labels: vec!["cold".to_owned()],
        };

        let error = validate_compare_args(&args).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--label must be supplied once per run when used"
        );
    }

    #[test]
    fn comparison_json_uses_explicit_labels() {
        let mut cold = summary_with_startup(5_000);
        cold.run_id = Some("jk-run-cold".to_owned());
        let runs = vec![
            (PathBuf::from("a.jsonl"), cold),
            (PathBuf::from("b.jsonl"), summary_with_startup(900)),
        ];
        let labels = vec!["cold-before".to_owned(), "warm-after".to_owned()];

        let json = comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &labels);

        assert_eq!(json["runs"][0]["label"], "cold-before");
        assert_eq!(json["runs"][1]["label"], "warm-after");
        assert_eq!(json["fastest_startup_run"]["label"], "warm-after");
        assert_eq!(json["slowest_startup_run"]["label"], "cold-before");
    }

    #[test]
    fn compare_output_writes_json_artifact_with_trailing_newline() {
        let runs = vec![
            (PathBuf::from("cold.jsonl"), summary_with_startup(5_000)),
            (PathBuf::from("warm.jsonl"), summary_with_startup(900)),
        ];
        let output =
            render_comparison_json(&runs, DiagnosticsCompareBaseline::Fastest, &[]).unwrap();
        let path = std::env::temp_dir().join(format!(
            "jackin-diagnostics-compare-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));

        write_compare_output(&path, &output).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        drop(fs::remove_file(&path));

        assert!(written.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&written).unwrap()["baseline"],
            "fastest"
        );
    }

    #[test]
    fn byte_formatter_uses_binary_units() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn comparison_names_are_ranked_by_slowest_observed_duration() {
        let runs = vec![
            (
                PathBuf::from("first.jsonl"),
                summary_with_stages([("credentials", 200), ("role", 10)]),
            ),
            (
                PathBuf::from("second.jsonl"),
                summary_with_stages([("derived image", 500), ("credentials", 40)]),
            ),
        ];

        let names = comparison_names(&runs, 2, |summary| &summary.stage_durations_ms);

        assert_eq!(names, vec!["derived image", "credentials"]);
    }

    #[test]
    fn comparison_names_are_truncated_to_display_width() {
        assert_eq!(truncate_name("short", 10), "short");
        assert_eq!(truncate_name("abcdefghijklmnopqrstuvwxyz", 8), "abcdefg…");
    }

    #[test]
    fn build_context_comparison_uses_max_snapshot_per_run() {
        let mut summary = summary_with_stages([]);
        summary
            .build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("workspace".to_owned()),
                files: 2,
                bytes: 1024,
                context_dir: Some("/tmp/one".to_owned()),
            });
        summary
            .build_context_snapshots
            .push(jackin_diagnostics::BuildContextSnapshotSummary {
                source: Some("published".to_owned()),
                files: 5,
                bytes: 512,
                context_dir: Some("/tmp/two".to_owned()),
            });

        assert_eq!(max_build_context_bytes(&summary), Some(1024));
        assert_eq!(max_build_context_files(&summary), Some(5));
    }

    #[test]
    fn cache_comparison_uses_first_cache_decision_per_run() {
        let mut summary = summary_with_stages([]);
        summary
            .cache_events
            .push(jackin_diagnostics::CacheEventSummary {
                kind: "image_cache_miss".to_owned(),
                stage: Some("derived image".to_owned()),
                message: "rebuild".to_owned(),
                detail: Some("hooks_hash_changed".to_owned()),
            });

        assert_eq!(summary.cache_events[0].kind, "image_cache_miss");
        assert_eq!(
            summary.cache_events[0].detail.as_deref(),
            Some("hooks_hash_changed")
        );
    }

    #[test]
    fn skipped_timing_comparison_lists_union_across_runs() {
        let mut first = summary_with_stages([]);
        first
            .skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "operator_env".to_owned(),
                detail: "skipped".to_owned(),
            });
        let mut second = summary_with_stages([]);
        second
            .skipped_timings
            .push(jackin_diagnostics::SkippedTimingSummary {
                stage: "credentials".to_owned(),
                name: "manifest_env".to_owned(),
                detail: "skipped".to_owned(),
            });
        let runs = vec![
            (PathBuf::from("warm.jsonl"), first.clone()),
            (PathBuf::from("attach.jsonl"), second),
        ];

        assert_eq!(
            skipped_timing_names(&runs, 10),
            vec![
                "credentials/manifest_env".to_owned(),
                "credentials/operator_env".to_owned()
            ]
        );
        assert_eq!(
            skipped_timing_detail(&first, "credentials/operator_env"),
            Some("skipped")
        );
        assert_eq!(
            skipped_timing_detail(&first, "credentials/manifest_env"),
            None
        );
    }

    #[test]
    fn launch_plan_comparison_uses_selected_plan() {
        let mut summary = summary_with_stages([]);
        summary
            .launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan_rejected".to_owned(),
                plan: Some("AttachExisting".to_owned()),
                reason: Some("container_missing".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });
        summary
            .launch_plan_events
            .push(jackin_diagnostics::LaunchPlanEventSummary {
                kind: "launch_plan".to_owned(),
                plan: Some("CreateFromValidImage".to_owned()),
                reason: Some("recipe_hash_match".to_owned()),
                container: Some("jk-demo".to_owned()),
                state: Some("missing".to_owned()),
            });

        let selected = selected_launch_plan(&summary).unwrap();

        assert_eq!(selected.plan.as_deref(), Some("CreateFromValidImage"));
        assert_eq!(selected.reason.as_deref(), Some("recipe_hash_match"));
    }

    #[test]
    fn prewarmed_dind_comparison_uses_latest_adoption() {
        let mut summary = summary_with_stages([]);
        summary
            .prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "skipped".to_owned(),
                detail: Some("locked".to_owned()),
                reason: Some("locked".to_owned()),
                source: None,
                ready_ms: None,
                prewarm_ready_ms: None,
                state_age_ms: None,
            });
        summary
            .prewarmed_dind_adoptions
            .push(jackin_diagnostics::PrewarmedDindAdoptionSummary {
                outcome: "adopted".to_owned(),
                detail: Some(
                    "ready_ms=7;source=state;state_age_ms=12;prewarm_ready_ms=34".to_owned(),
                ),
                reason: None,
                source: Some("state".to_owned()),
                ready_ms: Some(7),
                prewarm_ready_ms: Some(34),
                state_age_ms: Some(12),
            });

        let latest = last_prewarmed_dind_adoption(&summary).unwrap();

        assert_eq!(latest.outcome, "adopted");
        assert_eq!(
            super::format_prewarmed_dind_adoption_detail(latest),
            "source=state ready_ms=7 prewarm_ready_ms=34 state_age_ms=12"
        );
    }

    #[test]
    fn docker_build_step_comparison_uses_slowest_step_per_run() {
        let mut first = summary_with_stages([]);
        first
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#12".to_owned(),
                label: "RUN claude install".to_owned(),
                duration_ms: Some(1_200),
                cached: false,
            });
        first
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#12".to_owned(),
                label: "RUN claude install".to_owned(),
                duration_ms: Some(800),
                cached: true,
            });
        let mut second = summary_with_stages([]);
        second
            .docker_build_steps
            .push(jackin_diagnostics::DockerBuildStepSummary {
                step: "#46".to_owned(),
                label: "exporting to image".to_owned(),
                duration_ms: Some(76_500),
                cached: false,
            });
        let runs = vec![
            (PathBuf::from("first.jsonl"), first.clone()),
            (PathBuf::from("second.jsonl"), second),
        ];

        assert_eq!(
            docker_build_step_names(&runs, 2),
            vec![
                "#46 exporting to image".to_owned(),
                "#12 RUN claude install".to_owned()
            ]
        );
        assert_eq!(
            max_docker_build_step_duration(&first, "#12 RUN claude install"),
            Some(1_200)
        );
    }

    fn summary_with_stages<const N: usize>(
        stages: [(&str, u64); N],
    ) -> jackin_diagnostics::DiagnosticsSummary {
        let mut stage_durations_ms = BTreeMap::new();
        for (name, duration) in stages {
            stage_durations_ms.insert(name.to_owned(), vec![duration]);
        }
        jackin_diagnostics::DiagnosticsSummary {
            run_id: None,
            event_count: 0,
            event_counts: BTreeMap::new(),
            first_ts_ms: None,
            last_ts_ms: None,
            hardline_ts_ms: None,
            stage_durations_ms,
            timing_durations_ms: BTreeMap::new(),
            build_context_snapshots: Vec::new(),
            image_build_sources: Vec::new(),
            docker_build_steps: Vec::new(),
            cache_events: Vec::new(),
            launch_plan_events: Vec::new(),
            prewarmed_dind_adoptions: Vec::new(),
            skipped_timings: Vec::new(),
        }
    }

    fn summary_with_startup(startup_ms: u128) -> jackin_diagnostics::DiagnosticsSummary {
        let mut summary = summary_with_stages([]);
        summary.first_ts_ms = Some(100);
        summary.hardline_ts_ms = Some(100 + startup_ms);
        summary.last_ts_ms = Some(100 + startup_ms + 1_000);
        summary
    }
}
