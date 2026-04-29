use std::collections::{HashMap, HashSet, VecDeque};

use crate::parser::BuildFile;

/// Build graph: adjacency list + reverse adjacency for dependents.
#[derive(Debug)]
pub struct BuildGraph {
    /// rule_name -> list of dependencies (edges: this rule depends on them)
    pub deps: HashMap<String, Vec<String>>,
    /// rule_name -> list of dependents (reverse edges: rules that depend on this one)
    pub rdeps: HashMap<String, Vec<String>>,
    /// All rule names
    pub nodes: Vec<String>,
}

/// Construct a DAG from the build file, validating that all referenced
/// dependencies actually exist as rules.
pub fn build_graph(bf: &BuildFile) -> Result<BuildGraph, String> {
    let rule_names: HashSet<&str> = bf.rules.keys().map(|s| s.as_str()).collect();

    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    let mut rdeps: HashMap<String, Vec<String>> = HashMap::new();

    for (name, rule) in &bf.rules {
        deps.entry(name.clone()).or_default();
        rdeps.entry(name.clone()).or_default();

        for dep in &rule.deps {
            if !rule_names.contains(dep.as_str()) {
                return Err(format!(
                    "rule '{}' depends on '{}', which is not defined",
                    name, dep
                ));
            }
            deps.entry(name.clone()).or_default().push(dep.clone());
            rdeps.entry(dep.clone()).or_default().push(name.clone());
        }
    }

    let nodes: Vec<String> = bf.rules.keys().cloned().collect();

    let graph = BuildGraph { deps, rdeps, nodes };
    detect_cycle(&graph)?;
    Ok(graph)
}

/// Detect cycles using DFS with three-color marking.
fn detect_cycle(graph: &BuildGraph) -> Result<(), String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: HashMap<&str, Color> = HashMap::new();
    for n in &graph.nodes {
        color.insert(n.as_str(), Color::White);
    }

    let mut path: Vec<&str> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        graph: &'a BuildGraph,
        color: &mut HashMap<&'a str, Color>,
        path: &mut Vec<&'a str>,
    ) -> Result<(), String> {
        color.insert(node, Color::Gray);
        path.push(node);

        if let Some(neighbors) = graph.deps.get(node) {
            for dep in neighbors {
                match color.get(dep.as_str()) {
                    Some(Color::Gray) => {
                        // Found a cycle — extract the cycle path
                        let cycle_start = path.iter().position(|&n| n == dep.as_str()).unwrap();
                        let cycle: Vec<&str> = path[cycle_start..].to_vec();
                        let mut desc = cycle
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        desc.push_str(&format!(" -> {dep}"));
                        return Err(format!("circular dependency detected: {desc}"));
                    }
                    Some(Color::Black) => continue,
                    _ => dfs(dep, graph, color, path)?,
                }
            }
        }

        path.pop();
        color.insert(node, Color::Black);
        Ok(())
    }

    for node in &graph.nodes {
        if color[node.as_str()] == Color::White {
            dfs(node, graph, &mut color, &mut path)?;
        }
    }

    Ok(())
}

/// Returns the set of rules reachable from `target` (including target itself),
/// following the dependency edges transitively.
pub fn reachable_from(target: &str, graph: &BuildGraph) -> Result<HashSet<String>, String> {
    if !graph.deps.contains_key(target) {
        return Err(format!("target '{}' is not defined", target));
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(target.to_string());
    visited.insert(target.to_string());

    while let Some(node) = queue.pop_front() {
        if let Some(deps) = graph.deps.get(&node) {
            for dep in deps {
                if visited.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    Ok(visited)
}

/// Kahn's algorithm for topological sort, restricted to the given subset of nodes.
/// Returns nodes in execution order (leaves first).
pub fn topological_sort(
    graph: &BuildGraph,
    subset: &HashSet<String>,
) -> Vec<String> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for node in subset {
        in_degree.insert(node.as_str(), 0);
    }

    for node in subset {
        if let Some(deps) = graph.deps.get(node.as_str()) {
            for dep in deps {
                if subset.contains(dep) {
                    *in_degree.entry(node.as_str()).or_default() += 1;
                }
            }
        }
    }

    let mut queue: VecDeque<String> = VecDeque::new();

    // Sort for deterministic output
    let mut zero_deg: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&n, _)| n)
        .collect();
    zero_deg.sort();
    for n in zero_deg {
        queue.push_back(n.to_string());
    }

    let mut order = Vec::new();

    while let Some(node) = queue.pop_front() {
        order.push(node.clone());
        if let Some(dependents) = graph.rdeps.get(node.as_str()) {
            let mut ready: Vec<&str> = Vec::new();
            for dependent in dependents {
                if !subset.contains(dependent) {
                    continue;
                }
                let deg = in_degree.get_mut(dependent.as_str()).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    ready.push(dependent.as_str());
                }
            }
            ready.sort();
            for r in ready {
                queue.push_back(r.to_string());
            }
        }
    }

    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn make_buildfile(input: &str) -> BuildFile {
        parser::parse(input).unwrap()
    }

    #[test]
    fn test_simple_graph() {
        let bf = make_buildfile(
            "rule a\n  deps b\n  run echo a\nrule b\n  run echo b\n",
        );
        let g = build_graph(&bf).unwrap();
        assert_eq!(g.deps["a"], vec!["b"]);
        assert!(g.deps["b"].is_empty());
    }

    #[test]
    fn test_cycle_detection() {
        let bf = make_buildfile(
            "rule a\n  deps b\n  run echo a\nrule b\n  deps a\n  run echo b\n",
        );
        let result = build_graph(&bf);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("circular dependency"), "got: {err}");
    }

    #[test]
    fn test_three_node_cycle() {
        let bf = make_buildfile(
            "rule a\n  deps b\n  run echo a\nrule b\n  deps c\n  run echo b\nrule c\n  deps a\n  run echo c\n",
        );
        let result = build_graph(&bf);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("circular dependency"));
    }

    #[test]
    fn test_missing_dependency() {
        let bf = make_buildfile("rule a\n  deps ghost\n  run echo a\n");
        let result = build_graph(&bf);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not defined"));
    }

    #[test]
    fn test_diamond_topo_sort() {
        let bf = make_buildfile(
            "\
rule a\n  deps b c\n  run echo a
rule b\n  deps d\n  run echo b
rule c\n  deps d\n  run echo c
rule d\n  run echo d\n",
        );
        let g = build_graph(&bf).unwrap();
        let all: HashSet<String> = g.nodes.iter().cloned().collect();
        let order = topological_sort(&g, &all);
        assert_eq!(order.len(), 4);
        // d must come before b and c; b and c must come before a
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("d") < pos("b"));
        assert!(pos("d") < pos("c"));
        assert!(pos("b") < pos("a"));
        assert!(pos("c") < pos("a"));
    }

    #[test]
    fn test_reachable_from() {
        let bf = make_buildfile(
            "\
rule a\n  deps b c\n  run echo a
rule b\n  deps d\n  run echo b
rule c\n  run echo c
rule d\n  run echo d
rule e\n  run echo e\n",
        );
        let g = build_graph(&bf).unwrap();
        let reachable = reachable_from("a", &g).unwrap();
        assert!(reachable.contains("a"));
        assert!(reachable.contains("b"));
        assert!(reachable.contains("c"));
        assert!(reachable.contains("d"));
        assert!(!reachable.contains("e"));
    }

    #[test]
    fn test_reachable_from_unknown() {
        let bf = make_buildfile("rule a\n  run echo a\n");
        let g = build_graph(&bf).unwrap();
        assert!(reachable_from("ghost", &g).is_err());
    }
}