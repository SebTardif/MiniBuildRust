use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub file: String,
    pub jobs: usize,
    pub target: Option<String>,
    pub clean: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub json: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(4);
        Self {
            file: "Buildfile".to_string(),
            jobs: cpus,
            target: None,
            clean: false,
            dry_run: false,
            verbose: false,
            json: false,
        }
    }
}

/// Outcome of CLI argument parsing.
#[derive(Debug)]
pub enum ParseOutcome {
    /// Successfully parsed arguments; proceed with build.
    Run(CliArgs),
    /// Informational output (--help, --version); print to stdout and exit 0.
    Info(String),
}

pub fn parse_args(args: &[String]) -> Result<ParseOutcome, String> {
    let mut cli = CliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--file" | "-f" => {
                i += 1;
                if i >= args.len() {
                    return Err("--file requires a value".to_string());
                }
                cli.file = args[i].clone();
            }
            "--jobs" | "-j" => {
                i += 1;
                if i >= args.len() {
                    return Err("--jobs requires a value".to_string());
                }
                cli.jobs = args[i]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --jobs value: {}", args[i]))?;
                if cli.jobs == 0 {
                    return Err("--jobs must be at least 1".to_string());
                }
            }
            "--clean" => cli.clean = true,
            "--dry-run" | "-n" => cli.dry_run = true,
            "--json" => cli.json = true,
            "--verbose" | "-v" => cli.verbose = true,
            "--version" | "-V" => {
                return Ok(ParseOutcome::Info(format!(
                    "minibuild {}",
                    env!("CARGO_PKG_VERSION")
                )));
            }
            "--help" | "-h" => {
                return Ok(ParseOutcome::Info(usage()));
            }
            s if s.starts_with('-') => {
                if is_glued_jobs(s) {
                    return Err("use `-j 4` or `--jobs 4`".to_string());
                }
                const FLAGS: &[&str] = &[
                    "--file",
                    "-f",
                    "--jobs",
                    "-j",
                    "--clean",
                    "--dry-run",
                    "-n",
                    "--json",
                    "--verbose",
                    "-v",
                    "--version",
                    "-V",
                    "--help",
                    "-h",
                ];
                return Err(match crate::suggest::closest(s, FLAGS) {
                    Some(hint) => format!("unknown flag: {s} (did you mean `{hint}`?)"),
                    None => format!("unknown flag: {s}"),
                });
            }
            _ => {
                if cli.target.is_some() {
                    return Err(format!("unexpected argument: {}", args[i]));
                }
                cli.target = Some(args[i].clone());
            }
        }
        i += 1;
    }
    if cli.json && !cli.dry_run {
        return Err("--json requires --dry-run".to_string());
    }
    Ok(ParseOutcome::Run(cli))
}

