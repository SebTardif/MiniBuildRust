use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::SystemTime;

const CACHE_FILE: &str = ".minibuild_cache";

/// Per-rule cache entry: stores the last successful build signature.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    /// Sorted list of (path, modified_timestamp_nanos) for input files.
    pub input_hashes: Vec<(String, u128)>,
    /// Sorted list of (path, modified_timestamp_nanos) for output files.
    pub output_hashes: Vec<(String, u128)>,
}

/// The build cache: maps rule names to their last-known file signatures.
#[derive(Debug, Clone)]
pub struct BuildCache {
    pub entries: HashMap<String, CacheEntry>,
}

impl BuildCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load cache from disk. Returns empty cache if file doesn't exist.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(CACHE_FILE);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::new(),
        };
        Self::deserialize(&content)
    }

    /// Save cache to disk.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        let path = dir.join(CACHE_FILE);
        let data = self.serialize();
        let mut f = fs::File::create(&path)?;
        f.write_all(data.as_bytes())?;
        Ok(())
    }

    /// Delete the cache file.
    pub fn clean(dir: &Path) {
        let path = dir.join(CACHE_FILE);
        let _ = fs::remove_file(&path);
    }

    /// Check if a rule needs rebuilding.
    pub fn is_up_to_date(&self, rule_name: &str, inputs: &[String], outputs: &[String]) -> bool {
        let entry = match self.entries.get(rule_name) {
            Some(e) => e,
            None => return false,
        };

        let current_inputs = compute_signatures(inputs);
        let current_outputs = compute_signatures(outputs);

        entry.input_hashes == current_inputs && entry.output_hashes == current_outputs
    }

    /// Record a successful build for this rule.
    pub fn record(&mut self, rule_name: &str, inputs: &[String], outputs: &[String]) {
        let entry = CacheEntry {
            input_hashes: compute_signatures(inputs),
            output_hashes: compute_signatures(outputs),
        };
        self.entries.insert(rule_name.to_string(), entry);
    }

    /// Mark a rule as needing rebuild (invalidate).
    #[allow(dead_code)]
    pub fn invalidate(&mut self, rule_name: &str) {
        self.entries.remove(rule_name);
    }

    fn serialize(&self) -> String {
        let mut out = String::new();
        let mut keys: Vec<&String> = self.entries.keys().collect();
        keys.sort();
        for key in keys {
            let entry = &self.entries[key];
            out.push_str(&format!("RULE {}\n", key));
            for (path, ts) in &entry.input_hashes {
                out.push_str(&format!("  IN {} {}\n", ts, path));
            }
            for (path, ts) in &entry.output_hashes {
                out.push_str(&format!("  OUT {} {}\n", ts, path));
            }
        }
        out
    }

    fn deserialize(content: &str) -> Self {
        let mut entries = HashMap::new();
        let mut current_rule: Option<String> = None;
        let mut current_inputs: Vec<(String, u128)> = Vec::new();
        let mut current_outputs: Vec<(String, u128)> = Vec::new();

        let reader = io::Cursor::new(content);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let trimmed = line.trim();

            if let Some(rest) = trimmed.strip_prefix("RULE ") {
                // Finalize previous rule
                if let Some(name) = current_rule.take() {
                    entries.insert(
                        name,
                        CacheEntry {
                            input_hashes: std::mem::take(&mut current_inputs),
                            output_hashes: std::mem::take(&mut current_outputs),
                        },
                    );
                }
                current_rule = Some(rest.to_string());
            } else if let Some(rest) = trimmed.strip_prefix("IN ") {
                if let Some((ts_str, path)) = rest.split_once(' ') {
                    if let Ok(ts) = ts_str.parse::<u128>() {
                        current_inputs.push((path.to_string(), ts));
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("OUT ") {
                if let Some((ts_str, path)) = rest.split_once(' ') {
                    if let Ok(ts) = ts_str.parse::<u128>() {
                        current_outputs.push((path.to_string(), ts));
                    }
                }
            }
        }

        // Finalize last rule
        if let Some(name) = current_rule {
            entries.insert(
                name,
                CacheEntry {
                    input_hashes: current_inputs,
                    output_hashes: current_outputs,
                },
            );
        }

        Self { entries }
    }
}

/// Compute signatures for a list of file paths. Returns sorted (path, mtime_nanos).
fn compute_signatures(paths: &[String]) -> Vec<(String, u128)> {
    let mut sigs: Vec<(String, u128)> = paths
        .iter()
        .filter_map(|p| {
            let meta = fs::metadata(p).ok()?;
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some((p.clone(), mtime))
        })
        .collect();
    sigs.sort();
    sigs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_cache_roundtrip() {
        let dir = std::env::temp_dir().join("minibuild_test_cache_rt");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut cache = BuildCache::new();
        cache.entries.insert(
            "compile".to_string(),
            CacheEntry {
                input_hashes: vec![("src/main.c".to_string(), 12345)],
                output_hashes: vec![("build/main.o".to_string(), 67890)],
            },
        );

        cache.save(&dir).unwrap();

        let loaded = BuildCache::load(&dir);
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries["compile"], cache.entries["compile"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_load_missing() {
        let dir = std::env::temp_dir().join("minibuild_test_cache_missing");
        let cache = BuildCache::load(&dir);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn test_is_up_to_date() {
        let dir = std::env::temp_dir().join("minibuild_test_uptodate");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let input_file = dir.join("input.txt");
        let output_file = dir.join("output.txt");
        fs::write(&input_file, "hello").unwrap();
        fs::write(&output_file, "world").unwrap();

        let inputs = vec![input_file.to_string_lossy().to_string()];
        let outputs = vec![output_file.to_string_lossy().to_string()];

        let mut cache = BuildCache::new();
        assert!(!cache.is_up_to_date("test", &inputs, &outputs));

        cache.record("test", &inputs, &outputs);
        assert!(cache.is_up_to_date("test", &inputs, &outputs));

        cache.invalidate("test");
        assert!(!cache.is_up_to_date("test", &inputs, &outputs));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clean() {
        let dir = std::env::temp_dir().join("minibuild_test_clean");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut cache = BuildCache::new();
        cache.entries.insert(
            "test".to_string(),
            CacheEntry {
                input_hashes: vec![],
                output_hashes: vec![],
            },
        );
        cache.save(&dir).unwrap();
        assert!(dir.join(CACHE_FILE).exists());

        BuildCache::clean(&dir);
        assert!(!dir.join(CACHE_FILE).exists());

        let _ = fs::remove_dir_all(&dir);
    }
}