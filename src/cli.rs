use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub file: String,
    pub jobs: usize,
    pub target: Option<String>,
    pub clean: bool,
    pub dry_run: bool,
    pub verbose: bool,
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
                return Err(format!("unknown flag: {s}"));
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
    Ok(ParseOutcome::Run(cli))
}

fn usage() -> String {
    "Usage: minibuild [OPTIONS] [TARGET]\n\n\
     Options:\n  \
       --file, -f <FILE>   Build file (default: Buildfile)\n  \
       --jobs, -j <N>      Parallel jobs (default: num CPUs)\n  \
       --clean             Remove cache and rebuild all\n  \
       --dry-run, -n       Print what would be executed\n  \
       --verbose, -v       Verbose output\n  \
       --version, -V       Show version\n  \
       --help, -h          Show this help"
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
            ParseOutcome::Info(msg) => assert!(msg.contains("Usage:")),
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