fn is_glued_jobs(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("-j") {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    if let Some(rest) = s.strip_prefix("--jobs=") {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn usage() -> String {
    "Usage: minibuild [OPTIONS] [TARGET]\n\n\
     Options:\n  \
       --file, -f <FILE>   Build file path (default: Buildfile)\n  \
       --jobs, -j <N>      Max parallel jobs (default: number of CPU cores)\n  \
       --clean             Remove the build cache and rebuild everything\n  \
       --dry-run, -n       Print what would be executed without running anything\n  \
       --json              Write dry-run plan as JSON to stdout (requires --dry-run)\n  \
       --verbose, -v       Show detailed execution info\n  \
       --version, -V       Show version\n  \
       --help, -h          Show help"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract CliArgs from a ParseOutcome::Run, panicking on Info or Err.
    fn unwrap_run(result: Result<ParseOutcome, String>) -> CliArgs {
        match result {
            Ok(ParseOutcome::Run(cli)) => cli,
            Ok(ParseOutcome::Info(msg)) => panic!("expected Run, got Info: {msg}"),
            Err(e) => panic!("expected Run, got Err: {e}"),
        }
    }

    #[test]
    fn test_defaults() {
        let cli = unwrap_run(parse_args(&[]));
        assert_eq!(cli.file, "Buildfile");
        assert!(cli.jobs >= 1);
        assert!(cli.target.is_none());
    }

    #[test]
    fn test_all_flags() {
        let args: Vec<String> = vec![
            "--file",
            "build.mb",
            "--jobs",
            "8",
            "--clean",
            "--dry-run",
            "--json",
            "--verbose",
            "all",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let cli = unwrap_run(parse_args(&args));
        assert_eq!(cli.file, "build.mb");
        assert_eq!(cli.jobs, 8);
        assert!(cli.clean);
        assert!(cli.dry_run);
        assert!(cli.json);
        assert!(cli.verbose);
        assert_eq!(cli.target.as_deref(), Some("all"));
    }

    #[test]
    fn test_zero_jobs_rejected() {
        let args: Vec<String> = vec!["--jobs", "0"].into_iter().map(String::from).collect();
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_version_flag() {
        let args: Vec<String> = vec!["--version"].into_iter().map(String::from).collect();
        match parse_args(&args).unwrap() {
            ParseOutcome::Info(msg) => assert!(msg.starts_with("minibuild ")),
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn test_help_flag() {
        let args: Vec<String> = vec!["--help"].into_iter().map(String::from).collect();
        match parse_args(&args).unwrap() {
            ParseOutcome::Info(msg) => {
                assert!(msg.contains("Usage: minibuild [OPTIONS] [TARGET]"));
                // README.md CLI Usage table is the spec for these strings.
                assert!(msg.contains("--file, -f <FILE>   Build file path (default: Buildfile)"));
                assert!(msg.contains(
                    "--jobs, -j <N>      Max parallel jobs (default: number of CPU cores)"
                ));
                assert!(msg
                    .contains("--clean             Remove the build cache and rebuild everything"));
                assert!(msg.contains(
                    "--dry-run, -n       Print what would be executed without running anything"
                ));
                assert!(msg.contains(
                    "--json              Write dry-run plan as JSON to stdout (requires --dry-run)"
                ));
                assert!(msg.contains("--verbose, -v       Show detailed execution info"));
                assert!(msg.contains("--version, -V       Show version"));
                assert!(msg.contains("--help, -h          Show help"));
            }
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn test_help_short_flag() {
        let args: Vec<String> = vec!["-h"].into_iter().map(String::from).collect();
        match parse_args(&args).unwrap() {
            ParseOutcome::Info(msg) => assert!(msg.contains("Usage:")),
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn test_unknown_flag() {
        let args: Vec<String> = vec!["--unknown"].into_iter().map(String::from).collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown flag"));
    }

    #[test]
    fn test_unknown_flag_suggests_jobs() {
        let args: Vec<String> = vec!["--job"].into_iter().map(String::from).collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unknown flag"), "got: {err}");
        assert!(err.contains("did you mean `--jobs`"), "got: {err}");
    }

    #[test]
    fn test_glued_short_jobs() {
        let args: Vec<String> = vec!["-j4"].into_iter().map(String::from).collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("use `-j 4` or `--jobs 4`"), "got: {err}");
    }

    #[test]
    fn test_glued_long_jobs() {
        let args: Vec<String> = vec!["--jobs=4"].into_iter().map(String::from).collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("use `-j 4` or `--jobs 4`"), "got: {err}");
    }

    #[test]
    fn test_file_missing_value() {
        let args: Vec<String> = vec!["--file"].into_iter().map(String::from).collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn test_jobs_invalid_value() {
        let args: Vec<String> = vec!["--jobs", "abc"]
            .into_iter()
            .map(String::from)
            .collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("invalid --jobs value"));
    }

    #[test]
    fn test_duplicate_target() {
        let args: Vec<String> = vec!["target1", "target2"]
            .into_iter()
            .map(String::from)
            .collect();
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn test_json_flag() {
        for args in [vec!["--dry-run", "--json"], vec!["--json", "--dry-run"]] {
            let args: Vec<String> = args.into_iter().map(String::from).collect();
            let cli = unwrap_run(parse_args(&args));
            assert!(cli.json);
            assert!(cli.dry_run);
        }
    }

    #[test]
    fn test_json_requires_dry_run() {
        let args: Vec<String> = vec!["--json".into()];
        let err = parse_args(&args).unwrap_err();
        assert!(err.contains("--json requires --dry-run"), "got: {err}");
    }

    #[test]
    fn test_short_flags() {
        let args: Vec<String> = vec!["-f", "my.build", "-j", "2", "-n", "-v"]
            .into_iter()
            .map(String::from)
            .collect();
        let cli = unwrap_run(parse_args(&args));
        assert_eq!(cli.file, "my.build");
        assert_eq!(cli.jobs, 2);
        assert!(cli.dry_run);
        assert!(cli.verbose);
    }
}
