use std::collections::HashMap;

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
pub fn parse(input: &str) -> Result<BuildFile, String> {
    let mut rules: HashMap<String, Rule> = HashMap::new();
    let mut global_env: HashMap<String, String> = HashMap::new();
    let mut default_target: Option<String> = None;
    let mut current_rule: Option<Rule> = None;

    for (line_no, raw_line) in input.lines().enumerate() {
        let line_num = line_no + 1;
        let line = raw_line.trim();

        // blank or comment
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // global directives (not indented)
        if !raw_line.starts_with(' ') && !raw_line.starts_with('\t') {
            // finalize previous rule
            if let Some(r) = current_rule.take() {
                if rules.contains_key(&r.name) {
                    return Err(format!("line {line_num}: duplicate rule '{}'", r.name));
                }
                rules.insert(r.name.clone(), r);
            }

            if let Some(rest) = line.strip_prefix("rule ") {
                let name = rest.trim().to_string();
                if name.is_empty() {
                    return Err(format!("line {line_num}: rule has no name"));
                }
                current_rule = Some(Rule::new(name));
            } else if let Some(rest) = line.strip_prefix("env ") {
                let (k, v) = parse_kv(rest, line_num)?;
                global_env.insert(k, v);
            } else if let Some(rest) = line.strip_prefix("default ") {
                default_target = Some(rest.trim().to_string());
            } else {
                return Err(format!(
                    "line {line_num}: unexpected top-level directive: {line}"
                ));
            }
        } else {
            // indented line — belongs to current rule
            let rule = current_rule.as_mut().ok_or_else(|| {
                format!("line {line_num}: indented line outside of a rule block")
            })?;

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
                return Err(format!(
                    "line {line_num}: unknown rule directive: {line}"
                ));
            }
        }
    }

    // finalize last rule
    if let Some(r) = current_rule.take() {
        if rules.contains_key(&r.name) {
            return Err(format!("duplicate rule '{}'", r.name));
        }
        rules.insert(r.name.clone(), r);
    }

    if rules.is_empty() {
        return Err("Buildfile contains no rules".to_string());
    }

    Ok(BuildFile {
        rules,
        global_env,
        default_target,
    })
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
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
            {
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
        result.push(bytes[i] as char);
        i += 1;
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
        assert!(parse(input).is_err());
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
    fn test_parse_env_missing_equals() {
        let input = "env BROKEN\nrule a\n  run echo a\n";
        assert!(parse(input).is_err());
    }
}