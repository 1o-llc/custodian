use custodian_kit::{
    code_context_json, code_graph_json, code_nav, code_probe, code_query, code_query_json, code_search_plan_json, edit_plan_json, klepto_field_plan_json, code_search_index_json, code_search_query_json, code_search_disk_query_json, code_search_disk_status_json, code_search_disk_manifest_json, code_search_disk_compare_json, code_search_disk_refresh_plan_json, klepto_index_key_json, klepto_index_version_plan_json, klepto_index_registry_entry_json, klepto_index_registry_write_json, klepto_index_registry_resolve_json, klepto_index_registry_refresh_plan_json, klepto_index_registry_refresh_exec_plan_json, klepto_index_registry_refresh_exec_json, klepto_index_registry_refresh_stage_json, klepto_index_registry_refresh_stage_verify_json, klepto_index_registry_refresh_swap_plan_json, klepto_index_registry_refresh_swap_exec_json, klepto_index_registry_refresh_apply_plan_json, klepto_index_registry_refresh_apply_exec_json, klepto_index_registry_refresh_live_apply_request_json, klepto_index_registry_refresh_live_apply_preflight_json, klepto_index_registry_refresh_live_apply_commit_plan_json, klepto_index_registry_refresh_live_apply_commit_exec_json, klepto_index_registry_refresh_live_apply_commit_stage_json, klepto_index_registry_refresh_live_apply_commit_stage_verify_json, klepto_index_registry_refresh_live_apply_commit_gate_json,
    feature_map, feature_probe, feature_sites, pin_query, pin_query_json,
};
use std::env;

