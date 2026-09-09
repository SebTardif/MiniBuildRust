/// Return the closest candidate to `got`, or `None` if none is close enough.
/// Never returns the exact same string as `got`.
pub fn closest<'a>(got: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let mut best: Option<(&'a str, usize)> = None;
    for &cand in candidates {
        if cand == got {
            continue;
        }
        let distance = levenshtein(got, cand);
        if !close_enough(distance, got, cand) {
            continue;
        }
        match best {
            None => best = Some((cand, distance)),
            Some((_, best_d)) if distance < best_d => best = Some((cand, distance)),
            _ => {}
        }
    }
    best.map(|(name, _)| name)
}

/// Format `base`, appending a did-you-mean hint when `got` is close to a candidate.
pub fn with_hint(base: &str, got: &str, candidates: &[&str]) -> String {
    match closest(got, candidates) {
        Some(hint) => format!("{base} (did you mean `{hint}`?)"),
        None => base.to_string(),
    }
}

fn close_enough(distance: usize, got: &str, cand: &str) -> bool {
    if distance == 0 {
        return false;
    }
    let max_len = got.chars().count().max(cand.chars().count());
    distance <= 2 || distance * 3 <= max_len
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }

    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];

    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closest_typical_typo() {
        let flags = [
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
        assert_eq!(closest("--job", &flags), Some("--jobs"));
        assert_eq!(closest("--verbos", &flags), Some("--verbose"));
    }

    #[test]
    fn test_closest_rejects_exact_match() {
        assert_eq!(closest("jobs", &["jobs"]), None);
        assert_eq!(closest("jobs", &["jobs", "job"]), Some("job"));
    }

    #[test]
    fn test_closest_too_far() {
        assert_eq!(closest("zzzzzzzz", &["env", "default", "rule"]), None);
        assert_eq!(closest("ghost", &["a"]), None);
    }

    #[test]
    fn test_closest_empty() {
        assert_eq!(closest("jobs", &[]), None);
        assert_eq!(closest("", &["env"]), None);
    }

    #[test]
    fn test_with_hint() {
        assert_eq!(
            with_hint("unknown flag: --job", "--job", &["--jobs", "--help"]),
            "unknown flag: --job (did you mean `--jobs`?)"
        );
        assert_eq!(
            with_hint("unknown flag: --zzzz", "--zzzz", &["--jobs"]),
            "unknown flag: --zzzz"
        );
    }

    #[test]
    fn test_levenshtein_known_distances() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("job", "jobs"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("defualt", "default"), 2);
    }
}
