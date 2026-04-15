use std::path::{Path, PathBuf};

use anyhow::Result;
use globset::{Glob, GlobMatcher};
use walkdir::WalkDir;

use crate::config::PathsConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
}

pub fn scan_repo(repo: &Path, paths: &PathsConfig) -> Result<Vec<ScannedFile>> {
    let deny_matchers = build_matchers(&paths.deny)?;
    let vendor_matchers = build_matchers(&paths.allow_vendor_paths)?;
    let mut results = Vec::new();

    for allow in &paths.allow {
        let root = repo.join(allow);
        if !root.exists() {
            continue;
        }
        if root.is_file() {
            let rel = root.strip_prefix(repo).unwrap().to_path_buf();
            if is_allowed_file(&rel, &deny_matchers, paths, &vendor_matchers) {
                results.push(ScannedFile {
                    absolute_path: root,
                    relative_path: rel,
                });
            }
            continue;
        }

        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let rel = entry.path().strip_prefix(repo).unwrap().to_path_buf();
            if is_allowed_file(&rel, &deny_matchers, paths, &vendor_matchers) {
                results.push(ScannedFile {
                    absolute_path: entry.path().to_path_buf(),
                    relative_path: rel,
                });
            }
        }
    }

    if paths.allow_vendor {
        let vendor_root = repo.join("vendor");
        if vendor_root.exists() {
            for entry in WalkDir::new(&vendor_root)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                let rel = entry.path().strip_prefix(repo).unwrap().to_path_buf();
                if is_allowed_file(&rel, &deny_matchers, paths, &vendor_matchers) {
                    results.push(ScannedFile {
                        absolute_path: entry.path().to_path_buf(),
                        relative_path: rel,
                    });
                }
            }
        }
    }

    results.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    results.dedup_by(|left, right| left.relative_path == right.relative_path);
    Ok(results)
}

fn build_matchers(globs: &[String]) -> Result<Vec<GlobMatcher>> {
    globs
        .iter()
        .map(|glob| Ok(Glob::new(glob)?.compile_matcher()))
        .collect()
}

fn is_allowed_file(
    relative: &Path,
    deny_matchers: &[GlobMatcher],
    paths: &PathsConfig,
    vendor_matchers: &[GlobMatcher],
) -> bool {
    let rel = relative.to_string_lossy();
    if deny_matchers.iter().any(|matcher| matcher.is_match(&*rel)) {
        return false;
    }

    let is_vendor = rel.starts_with("vendor/");
    if is_vendor {
        if !paths.allow_vendor {
            return false;
        }
        if !vendor_matchers.iter().any(|matcher| {
            matcher.is_match(&*rel)
                || relative
                    .ancestors()
                    .any(|ancestor| matcher.is_match(ancestor.to_string_lossy().as_ref()))
        }) {
            return false;
        }
    }

    rel.ends_with(".php")
        || matches!(
            rel.as_ref(),
            "composer.json" | "composer.lock" | "phpunit.xml" | "pest.php"
        )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::config::IndexerConfig;

    use super::scan_repo;

    #[test]
    fn scans_allowlisted_php_and_blocks_denied_files() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app")).unwrap();
        fs::create_dir_all(dir.path().join("storage")).unwrap();
        fs::create_dir_all(dir.path().join("routes")).unwrap();
        fs::write(dir.path().join("app/Service.php"), "<?php class Service {}").unwrap();
        fs::write(dir.path().join("routes/web.php"), "<?php").unwrap();
        fs::write(dir.path().join("storage/secret.php"), "<?php").unwrap();
        fs::write(dir.path().join(".env"), "DB_PASSWORD=secret").unwrap();
        fs::write(dir.path().join("dump.csv"), "bad").unwrap();

        let scanned = scan_repo(dir.path(), &IndexerConfig::default().paths).unwrap();
        let files: Vec<_> = scanned
            .into_iter()
            .map(|entry| entry.relative_path.to_string_lossy().into_owned())
            .collect();

        assert_eq!(files, vec!["app/Service.php", "routes/web.php"]);
    }

    #[test]
    fn vendor_paths_respect_flag_and_glob() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("vendor/acme/package/src")).unwrap();
        fs::create_dir_all(dir.path().join("vendor/acme/package/tests")).unwrap();
        fs::write(
            dir.path().join("vendor/acme/package/src/Thing.php"),
            "<?php class Thing {}",
        )
        .unwrap();
        fs::write(
            dir.path().join("vendor/acme/package/tests/ThingTest.php"),
            "<?php",
        )
        .unwrap();

        let config = IndexerConfig::default();
        let scanned = scan_repo(dir.path(), &config.paths).unwrap();
        let files: Vec<_> = scanned
            .into_iter()
            .map(|entry| entry.relative_path.to_string_lossy().into_owned())
            .collect();

        assert_eq!(files, vec!["vendor/acme/package/src/Thing.php"]);
    }
}
