use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// A single build rule parsed from the Buildfile.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub commands: Vec<String>,
    pub deps: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub env: HashMap<String, String>,
    pub description: String,
    pub phony: bool,
}

impl Rule {
    fn new(name: String) -> Self {
        Self {
            name,
            commands: Vec::new(),
            deps: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            env: HashMap::new(),
            description: String::new(),
            phony: false,
        }
    }
}

/// Top-level Buildfile representation.
#[derive(Debug, Clone)]
pub struct BuildFile {
    pub rules: HashMap<String, Rule>,
    pub global_env: HashMap<String, String>,
    pub default_target: Option<String>,
}

/// Parses the Buildfile DSL. Format:
///
/// ```text
/// # comment
/// include common.mb
/// env CC = gcc
/// env CFLAGS = -Wall -O2
///
/// default all
///
/// rule all
///   deps build test
///   description Build everything
///   phony true
///
/// rule compile
///   deps []
///   inputs src/main.c src/util.c
///   outputs build/main.o build/util.o
///   env EXTRA = -DDEBUG
///   run $CC $CFLAGS $EXTRA -c src/main.c -o build/main.o
///   run $CC $CFLAGS $EXTRA -c src/util.c -o build/util.o
/// ```
///
/// `include` is only supported via [`parse_file`]. Paths resolve relative
/// to the including file. The string [`parse`] entry point rejects `include`
/// so it never touches the filesystem.
///
/// The binary loads files via [`parse_file`]; this string entry point is
/// the API for tests and in-memory Buildfiles.
#[allow(dead_code)]
pub fn parse(input: &str) -> Result<BuildFile, String> {
    let mut rules: HashMap<String, Rule> = HashMap::new();
    let mut global_env: HashMap<String, String> = HashMap::new();
    let mut default_target: Option<String> = None;
    parse_into(
        input,
        Path::new("."),
        None,
        &mut rules,
        &mut global_env,
        &mut default_target,
    )?;
    finish_parse(rules, global_env, default_target)
}

/// Parse a Buildfile from disk so `include` paths resolve relative to it.
pub fn parse_file(path: &Path) -> Result<BuildFile, String> {
    let canonical =
        fs::canonicalize(path).map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let content = fs::read_to_string(&canonical)
        .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
    let base = parent_dir(path);
    let mut ctx = IncludeCtx {
        stack: vec![IncludeFrame {
            display: path_display(path),
            canonical,
        }],
        loaded: HashSet::new(),
    };
    let mut rules: HashMap<String, Rule> = HashMap::new();
    let mut global_env: HashMap<String, String> = HashMap::new();
    let mut default_target: Option<String> = None;
    parse_into(
        &content,
        base,
        Some(&mut ctx),
        &mut rules,
        &mut global_env,
        &mut default_target,
    )?;
    finish_parse(rules, global_env, default_target)
}

struct IncludeFrame {
    display: String,
    canonical: PathBuf,
}

