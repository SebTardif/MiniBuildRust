mod cache;
mod cli;
mod executor;
mod graph;
mod parser;

use std::fs;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match cli::parse_args(&args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}", msg);
            process::exit(1);
        }
    };

    // Clean mode
    if cli.clean {
        cache::BuildCache::clean(Path::new("."));
        eprintln!("Cache cleaned.");
        if cli.target.is_none() {
            return;
        }
    }

    // Read build file
    let content = match fs::read_to_string(&cli.file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: cannot read '{}': {}", cli.file, e);
            process::exit(1);
        }
    };

    // Parse
    let bf = match parser::parse(&content) {
        Ok(bf) => bf,
        Err(e) => {
            eprintln!("parse error: {}", e);
            process::exit(1);
        }
    };

    // Determine target
    let target = cli
        .target
        .clone()
        .or_else(|| bf.default_target.clone())
        .unwrap_or_else(|| {
            eprintln!("error: no target specified and no default target in build file");
            process::exit(1);
        });

    // Build graph
    let g = match graph::build_graph(&bf) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("graph error: {}", e);
            process::exit(1);
        }
    };

    // Find reachable rules
    let reachable = match graph::reachable_from(&target, &g) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    // Topological sort
    let order = graph::topological_sort(&g, &reachable);

    if cli.verbose {
        eprintln!("Execution order: {:?}", order);
        eprintln!("Parallelism: {} jobs", cli.jobs);
    }

    // Load cache
    let cache = Arc::new(Mutex::new(cache::BuildCache::load(Path::new("."))));

    // Execute
    let opts = executor::ExecOptions {
        jobs: cli.jobs,
        dry_run: cli.dry_run,
        verbose: cli.verbose,
    };

    let results = executor::execute(&bf, &g, &order, &cache, &opts);

    // Save cache
    if !cli.dry_run {
        if let Err(e) = cache.lock().unwrap().save(Path::new(".")) {
            eprintln!("warning: failed to save cache: {}", e);
        }
    }

    // Exit with failure if any rule failed
    let has_failure = results
        .iter()
        .any(|r| matches!(r, executor::RuleResult::Failed(_, _)));
    if has_failure {
        process::exit(1);
    }
}

#[cfg(test)]
mod integration_tests {
    use std::collections::HashSet;
    use std::fs;
    use std::sync::{Arc, Mutex};

    use crate::cache::BuildCache;
    use crate::executor::{self, ExecOptions, RuleResult};
    use crate::graph;
    use crate::parser;

    /// Helper: parse, build graph, sort, execute — return results.
    fn full_build(
        input: &str,
        target: &str,
        jobs: usize,
        cache: &Arc<Mutex<BuildCache>>,
    ) -> Vec<RuleResult> {
        let bf = parser::parse(input).unwrap();
        let g = graph::build_graph(&bf).unwrap();
        let reachable = graph::reachable_from(target, &g).unwrap();
        let order = graph::topological_sort(&g, &reachable);
        let opts = ExecOptions {
            jobs,
            dry_run: false,
            verbose: false,
        };
        executor::execute(&bf, &g, &order, cache, &opts)
    }