mod cli;

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_help();
        return;
    }
    if cli::is_command(&args[0]) {
        let word = args.remove(0);
        cli::dispatch(&word, &args);
        return;
    }

    match args.remove(0).as_str() {
        "help" | "--help" | "-h" => print_help(),
        "code-graph-json" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let max_depth = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("code-graph-json", code_graph_json(root, max_depth));
        }
        "code-nav" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let max_depth = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("code-nav", code_nav(root, max_depth));
        }
        "code-probe" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let query = args.get(1).map(String::as_str).unwrap_or("summary");
            let max_depth = args.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("code-probe", code_probe(root, query, max_depth));
        }
        "code-query-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-query-json", code_query_json(root, max_depth, &query_args));
        }
        "code-query" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-query", code_query(root, max_depth, &query_args));
        }
        "pin-query-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("pin-query-json", pin_query_json(root, max_depth, &query_args));
        }
        "pin-query" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("pin-query", pin_query(root, max_depth, &query_args));
        }
        "code-context-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-context-json", code_context_json(root, max_depth, &query_args));
        }
        "code-search-plan-json" | "search-plan-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-search-plan-json", code_search_plan_json(root, max_depth, &query_args));
        }
        "edit-plan-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("edit-plan-json", edit_plan_json(root, max_depth, &query_args));
        }
        "klepto-field-plan-json" | "code-field-plan-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("klepto-field-plan-json", klepto_field_plan_json(root, max_depth, &query_args));
        }
        "klepto-index-key-json" | "code-search-klepto-key-json" | "code-index-key-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("klepto-index-key-json", klepto_index_key_json(root, max_depth, &query_args));
        }
        "klepto-index-version-plan-json" | "code-search-klepto-version-json" | "code-index-version-plan-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("klepto-index-version-plan-json", klepto_index_version_plan_json(root, max_depth, &query_args));
        }
        "klepto-index-registry-entry-json" | "code-search-klepto-registry-entry-json" | "code-index-registry-entry-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("klepto-index-registry-entry-json", klepto_index_registry_entry_json(root, max_depth, &query_args));
        }
        "klepto-index-registry-write-json" | "code-search-klepto-registry-write-json" | "code-index-registry-write-json" => {
            let registry_path = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let root = args.get(1).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 2usize;
            if let Some(value) = args.get(2) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 3;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-write-json", klepto_index_registry_write_json(registry_path, root, max_depth, &query_args));
        }
        "klepto-index-registry-resolve-json" | "code-search-klepto-registry-resolve-json" | "code-index-registry-resolve-json" => {
            let registry_path = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let root = args.get(1).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 2usize;
            if let Some(value) = args.get(2) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 3;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-resolve-json", klepto_index_registry_resolve_json(registry_path, root, max_depth, &query_args));
        }
        "klepto-index-registry-refresh-plan-json" | "code-search-klepto-registry-refresh-plan-json" | "code-index-registry-refresh-plan-json" => {
            let registry_path = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let index_dir = args.get(1).map(String::as_str).unwrap_or(".");
            let root = args.get(2).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 3usize;
            if let Some(value) = args.get(3) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 4;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-plan-json", klepto_index_registry_refresh_plan_json(registry_path, index_dir, root, max_depth, &query_args));
        }
        "klepto-index-registry-refresh-exec-plan-json" | "code-search-klepto-registry-refresh-exec-plan-json" | "code-index-registry-refresh-exec-plan-json" => {
            let registry_path = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let index_dir = args.get(1).map(String::as_str).unwrap_or(".");
            let root = args.get(2).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 3usize;
            if let Some(value) = args.get(3) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 4;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-exec-plan-json", klepto_index_registry_refresh_exec_plan_json(registry_path, index_dir, root, max_depth, &query_args));
        }
        "klepto-index-registry-refresh-exec-json" | "klepto-index-registry-refresh-execute-json" | "code-search-klepto-registry-refresh-exec-json" | "code-index-registry-refresh-exec-json" => {
            let registry_path = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let index_dir = args.get(1).map(String::as_str).unwrap_or(".");
            let root = args.get(2).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 3usize;
            if let Some(value) = args.get(3) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 4;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-exec-json", klepto_index_registry_refresh_exec_json(registry_path, index_dir, root, max_depth, &query_args));
        }
        "klepto-index-registry-refresh-stage-json" | "klepto-index-registry-refresh-staging-json" | "code-search-klepto-registry-refresh-stage-json" | "code-index-registry-refresh-stage-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let registry_path = args.get(1).map(String::as_str).unwrap_or("./code_passenger_klepto_index_registry.json");
            let index_dir = args.get(2).map(String::as_str).unwrap_or(".");
            let root = args.get(3).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 4usize;
            if let Some(value) = args.get(4) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 5;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-stage-json", klepto_index_registry_refresh_stage_json(staging_dir, registry_path, index_dir, root, max_depth, &query_args));
        }
        "klepto-index-registry-refresh-stage-verify-json" | "klepto-index-registry-refresh-verify-stage-json" | "code-search-klepto-registry-refresh-stage-verify-json" | "code-index-registry-refresh-stage-verify-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let verify_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-stage-verify-json", klepto_index_registry_refresh_stage_verify_json(staging_dir, &verify_args));
        }
        "klepto-index-registry-refresh-swap-plan-json" | "klepto-index-registry-refresh-plan-swap-json" | "code-search-klepto-registry-refresh-swap-plan-json" | "code-index-registry-refresh-swap-plan-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let swap_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-swap-plan-json", klepto_index_registry_refresh_swap_plan_json(staging_dir, &swap_args));
        }
        "klepto-index-registry-refresh-swap-exec-json" | "klepto-index-registry-refresh-execute-swap-json" | "code-search-klepto-registry-refresh-swap-exec-json" | "code-index-registry-refresh-swap-exec-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let swap_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-swap-exec-json", klepto_index_registry_refresh_swap_exec_json(staging_dir, &swap_args));
        }
        "klepto-index-registry-refresh-apply-plan-json" | "klepto-index-registry-refresh-plan-apply-json" | "code-search-klepto-registry-refresh-apply-plan-json" | "code-index-registry-refresh-apply-plan-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let apply_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-apply-plan-json", klepto_index_registry_refresh_apply_plan_json(staging_dir, &apply_args));
        }
        "klepto-index-registry-refresh-apply-exec-json" | "klepto-index-registry-refresh-execute-apply-json" | "code-search-klepto-registry-refresh-apply-exec-json" | "code-index-registry-refresh-apply-exec-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let apply_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-apply-exec-json", klepto_index_registry_refresh_apply_exec_json(staging_dir, &apply_args));
        }
        "klepto-index-registry-refresh-live-apply-request-json" | "klepto-index-registry-refresh-request-live-apply-json" | "code-search-klepto-registry-refresh-live-apply-request-json" | "code-index-registry-refresh-live-apply-request-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let request_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-request-json", klepto_index_registry_refresh_live_apply_request_json(staging_dir, &request_args));
        }
        "klepto-index-registry-refresh-live-apply-preflight-json" | "klepto-index-registry-refresh-preflight-live-apply-json" | "code-search-klepto-registry-refresh-live-apply-preflight-json" | "code-index-registry-refresh-live-apply-preflight-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let preflight_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-preflight-json", klepto_index_registry_refresh_live_apply_preflight_json(staging_dir, &preflight_args));
        }
        "klepto-index-registry-refresh-live-apply-commit-plan-json" | "klepto-index-registry-refresh-plan-live-apply-commit-json" | "code-search-klepto-registry-refresh-live-apply-commit-plan-json" | "code-index-registry-refresh-live-apply-commit-plan-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let commit_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-commit-plan-json", klepto_index_registry_refresh_live_apply_commit_plan_json(staging_dir, &commit_args));
        }
        "klepto-index-registry-refresh-live-apply-commit-exec-json" | "klepto-index-registry-refresh-execute-live-apply-commit-json" | "code-search-klepto-registry-refresh-live-apply-commit-exec-json" | "code-index-registry-refresh-live-apply-commit-exec-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let commit_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-commit-exec-json", klepto_index_registry_refresh_live_apply_commit_exec_json(staging_dir, &commit_args));
        }
        "klepto-index-registry-refresh-live-apply-commit-stage-json" | "klepto-index-registry-refresh-stage-live-apply-commit-json" | "code-search-klepto-registry-refresh-live-apply-commit-stage-json" | "code-index-registry-refresh-live-apply-commit-stage-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let stage_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-commit-stage-json", klepto_index_registry_refresh_live_apply_commit_stage_json(staging_dir, &stage_args));
        }
        "klepto-index-registry-refresh-live-apply-commit-stage-verify-json" | "klepto-index-registry-refresh-verify-live-apply-commit-stage-json" | "code-search-klepto-registry-refresh-live-apply-commit-stage-verify-json" | "code-index-registry-refresh-live-apply-commit-stage-verify-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let verify_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-commit-stage-verify-json", klepto_index_registry_refresh_live_apply_commit_stage_verify_json(staging_dir, &verify_args));
        }
        "klepto-index-registry-refresh-live-apply-commit-gate-json" | "klepto-index-registry-refresh-gate-live-apply-commit-json" | "code-search-klepto-registry-refresh-live-apply-commit-gate-json" | "code-index-registry-refresh-live-apply-commit-gate-json" => {
            let staging_dir = args.get(0).map(String::as_str).unwrap_or("./code_passenger_klepto_refresh_stage");
            let gate_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("klepto-index-registry-refresh-live-apply-commit-gate-json", klepto_index_registry_refresh_live_apply_commit_gate_json(staging_dir, &gate_args));
        }
        "code-search-index-json" | "tantivy-index-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-search-index-json", code_search_index_json(root, max_depth, &query_args));
        }
        "code-search-query-json" | "tantivy-query-json" => {
            let (root, max_depth, query_args) = split_root_depth_query_args(&args);
            print_result("code-search-query-json", code_search_query_json(root, max_depth, &query_args));
        }
        "code-search-disk-query-json" | "tantivy-disk-query-json" | "code-search-open-query-json" => {
            let index_dir = args.get(0).map(String::as_str).unwrap_or(".");
            let query_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("code-search-disk-query-json", code_search_disk_query_json(index_dir, &query_args));
        }
        "code-search-disk-status-json" | "tantivy-disk-status-json" | "code-search-index-status-json" => {
            let index_dir = args.get(0).map(String::as_str).unwrap_or(".");
            let query_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("code-search-disk-status-json", code_search_disk_status_json(index_dir, &query_args));
        }
        "code-search-disk-manifest-json" | "tantivy-disk-manifest-json" | "code-search-index-manifest-json" => {
            let index_dir = args.get(0).map(String::as_str).unwrap_or(".");
            let query_args = args.iter().skip(1).cloned().collect::<Vec<_>>();
            print_result("code-search-disk-manifest-json", code_search_disk_manifest_json(index_dir, &query_args));
        }
        "code-search-disk-compare-json" | "tantivy-disk-compare-json" | "code-search-index-compare-json" => {
            let left_index_dir = args.get(0).map(String::as_str).unwrap_or(".");
            let right_index_dir = args.get(1).map(String::as_str).unwrap_or(".");
            let query_args = args.iter().skip(2).cloned().collect::<Vec<_>>();
            print_result("code-search-disk-compare-json", code_search_disk_compare_json(left_index_dir, right_index_dir, &query_args));
        }
        "code-search-disk-refresh-plan-json" | "tantivy-disk-refresh-plan-json" | "code-search-index-refresh-plan-json" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let index_dir = args.get(1).map(String::as_str).unwrap_or(".");
            let mut max_depth = 8usize;
            let mut query_start = 2usize;
            if let Some(value) = args.get(2) {
                if let Ok(parsed) = value.parse::<usize>() {
                    max_depth = parsed;
                    query_start = 3;
                }
            }
            let query_args = args.iter().skip(query_start).cloned().collect::<Vec<_>>();
            print_result("code-search-disk-refresh-plan-json", code_search_disk_refresh_plan_json(root, index_dir, max_depth, &query_args));
        }
        "feature-map" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let max_depth = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("feature-map", feature_map(root, max_depth));
        }
        "feature-sites" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let target = args.get(1).map(String::as_str).unwrap_or("");
            let max_depth = args.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("feature-sites", feature_sites(root, target, max_depth));
        }
        "feature-probe" => {
            let root = args.get(0).map(String::as_str).unwrap_or(".");
            let target = args.get(1).map(String::as_str).unwrap_or("");
            let max_depth = args.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(8);
            print_result("feature-probe", feature_probe(root, target, max_depth));
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn split_root_depth_query_args(args: &[String]) -> (&str, usize, Vec<String>) {
    let root = args.get(0).map(String::as_str).unwrap_or(".");
    let mut max_depth = 8usize;
    let mut query_args = Vec::<String>::new();
    if args.len() > 1 {
        if let Ok(parsed) = args[1].parse::<usize>() {
            max_depth = parsed;
            query_args.extend(args.iter().skip(2).cloned());
        } else {
            query_args.extend(args.iter().skip(1).cloned());
        }
    }
    (root, max_depth, query_args)
}

fn print_result(command: &str, result: custodian_kit::Result<String>) {
    match result {
        Ok(text) => println!("{text}"),
        Err(err) => {
            eprintln!("{command} failed: {err}");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("CODEPASSENGER :: PHASE 42 KLEPTO REGISTRY REFRESH LIVE APPLY COMMIT GATE");
    println!("──────────────────────────────");
    println!("commands:");
    println!("  help");
    for (_, line) in cli::COMMANDS {
        println!("  {line}");
    }
    println!("  code-graph-json [ROOT] [MAX_DEPTH]");
    println!("  code-nav [ROOT] [MAX_DEPTH]");
    println!("  code-probe [ROOT] [QUERY] [MAX_DEPTH]");
    println!("  feature-map [ROOT] [MAX_DEPTH]");
    println!("  feature-sites [ROOT] [DEP_OR_FEATURE] [MAX_DEPTH]");
    println!("  feature-probe [ROOT] [DEP_OR_FEATURE] [MAX_DEPTH]");
    println!("  code-query-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-query [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  pin-query-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  pin-query [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-context-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-search-plan-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  edit-plan-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-field-plan-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-search-index-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-search-query-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  code-search-disk-query-json [INDEX_DIR] [QUERY_FLAGS...]");
    println!("  code-search-disk-status-json [INDEX_DIR] [--sample N]");
    println!("  code-search-disk-manifest-json [INDEX_DIR] [--sample N] [--max-files N]");
    println!("  code-search-disk-compare-json [LEFT_INDEX_DIR] [RIGHT_INDEX_DIR] [--sample N] [--max-files N]");
    println!("  code-search-disk-refresh-plan-json [ROOT] [INDEX_DIR] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-key-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-version-plan-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-entry-json [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-write-json [REGISTRY_PATH] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-resolve-json [REGISTRY_PATH] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-refresh-plan-json [REGISTRY_PATH] [INDEX_DIR] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-refresh-exec-plan-json [REGISTRY_PATH] [INDEX_DIR] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-refresh-exec-json [REGISTRY_PATH] [INDEX_DIR] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-refresh-stage-json [STAGING_DIR] [REGISTRY_PATH] [INDEX_DIR] [ROOT] [MAX_DEPTH] [QUERY_FLAGS...]");
    println!("  klepto-index-registry-refresh-stage-verify-json [STAGING_DIR] [--expected-execution-token TOKEN]");
    println!("  klepto-index-registry-refresh-swap-plan-json [STAGING_DIR] [--expected-execution-token TOKEN]");
    println!("  klepto-index-registry-refresh-swap-exec-json [STAGING_DIR] [--execute-swap] [--ack-execute-swap] [--expected-execution-token TOKEN] [--expected-swap-plan-id ID]");
    println!("  klepto-index-registry-refresh-apply-plan-json [STAGING_DIR] [--execute-swap] [--ack-execute-swap] [--expected-execution-token TOKEN] [--expected-swap-plan-id ID]");
    println!("  klepto-index-registry-refresh-apply-exec-json [STAGING_DIR] [--execute-apply] [--ack-execute-apply] [--expected-apply-plan-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-request-json [STAGING_DIR] [--request-live-apply] [--ack-request-live-apply]");
    println!("  klepto-index-registry-refresh-live-apply-preflight-json [STAGING_DIR] [--preflight-live-apply] [--ack-preflight-live-apply] [--expected-live-apply-request-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-commit-plan-json [STAGING_DIR] [--plan-live-commit] [--ack-plan-live-commit] [--expected-live-apply-request-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-commit-exec-json [STAGING_DIR] [--execute-live-commit] [--ack-execute-live-commit] [--expected-live-commit-plan-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-commit-stage-json [STAGING_DIR] [--stage-live-commit] [--ack-stage-live-commit] [--expected-live-commit-execution-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-commit-stage-verify-json [STAGING_DIR] [--expected-live-commit-execution-id ID]");
    println!("  klepto-index-registry-refresh-live-apply-commit-gate-json [STAGING_DIR] [--open-live-commit-gate] [--ack-open-live-commit-gate] [--expected-live-commit-execution-id ID]");
    println!();
    println!("phase 42 goal:");
    println!("  keep code_passenger.graph.v1 raw JSON available");
    println!("  render the graph as stable code navigation regions");
    println!("  probe one crate/file/feature/dep/function/import/site/finding");
    println!("  map Cargo features, optional deps, forwarded dep features, and use-sites");
    println!("  list exact use-sites for one feature/dependency target");
    println!("  synthesize feature/dependency target into a read-only review card");
    println!("  expose buildable machine-readable code queries");
    println!("  render buildable code queries as visual selectable regions");
    println!("  convert query results into stable pin-set receipts");
    println!("  emit bounded source context windows around pinned regions");
    println!("  build Tantivy-ready multi-tenant search plan documents from context windows");
    println!("  emit dry-run edit plans anchored to pins and context windows");
    println!("  stop before guarded writes; edit plan emits JSON only");
    println!("  assemble Klepto-style customizable field plans for future search/indexing");
    println!("  use real Tantivy for RAM/disk index receipts");
    println!("  execute real Tantivy queries over RAM indexes and emit hydrated hit receipts");
    println!("  no repo writes or shell execution; query mode never writes disk indexes");
    println!("  open existing disk indexes read-only and query stored hydrated docs");
    println!("  audit persistent disk index schema, tenant scopes, stored fields, and samples without mutation");
    println!("  emit persistent disk index manifest/hash receipts for comparison before refresh/merge work");
    println!("  compare two persistent disk index manifests without mutation before refresh/merge work");
    println!("  plan persistent disk index refreshes by comparing an existing manifest to a fresh RAM candidate");
    println!("  bind Klepto source schemas to polymorphic index keys");
    println!("  plan index version reuse/patch/minor/major decisions from Klepto key hashes");
    println!("  turn Klepto version decisions into read-only registry-entry receipts");
    println!("  persist Klepto registry entries only behind explicit write+ack flags");
    println!("  resolve current Klepto keys against persisted registry entries without mutation");
    println!("  combine persisted registry resolution with disk refresh planning without mutation");
    println!("  turn registry-aware refresh plans into read-only execution tickets");
    println!("  consume execution tickets into dry-run execution receipts without mutation");
    println!("  write isolated staging manifests only after explicit stage+ack flags");
    println!("  verify staged manifests, execution receipts, tokens, and staged-file counts before future swaps");
    println!("  plan verified staged artifacts for future swaps without mutation");
    println!("  consume swap plans into dry-run swap execution receipts without mutation
  plan future live applies from permitted swap execution receipts without mutation
  consume apply plans into dry-run apply execution receipts without mutation
  record acknowledged live-apply intent for future guarded writers without mutation
  preflight acknowledged live-apply requests before future guarded writers without mutation
  plan future live commit locks, backups, and journals without mutation
  consume live commit plans into dry-run live commit execution receipts without mutation");
    println!("  stage future live commit journals under isolated staging only");
    println!("  verify staged live commit journals before future guarded live writers without mutation");
    println!("  open a final live commit gate for future guarded writers without mutation");
}
