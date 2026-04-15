use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposerExport {
    pub root: ComposerPackage,
    #[serde(default)]
    pub packages: Vec<ComposerPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposerPackage {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(rename = "type", default)]
    pub package_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub is_root: bool,
}

impl ComposerExport {
    pub fn package_for_path<'a>(&'a self, absolute_path: &Path) -> &'a ComposerPackage {
        self.packages
            .iter()
            .filter_map(|package| {
                let install_path = package.install_path.as_ref()?;
                let install_path = PathBuf::from(install_path);
                if absolute_path.starts_with(&install_path) {
                    Some((install_path.components().count(), package))
                } else {
                    None
                }
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, package)| package)
            .unwrap_or(&self.root)
    }
}

pub fn export_packages(repo: &Path) -> Result<ComposerExport> {
    if let Ok(export) = export_packages_via_php(repo) {
        return Ok(export);
    }
    export_packages_via_lock(repo)
}

fn export_packages_via_php(repo: &Path) -> Result<ComposerExport> {
    let script = r#"<?php
$repo = $argv[1] ?? getcwd();
$autoload = $repo . '/vendor/autoload.php';
if (!file_exists($autoload)) {
    fwrite(STDERR, "vendor autoload missing\n");
    exit(2);
}
require $autoload;
$root = json_decode(file_get_contents($repo . '/composer.json'), true);
$packages = [];
if (class_exists('Composer\\InstalledVersions')) {
    foreach (Composer\InstalledVersions::getInstalledPackages() as $name) {
        $packages[] = [
            'name' => $name,
            'version' => Composer\InstalledVersions::getPrettyVersion($name),
            'install_path' => Composer\InstalledVersions::getInstallPath($name),
            'type' => null,
            'description' => null,
            'keywords' => [],
            'is_root' => false,
        ];
    }
}
echo json_encode([
    'root' => [
        'name' => $root['name'] ?? 'root/app',
        'version' => $root['version'] ?? null,
        'type' => $root['type'] ?? null,
        'description' => $root['description'] ?? null,
        'install_path' => $repo,
        'keywords' => $root['keywords'] ?? [],
        'is_root' => true,
    ],
    'packages' => $packages,
], JSON_UNESCAPED_SLASHES);
"#;

    let temp = tempfile::NamedTempFile::new().context("create php exporter temp file")?;
    fs::write(temp.path(), script)?;

    let output = Command::new("php")
        .arg(temp.path())
        .arg(repo)
        .output()
        .context("run embedded composer exporter")?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn export_packages_via_lock(repo: &Path) -> Result<ComposerExport> {
    #[derive(Debug, Deserialize)]
    struct ComposerJson {
        name: Option<String>,
        version: Option<String>,
        #[serde(rename = "type")]
        package_type: Option<String>,
        description: Option<String>,
        #[serde(default)]
        keywords: Vec<String>,
    }
    #[derive(Debug, Deserialize)]
    struct LockPackage {
        name: String,
        version: Option<String>,
        #[serde(rename = "type")]
        package_type: Option<String>,
        description: Option<String>,
        #[serde(default)]
        keywords: Vec<String>,
    }
    #[derive(Debug, Deserialize)]
    struct LockFile {
        #[serde(default)]
        packages: Vec<LockPackage>,
    }

    let composer_json: ComposerJson = serde_json::from_slice(
        &fs::read(repo.join("composer.json")).context("read composer.json")?,
    )?;
    let lock: LockFile = serde_json::from_slice(
        &fs::read(repo.join("composer.lock")).unwrap_or_else(|_| b"{\"packages\":[]}".to_vec()),
    )?;

    Ok(ComposerExport {
        root: ComposerPackage {
            name: composer_json.name.unwrap_or_else(|| "root/app".to_string()),
            version: composer_json.version,
            package_type: composer_json.package_type,
            description: composer_json.description,
            install_path: Some(repo.display().to_string()),
            keywords: composer_json.keywords,
            is_root: true,
        },
        packages: lock
            .packages
            .into_iter()
            .map(|package| ComposerPackage {
                install_path: Some(
                    repo.join("vendor")
                        .join(package.name.replace('/', std::path::MAIN_SEPARATOR_STR))
                        .display()
                        .to_string(),
                ),
                name: package.name,
                version: package.version,
                package_type: package.package_type,
                description: package.description,
                keywords: package.keywords,
                is_root: false,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::export_packages;

    #[test]
    fn lockfile_fallback_exports_root_and_packages() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("composer.json"),
            r#"{"name":"acme/app","description":"demo","keywords":["php","search"]}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("composer.lock"),
            r#"{"packages":[{"name":"laravel/framework","version":"11.0.0","type":"library"}]}"#,
        )
        .unwrap();

        let export = export_packages(dir.path()).unwrap();
        assert_eq!(export.root.name, "acme/app");
        assert_eq!(export.packages[0].name, "laravel/framework");
    }
}