    /// Diamond dependency: A→B, A→C, B→D, C→D
    #[test]
    fn test_diamond_dependency() {
        let input = "\
rule a
  deps b c
  run echo a

rule b
  deps d
  run echo b

rule c
  deps d
  run echo c

rule d
  run echo d
";
        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let results = full_build(input, "a", 4, &cache);

        let successes: Vec<&str> = results
            .iter()
            .filter_map(|r| match r {
                RuleResult::Success(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(successes.len(), 4);
        // d must come before b and c; b and c before a
        let pos = |name: &str| successes.iter().position(|&n| n == name).unwrap();
        assert!(pos("d") < pos("b"));
        assert!(pos("d") < pos("c"));
        assert!(pos("b") < pos("a"));
        assert!(pos("c") < pos("a"));
    }

    /// Large graph: 100+ rules in a chain.
    #[test]
    fn test_large_chain_graph() {
        let mut input = String::new();
        let n = 120;
        for i in 0..n {
            if i == 0 {
                input.push_str(&format!("rule r{}\n  run echo r{}\n", i, i));
            } else {
                input.push_str(&format!(
                    "rule r{}\n  deps r{}\n  run echo r{}\n",
                    i,
                    i - 1,
                    i
                ));
            }
        }
        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let results = full_build(&input, &format!("r{}", n - 1), 8, &cache);

        let success_count = results
            .iter()
            .filter(|r| matches!(r, RuleResult::Success(_)))
            .count();
        assert_eq!(success_count, n);
    }

    /// Large fan-out graph: 100+ independent rules under one parent.
    #[test]
    fn test_large_fanout_graph() {
        let n = 110;
        let mut input = String::new();

        let dep_list: Vec<String> = (0..n).map(|i| format!("leaf{}", i)).collect();
        input.push_str(&format!(
            "rule root\n  deps {}\n  phony true\n  run echo done\n",
            dep_list.join(" ")
        ));

        for i in 0..n {
            input.push_str(&format!("rule leaf{}\n  run echo leaf{}\n", i, i));
        }

        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let results = full_build(&input, "root", 16, &cache);

        let success_count = results
            .iter()
            .filter(|r| matches!(r, RuleResult::Success(_)))
            .count();
        assert_eq!(success_count, n + 1);
    }

    /// Incremental rebuild: second run should skip unchanged rules.
    #[test]
    fn test_incremental_rebuild() {
        let dir = std::env::temp_dir().join("minibuild_test_incremental");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let input_file = dir.join("input.txt");
        let output_file = dir.join("output.txt");
        fs::write(&input_file, "hello").unwrap();

        let input = format!(
            "rule build\n  inputs {inp}\n  outputs {out}\n  run cp {inp} {out}\n",
            inp = input_file.display(),
            out = output_file.display(),
        );

        let cache = Arc::new(Mutex::new(BuildCache::new()));

        // First build — should execute
        let results1 = full_build(&input, "build", 1, &cache);
        assert!(matches!(&results1[0], RuleResult::Success(_)));
        assert!(output_file.exists());

        // Second build — should skip (inputs unchanged)
        let results2 = full_build(&input, "build", 1, &cache);
        assert!(matches!(&results2[0], RuleResult::Skipped(_)));

        // Modify input — should rebuild
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&input_file, "changed").unwrap();
        // Invalidate cache since input changed
        cache.lock().unwrap().invalidate("build");

        let results3 = full_build(&input, "build", 1, &cache);
        assert!(matches!(&results3[0], RuleResult::Success(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Failure propagation: if a deep dependency fails, all downstream are skipped.
    #[test]
    fn test_failure_propagation_chain() {
        let input = "\
rule top
  deps mid
  run echo top

rule mid
  deps bottom
  run echo mid

rule bottom
  run exit 1
";
        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let results = full_build(input, "top", 1, &cache);

        // bottom fails, mid and top should be marked as failed (dependency failed)
        let failed_names: HashSet<&str> = results
            .iter()
            .filter_map(|r| match r {
                RuleResult::Failed(n, _) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        assert!(failed_names.contains("bottom"));
        assert!(failed_names.contains("mid"));
        assert!(failed_names.contains("top"));
    }

    /// Partial failure: independent branches should still succeed.
    #[test]
    fn test_partial_failure() {
        let input = "\
rule all
  deps good_branch bad_branch
  phony true
  run echo all

rule good_branch
  run echo success

rule bad_branch
  run exit 1
";
        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let results = full_build(input, "all", 4, &cache);

        let succeeded: HashSet<&str> = results
            .iter()
            .filter_map(|r| match r {
                RuleResult::Success(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        let failed: HashSet<&str> = results
            .iter()
            .filter_map(|r| match r {
                RuleResult::Failed(n, _) => Some(n.as_str()),
                _ => None,
            })
            .collect();

        assert!(succeeded.contains("good_branch"));
        assert!(failed.contains("bad_branch"));
        assert!(failed.contains("all"));
    }

    /// Dry run should not execute anything.
    #[test]
    fn test_dry_run() {
        let input = "rule test\n  run echo should not print\n";
        let bf = parser::parse(input).unwrap();
        let g = graph::build_graph(&bf).unwrap();
        let reachable = graph::reachable_from("test", &g).unwrap();
        let order = graph::topological_sort(&g, &reachable);
        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let opts = ExecOptions {
            jobs: 1,
            dry_run: true,
            verbose: false,
        };
        let results = executor::execute(&bf, &g, &order, &cache, &opts);
        // Should be skipped, not executed
        assert!(matches!(&results[0], RuleResult::Skipped(_)));
    }

    /// Phony rules should never be skipped by cache.
    #[test]
    fn test_phony_never_cached() {
        let input = "\
rule always_run
  phony true
  run echo running
";
        let cache = Arc::new(Mutex::new(BuildCache::new()));

        let results1 = full_build(input, "always_run", 1, &cache);
        assert!(matches!(&results1[0], RuleResult::Success(_)));

        let results2 = full_build(input, "always_run", 1, &cache);
        assert!(matches!(&results2[0], RuleResult::Success(_)));
    }
}
