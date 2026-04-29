use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::cache::BuildCache;
use crate::graph::BuildGraph;
use crate::parser::{expand_vars, BuildFile, Rule};

/// Outcome of a single rule execution.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RuleResult {
    Success(String),
    Skipped(String),
    Failed(String, String),
}

/// Message from worker threads back to the scheduler.
enum WorkerMsg {
    Done(String, Result<(), String>),
}

/// Options controlling execution.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub jobs: usize,
    pub dry_run: bool,
    pub verbose: bool,
}

/// Execute the build plan.
///
/// `order` is the topologically sorted list of rules to execute.
/// Returns a list of results for each rule.
pub fn execute(
    bf: &BuildFile,
    graph: &BuildGraph,
    order: &[String],
    cache: &Arc<Mutex<BuildCache>>,
    opts: &ExecOptions,
) -> Vec<RuleResult> {
    let subset: HashSet<String> = order.iter().cloned().collect();

    // In-degree for each node within the subset
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for name in order {
        let mut deg = 0usize;
        if let Some(deps) = graph.deps.get(name) {
            for dep in deps {
                if subset.contains(dep) {
                    deg += 1;
                }
            }
        }
        in_degree.insert(name.clone(), deg);
    }

    let mut results: Vec<RuleResult> = Vec::new();
    let mut completed: HashSet<String> = HashSet::new();
    let mut failed: HashSet<String> = HashSet::new();
    let mut in_flight: HashSet<String> = HashSet::new();

    // Channels for worker communication
    let (done_tx, done_rx) = mpsc::channel::<WorkerMsg>();

    // Merge global + rule env for each rule
    let merged_envs: HashMap<String, HashMap<String, String>> = order
        .iter()
        .map(|name| {
            let rule = &bf.rules[name];
            let mut env = bf.global_env.clone();
            for (k, v) in &rule.env {
                env.insert(k.clone(), v.clone());
            }
            (name.clone(), env)
        })
        .collect();

    let mut ready_queue: Vec<String> = Vec::new();

    // Seed ready queue with zero in-degree nodes
    for name in order {
        if in_degree[name] == 0 {
            ready_queue.push(name.clone());
        }
    }
    ready_queue.sort();

    let total = order.len();

    loop {
        // Launch jobs up to parallelism limit
        while !ready_queue.is_empty() && in_flight.len() < opts.jobs {
            let name = ready_queue.remove(0);

            // Check if any upstream dependency failed
            let upstream_failed = if let Some(deps) = graph.deps.get(&name) {
                deps.iter().any(|d| failed.contains(d))
            } else {
                false
            };

            if upstream_failed {
                eprintln!("[SKIP] {} (dependency failed)", name);
                failed.insert(name.clone());
                results.push(RuleResult::Failed(
                    name.clone(),
                    "dependency failed".to_string(),
                ));
                // Propagate: unblock dependents so they can also be skipped
                notify_dependents(
                    &name,
                    graph,
                    &subset,
                    &mut in_degree,
                    &mut ready_queue,
                );
                continue;
            }

            let rule = &bf.rules[&name];
            let env = &merged_envs[&name];

            // Incremental build check
            if !rule.phony && !rule.inputs.is_empty() {
                let cache_guard = cache.lock().unwrap();
                if cache_guard.is_up_to_date(&name, &rule.inputs, &rule.outputs) {
                    if opts.verbose {
                        eprintln!("[UP-TO-DATE] {}", name);
                    }
                    completed.insert(name.clone());
                    results.push(RuleResult::Skipped(name.clone()));
                    notify_dependents(
                        &name,
                        graph,
                        &subset,
                        &mut in_degree,
                        &mut ready_queue,
                    );
                    continue;
                }
            }

            if opts.dry_run {
                let expanded_cmds: Vec<String> = rule
                    .commands
                    .iter()
                    .map(|c| expand_vars(c, env))
                    .collect();
                eprintln!("[DRY-RUN] {} -> {}", name, expanded_cmds.join(" && "));
                completed.insert(name.clone());
                results.push(RuleResult::Skipped(name.clone()));
                notify_dependents(
                    &name,
                    graph,
                    &subset,
                    &mut in_degree,
                    &mut ready_queue,
                );
                continue;
            }

            // Spawn worker thread
            let rule_clone = rule.clone();
            let env_clone = env.clone();
            let tx = done_tx.clone();
            let name_clone = name.clone();
            let verbose = opts.verbose;

            in_flight.insert(name.clone());

            thread::spawn(move || {
                let result = execute_rule(&name_clone, &rule_clone, &env_clone, verbose);
                let _ = tx.send(WorkerMsg::Done(name_clone, result));
            });
        }

        // If nothing in flight and no ready work, we're done
        if in_flight.is_empty() {
            break;
        }

        // Wait for a worker to finish
        match done_rx.recv() {
            Ok(WorkerMsg::Done(name, result)) => {
                in_flight.remove(&name);
                match result {
                    Ok(()) => {
                        let rule = &bf.rules[&name];
                        // Record in cache
                        if !rule.phony {
                            let mut cache_guard = cache.lock().unwrap();
                            cache_guard.record(&name, &rule.inputs, &rule.outputs);
                        }
                        eprintln!(
                            "[OK] {} ({}/{})",
                            name,
                            completed.len() + failed.len() + 1,
                            total
                        );
                        completed.insert(name.clone());
                        results.push(RuleResult::Success(name.clone()));
                        notify_dependents(
                            &name,
                            graph,
                            &subset,
                            &mut in_degree,
                            &mut ready_queue,
                        );
                    }
                    Err(msg) => {
                        eprintln!("[FAIL] {}: {}", name, msg);
                        failed.insert(name.clone());
                        results.push(RuleResult::Failed(name.clone(), msg));
                        // Propagate failure: unblock dependents so they get skipped
                        notify_dependents(
                            &name,
                            graph,
                            &subset,
                            &mut in_degree,
                            &mut ready_queue,
                        );
                    }
                }
            }
            Err(_) => break,
        }
    }

    if !failed.is_empty() {
        let failed_list: Vec<&String> = failed.iter().collect();
        eprintln!(
            "\nBuild FAILED. {} rule(s) failed: {:?}",
            failed.len(),
            failed_list
        );
    } else {
        eprintln!(
            "\nBuild OK. {} rule(s) completed, {} skipped.",
            results.iter().filter(|r| matches!(r, RuleResult::Success(_))).count(),
            results.iter().filter(|r| matches!(r, RuleResult::Skipped(_))).count(),
        );
    }

    results
}