struct IncludeCtx {
    stack: Vec<IncludeFrame>,
    loaded: HashSet<PathBuf>,
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

fn path_display(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn finish_parse(
    rules: HashMap<String, Rule>,
    global_env: HashMap<String, String>,
    default_target: Option<String>,
) -> Result<BuildFile, String> {
    if rules.is_empty() {
        return Err("Buildfile contains no rules".to_string());
    }
    Ok(BuildFile {
        rules,
        global_env,
        default_target,
    })
}

fn parse_into(
    input: &str,
    base_dir: &Path,
    mut include: Option<&mut IncludeCtx>,
    rules: &mut HashMap<String, Rule>,
    global_env: &mut HashMap<String, String>,
    default_target: &mut Option<String>,
) -> Result<(), String> {
    let mut current_rule: Option<Rule> = None;
    let mut last_line_num = 0;

    for (line_no, raw_line) in input.lines().enumerate() {
        let line_num = line_no + 1;
        last_line_num = line_num;
        let line = raw_line.trim();

        // blank or comment
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // global directives (not indented)
        if !raw_line.starts_with(' ') && !raw_line.starts_with('\t') {
            // finalize previous rule
            if let Some(r) = current_rule.take() {
                insert_rule(rules, r, line_num)?;
            }

            if line == "rule" {
                return Err(format!("line {line_num}: rule has no name"));
            } else if let Some(rest) = line.strip_prefix("rule ") {
                let name = rest.trim().to_string();
                if name.is_empty() {
                    return Err(format!("line {line_num}: rule has no name"));
                }
                current_rule = Some(Rule::new(name));
            } else if let Some(rest) = line.strip_prefix("env ") {
                let (k, v) = parse_kv(rest, line_num)?;
                global_env.insert(k, v);
            } else if let Some(rest) = line.strip_prefix("default ") {
                *default_target = Some(rest.trim().to_string());
            } else if line == "include" {
                return Err(format!("line {line_num}: include has no path"));
            } else if let Some(rest) = line.strip_prefix("include ") {
                let spec = rest.trim();
                if spec.is_empty() {
                    return Err(format!("line {line_num}: include has no path"));
                }
                match include.as_mut() {
                    Some(ctx) => {
                        include_file(spec, base_dir, ctx, rules, global_env, default_target)?;
                    }
                    None => {
                        return Err(format!(
                            "line {line_num}: include requires a file path; use parse_file"
                        ));
                    }
                }
            } else {
                let token = line.split_whitespace().next().unwrap_or(line);
                const TOP: &[&str] = &["env", "default", "rule", "include"];
                return Err(crate::suggest::with_hint(
                    &format!("line {line_num}: unexpected top-level directive: {line}"),
                    token,
                    TOP,
                ));
            }
        } else {
            // indented line — belongs to current rule
            let rule = current_rule
                .as_mut()
                .ok_or_else(|| format!("line {line_num}: indented line outside of a rule block"))?;

            if let Some(rest) = line.strip_prefix("deps ") {
                rule.deps = split_list(rest);
            } else if let Some(rest) = line.strip_prefix("inputs ") {
                rule.inputs = split_list(rest);
            } else if let Some(rest) = line.strip_prefix("outputs ") {
                rule.outputs = split_list(rest);
            } else if let Some(rest) = line.strip_prefix("env ") {
                let (k, v) = parse_kv(rest, line_num)?;
                rule.env.insert(k, v);
            } else if let Some(rest) = line.strip_prefix("run ") {
                rule.commands.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("description ") {
                rule.description = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("phony ") {
                rule.phony = rest.trim().eq_ignore_ascii_case("true");
            } else {
                let token = line.split_whitespace().next().unwrap_or(line);
                const INNER: &[&str] = &[
                    "deps",
                    "inputs",
                    "outputs",
                    "env",
                    "run",
                    "description",
                    "phony",
                ];
                return Err(crate::suggest::with_hint(
                    &format!("line {line_num}: unknown rule directive: {line}"),
                    token,
                    INNER,
                ));
            }
        }
    }

    // finalize last rule
    if let Some(r) = current_rule.take() {
        insert_rule(rules, r, last_line_num)?;
    }

    Ok(())
}

fn include_file(
    spec: &str,
    base_dir: &Path,
    ctx: &mut IncludeCtx,
    rules: &mut HashMap<String, Rule>,
    global_env: &mut HashMap<String, String>,
    default_target: &mut Option<String>,
) -> Result<(), String> {
    let path = base_dir.join(spec);
    let canonical = fs::canonicalize(&path).map_err(|e| format!("cannot include '{spec}': {e}"))?;

    if let Some(pos) = ctx.stack.iter().position(|f| f.canonical == canonical) {
        let mut parts: Vec<&str> = ctx.stack[pos..]
            .iter()
            .map(|f| f.display.as_str())
            .collect();
        parts.push(spec);
        return Err(format!("include cycle: {}", parts.join(" -> ")));
    }

    if ctx.loaded.contains(&canonical) {
        return Ok(());
    }

    let content =
        fs::read_to_string(&canonical).map_err(|e| format!("cannot include '{spec}': {e}"))?;
    let child_base = parent_dir(&path);
    ctx.stack.push(IncludeFrame {
        display: spec.to_string(),
        canonical: canonical.clone(),
    });
    let result = parse_into(
        &content,
        child_base,
        Some(ctx),
        rules,
        global_env,
        default_target,
    );
    ctx.stack.pop();
    if result.is_ok() {
        ctx.loaded.insert(canonical);
    }
    result
}

fn insert_rule(
    rules: &mut HashMap<String, Rule>,
    rule: Rule,
    line_num: usize,
) -> Result<(), String> {
    if rules.contains_key(&rule.name) {
        return Err(format!("line {line_num}: duplicate rule '{}'", rule.name));
    }
    rules.insert(rule.name.clone(), rule);
    Ok(())
}

fn parse_kv(s: &str, line_num: usize) -> Result<(String, String), String> {
    let s = s.trim();
    let eq_pos = s
        .find('=')
        .ok_or_else(|| format!("line {line_num}: expected KEY = VALUE, got: {s}"))?;
    let key = s[..eq_pos].trim().to_string();
    let val = s[eq_pos + 1..].trim().to_string();
    if key.is_empty() {
        return Err(format!("line {line_num}: empty key in env directive"));
    }
    Ok((key, val))
}

fn split_list(s: &str) -> Vec<String> {
    s.split_whitespace()
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Expand `$VAR` and `${VAR}` references in a string using the given env map.
pub fn expand_vars(s: &str, env: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'{' {
                // ${VAR} form
                if let Some(end) = s[i + 2..].find('}') {
                    let var_name = &s[i + 2..i + 2 + end];
                    if let Some(val) = env.get(var_name) {
                        result.push_str(val);
                    }
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            // $VAR form
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                let var_name = &s[start..end];
                if let Some(val) = env.get(var_name) {
                    result.push_str(val);
                }
                i = end;
                continue;
            }
        }
        let ch = match s[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal() {
        let input = "rule hello\n  run echo hello\n";
        let bf = parse(input).unwrap();
        assert_eq!(bf.rules.len(), 1);
        assert_eq!(bf.rules["hello"].commands, vec!["echo hello"]);
    }

    #[test]
    fn test_parse_full() {
        let input = "\
env CC = gcc
env CFLAGS = -Wall

default all

rule all
  deps compile link
  description Build everything
  phony true

rule compile
  inputs src/main.c
  outputs build/main.o
  env EXTRA = -DDEBUG
  run $CC $CFLAGS -c src/main.c -o build/main.o

rule link
  deps compile
  inputs build/main.o
  outputs build/app
  run $CC build/main.o -o build/app
";
        let bf = parse(input).unwrap();
        assert_eq!(bf.rules.len(), 3);
        assert_eq!(bf.global_env["CC"], "gcc");
        assert_eq!(bf.default_target.as_deref(), Some("all"));
        assert!(bf.rules["all"].phony);
        assert_eq!(bf.rules["all"].deps, vec!["compile", "link"]);
        assert_eq!(bf.rules["compile"].inputs, vec!["src/main.c"]);
        assert_eq!(bf.rules["compile"].outputs, vec!["build/main.o"]);
        assert_eq!(bf.rules["compile"].env["EXTRA"], "-DDEBUG");
    }

    #[test]
    fn test_parse_duplicate_rule() {
        let input = "rule a\n  run echo a\nrule a\n  run echo b\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("duplicate rule"), "got: {err}");
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse("").is_err());
        assert!(parse("# just comments\n").is_err());
    }

    #[test]
    fn test_expand_vars() {
        let mut env = HashMap::new();
        env.insert("CC".to_string(), "gcc".to_string());
        env.insert("FLAGS".to_string(), "-O2".to_string());
        assert_eq!(expand_vars("$CC $FLAGS -c foo.c", &env), "gcc -O2 -c foo.c");
        assert_eq!(expand_vars("${CC} ${FLAGS}", &env), "gcc -O2");
    }

    #[test]
    fn test_expand_vars_missing() {
        let env = HashMap::new();
        assert_eq!(expand_vars("$MISSING test", &env), " test");
    }

    #[test]
    fn test_expand_vars_utf8_literal() {
        let env = HashMap::new();
        assert_eq!(expand_vars("echo café", &env), "echo café");
    }

    #[test]
    fn test_expand_vars_utf8_value() {
        let mut env = HashMap::new();
        env.insert("GREETING".to_string(), "café".to_string());
        assert_eq!(expand_vars("echo $GREETING", &env), "echo café");
    }

    #[test]
    fn test_parse_indented_line_outside_rule() {
        let input = "  run echo orphan\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("indented line outside of a rule block"));
    }

    #[test]
    fn test_parse_empty_rule_name() {
        // "rule" alone (no name following) should error
        let input = "rule\n  run echo test\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("rule has no name"));
    }

    #[test]
    fn test_parse_rule_trailing_whitespace_only() {
        // "rule   " (only whitespace after prefix) should also error
        let input = "rule   \n  run echo test\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("rule has no name"));
    }

    #[test]
    fn test_parse_unknown_top_level_directive() {
        let input = "unknown_directive something\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("unexpected top-level directive"));
    }

    #[test]
    fn test_parse_unknown_top_level_suggests() {
        let err = parse("rul hello\n").unwrap_err();
        assert!(err.contains("unexpected top-level directive"), "got: {err}");
        assert!(err.contains("did you mean `rule`"), "got: {err}");
    }

    #[test]
    fn test_parse_unknown_rule_directive() {
        let input = "rule a\n  badkey value\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("unknown rule directive"));
    }

    #[test]
    fn test_parse_unknown_rule_suggests() {
        let err = parse("rule a\n  dep b\n").unwrap_err();
        assert!(err.contains("unknown rule directive"), "got: {err}");
        assert!(err.contains("did you mean `deps`"), "got: {err}");
    }

    #[test]
    fn test_expand_vars_bare_dollar_at_end() {
        let env = HashMap::new();
        // Bare $ at end of string should pass through as literal $
        assert_eq!(expand_vars("hello$", &env), "hello$");
    }

    #[test]
    fn test_expand_vars_unclosed_brace() {
        let env = HashMap::new();
        // ${VAR without closing brace should pass through as literal
        assert_eq!(expand_vars("${UNCLOSED", &env), "${UNCLOSED");
    }

    #[test]
    fn test_parse_env_empty_key() {
        let input = "env  = value\nrule a\n  run echo a\n";
        let err = parse(input).unwrap_err();
        assert!(err.contains("empty key"));
    }

    #[test]
    fn test_expand_vars_double_dollar() {
        let mut env = HashMap::new();
        env.insert("X".to_string(), "val".to_string());
        // First $ sees next char $ (not alphanumeric), passes through as literal $.
        // Second $ sees X, expands to "val". Result: "$val"
        let result = expand_vars("$$X", &env);
        assert_eq!(result, "$val");
    }

    #[test]
    fn test_parse_unicode_rule_name() {
        let input = "rule caf\u{00e9}\n  run echo hello\n";
        let bf = parse(input).unwrap();
        assert!(bf.rules.contains_key("caf\u{00e9}"));
    }

    #[test]
    fn test_parse_many_deps() {
        let mut input = String::from("rule top\n  deps");
        for i in 0..50 {
            input.push_str(&format!(" dep{i}"));
        }
        input.push('\n');
        for i in 0..50 {
            input.push_str(&format!("rule dep{i}\n  run echo {i}\n"));
        }
        let bf = parse(&input).unwrap();
        assert_eq!(bf.rules["top"].deps.len(), 50);
    }

    #[test]
    fn test_parse_env_missing_equals() {
        let input = "env BROKEN\nrule a\n  run echo a\n";
        assert!(parse(input).is_err());
    }

    fn write_temp_build(dir_name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(dir_name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            fs::write(dir.join(name), body).unwrap();
        }
        dir
    }

    #[test]
    fn test_include_sibling_rule() {
        let dir = write_temp_build(
            "minibuild_test_include_sibling",
            &[
                ("child.mb", "rule child\n  run echo child\n"),
                (
                    "parent.mb",
                    "include child.mb\nrule parent\n  deps child\n  run echo parent\n",
                ),
            ],
        );
        let bf = parse_file(&dir.join("parent.mb")).unwrap();
        assert!(bf.rules.contains_key("child"));
        assert!(bf.rules.contains_key("parent"));
        assert_eq!(bf.rules["parent"].deps, vec!["child"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_cycle() {
        let dir = write_temp_build(
            "minibuild_test_include_cycle",
            &[
                ("a", "include b\nrule ra\n  run echo a\n"),
                ("b", "include a\nrule rb\n  run echo b\n"),
            ],
        );
        let err = parse_file(&dir.join("a")).unwrap_err();
        assert!(err.contains("include cycle: a -> b -> a"), "got: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_missing_file() {
        let dir = write_temp_build(
            "minibuild_test_include_missing",
            &[("parent.mb", "include missing.mb\nrule a\n  run echo a\n")],
        );
        let err = parse_file(&dir.join("parent.mb")).unwrap_err();
        assert!(err.contains("cannot include 'missing.mb'"), "got: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_duplicate_rule() {
        let dir = write_temp_build(
            "minibuild_test_include_duplicate",
            &[
                ("child.mb", "rule shared\n  run echo child\n"),
                (
                    "parent.mb",
                    "include child.mb\nrule shared\n  run echo parent\n",
                ),
            ],
        );
        let err = parse_file(&dir.join("parent.mb")).unwrap_err();
        assert!(err.contains("duplicate rule"), "got: {err}");
        assert!(err.contains("shared"), "got: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_diamond() {
        let dir = write_temp_build(
            "minibuild_test_include_diamond",
            &[
                ("common.mb", "rule shared\n  run echo shared\n"),
                (
                    "b.mb",
                    "include common.mb\nrule b\n  deps shared\n  run echo b\n",
                ),
                (
                    "c.mb",
                    "include common.mb\nrule c\n  deps shared\n  run echo c\n",
                ),
                (
                    "root.mb",
                    "include b.mb\ninclude c.mb\nrule root\n  deps b c\n  run echo root\n",
                ),
            ],
        );
        let bf = parse_file(&dir.join("root.mb")).unwrap();
        assert!(bf.rules.contains_key("shared"));
        assert!(bf.rules.contains_key("b"));
        assert!(bf.rules.contains_key("c"));
        assert!(bf.rules.contains_key("root"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_same_file_twice() {
        let dir = write_temp_build(
            "minibuild_test_include_twice",
            &[
                ("child.mb", "rule child\n  run echo child\n"),
                (
                    "parent.mb",
                    "include child.mb\ninclude child.mb\nrule parent\n  deps child\n  run echo parent\n",
                ),
            ],
        );
        let bf = parse_file(&dir.join("parent.mb")).unwrap();
        assert!(bf.rules.contains_key("child"));
        assert!(bf.rules.contains_key("parent"));
        assert_eq!(bf.rules.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_include_rejects_without_filesystem() {
        let err = parse("include x\n").unwrap_err();
        assert!(
            err.contains("include requires a file path; use parse_file"),
            "got: {err}"
        );
        assert!(!err.contains("cannot include"), "got: {err}");
        assert!(!err.contains("cannot read"), "got: {err}");
    }
}
