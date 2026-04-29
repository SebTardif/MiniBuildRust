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

pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
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
            "--help" | "-h" => {
                return Err(usage());
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
    Ok(cli)
}

fn usage() -> String {
    "Usage: minibuild [OPTIONS] [TARGET]\n\n\
     Options:\n  \
       --file, -f <FILE>   Build file (default: Buildfile)\n  \
       --jobs, -j <N>      Parallel jobs (default: num CPUs)\n  \
       --clean             Remove cache and rebuild all\n  \
       --dry-run, -n       Print what would be executed\n  \
       --verbose, -v       Verbose output\n  \
       --help, -h          Show this help"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let cli = parse_args(&[]).unwrap();
        assert_eq!(cli.file, "Buildfile");
        assert!(cli.jobs >= 1);
        assert!(cli.target.is_none());
    }

    #[test]
    fn test_all_flags() {
        let args: Vec<String> = vec![
            "--file", "build.mb", "--jobs", "8", "--clean", "--dry-run", "--verbose", "all",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let cli = parse_args(&args).unwrap();
        assert_eq!(cli.file, "build.mb");
        assert_eq!(cli.jobs, 8);
        assert!(cli.clean);
        assert!(cli.dry_run);
        assert!(cli.verbose);
        assert_eq!(cli.target.as_deref(), Some("all"));
    }

    #[test]
    fn test_zero_jobs_rejected() {
        let args: Vec<String> = vec!["--jobs", "0"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(parse_args(&args).is_err());
    }

    #[test]
    fn test_short_flags() {
        let args: Vec<String> = vec!["-f", "my.build", "-j", "2", "-n", "-v"]
            .into_iter()
            .map(String::from)
            .collect();
        let cli = parse_args(&args).unwrap();
        assert_eq!(cli.file, "my.build");
        assert_eq!(cli.jobs, 2);
        assert!(cli.dry_run);
        assert!(cli.verbose);
    }
}