/// When a rule completes (success or failure), decrement in-degree of its
/// dependents and enqueue any that become ready.
fn notify_dependents(
    name: &str,
    graph: &BuildGraph,
    subset: &HashSet<String>,
    in_degree: &mut HashMap<String, usize>,
    ready_queue: &mut Vec<String>,
) {
    if let Some(dependents) = graph.rdeps.get(name) {
        let mut newly_ready = Vec::new();
        for dep in dependents {
            if !subset.contains(dep) {
                continue;
            }
            if let Some(deg) = in_degree.get_mut(dep.as_str()) {
                *deg = deg.saturating_sub(1);
                if *deg == 0 {
                    newly_ready.push(dep.clone());
                }
            }
        }
        newly_ready.sort();
        ready_queue.extend(newly_ready);
    }
}

/// Execute a single rule's commands sequentially.
fn execute_rule(
    name: &str,
    rule: &Rule,
    env: &HashMap<String, String>,
    verbose: bool,
) -> Result<(), String> {
    if rule.commands.is_empty() {
        return Ok(());
    }

    for cmd_template in &rule.commands {
        let cmd = expand_vars(cmd_template, env);

        if verbose {
            eprintln!("[RUN] {}: {}", name, cmd);
        }

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn '{}': {}", cmd, e))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let rule_name = name.to_string();

        // Stream stdout in a thread
        let rule_name_out = rule_name.clone();
        let stdout_handle = stdout.map(|out| {
            thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        println!("[{}] {}", rule_name_out, line);
                    }
                }
            })
        });

        // Stream stderr in a thread
        let rule_name_err = rule_name.clone();
        let stderr_handle = stderr.map(|err| {
            thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        eprintln!("[{}] {}", rule_name_err, line);
                    }
                }
            })
        });

        if let Some(h) = stdout_handle {
            let _ = h.join();
        }
        if let Some(h) = stderr_handle {
            let _ = h.join();
        }

        let status = child
            .wait()
            .map_err(|e| format!("failed to wait on '{}': {}", cmd, e))?;

        if !status.success() {
            let code = status.code().unwrap_or(-1);
            return Err(format!("command '{}' exited with code {}", cmd, code));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::build_graph;
    use crate::parser;

    fn run_build(input: &str, target: Option<&str>, jobs: usize) -> Vec<RuleResult> {
        let bf = parser::parse(input).unwrap();
        let graph = build_graph(&bf).unwrap();

        let target_name = target
            .map(String::from)
            .or_else(|| bf.default_target.clone())
            .unwrap_or_else(|| bf.rules.keys().next().unwrap().clone());

        let reachable = crate::graph::reachable_from(&target_name, &graph).unwrap();
        let order = crate::graph::topological_sort(&graph, &reachable);

        let cache = Arc::new(Mutex::new(BuildCache::new()));
        let opts = ExecOptions {
            jobs,
            dry_run: false,
            verbose: false,
        };
        execute(&bf, &graph, &order, &cache, &opts)
    }

    #[test]
    fn test_simple_execution() {
        let results = run_build("rule hello\n  run echo hello world\n", None, 1);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], RuleResult::Success(n) if n == "hello"));
    }

    #[test]
    fn test_dependency_order() {
        let input = "\
rule top\n  deps mid\n  run echo top
rule mid\n  deps bottom\n  run echo mid
rule bottom\n  run echo bottom\n";
        let results = run_build(input, Some("top"), 1);
        assert_eq!(results.len(), 3);
        let names: Vec<&str> = results
            .iter()
            .filter_map(|r| match r {
                RuleResult::Success(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["bottom", "mid", "top"]);
    }

    #[test]
    fn test_failure_propagation() {
        let input = "\
rule top\n  deps failing\n  run echo should not run
rule failing\n  run exit 1\n";
        let results = run_build(input, Some("top"), 1);
        let failed_count = results
            .iter()
            .filter(|r| matches!(r, RuleResult::Failed(_, _)))
            .count();
        assert_eq!(failed_count, 2);
    }

    #[test]
    fn test_parallel_execution() {
        // Four independent rules — should all succeed with 4 workers
        let input = "\
rule a\n  run echo a
rule b\n  run echo b
rule c\n  run echo c
rule d\n  run echo d
rule all\n  deps a b c d\n  phony true\n  run echo all done\n";
        let results = run_build(input, Some("all"), 4);
        let success_count = results
            .iter()
            .filter(|r| matches!(r, RuleResult::Success(_)))
            .count();
        assert_eq!(success_count, 5);
    }

    #[test]
    fn test_env_expansion() {
        let input = "\
env GREETING = hello
rule test\n  env NAME = world\n  run echo $GREETING $NAME\n";
        let results = run_build(input, Some("test"), 1);
        assert!(matches!(&results[0], RuleResult::Success(_)));
    }
}