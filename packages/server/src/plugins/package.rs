//! Plugin package download, validation, and extraction.

use anyhow::Context;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use semver::Version;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tar::Archive;
use uuid::Uuid;

use super::registry::{Manifest, PluginTools, StorePlugin};

const MAX_PACKAGE_BYTES: usize = 50 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 200 * 1024 * 1024;
const MAX_FILES: usize = 64;

/// A validated, installed plugin package with manifest, skill, and tool definitions.
pub struct InstalledPackage {
    /// Parsed plugin manifest.
    pub manifest: Manifest,
    /// Full skill instructions loaded from `skill.md`.
    pub skill_md: String,
    /// Tool definitions exported by the plugin.
    pub tools: PluginTools,
    /// Filesystem path to the installed version directory.
    pub version_path: PathBuf,
}

/// Loads a previously installed plugin package from disk.
pub fn load_installed_package(version_path: &Path) -> anyhow::Result<InstalledPackage> {
    let manifest: Manifest =
        toml::from_str(&fs::read_to_string(version_path.join("manifest.toml"))?)
            .context("invalid installed plugin manifest")?;
    let skill_md = fs::read_to_string(version_path.join("skill.md"))?;
    let tools: PluginTools = serde_json::from_slice(&fs::read(version_path.join("tools.json"))?)
        .context("invalid installed plugin tools.json")?;
    validate_tools(&tools)?;
    Ok(InstalledPackage {
        manifest,
        skill_md,
        tools,
        version_path: version_path.to_path_buf(),
    })
}

/// Downloads, verifies, and extracts a plugin package from the store registry.
pub async fn install_store_package(
    data_dir: &Path,
    user_id: &str,
    plugin: &StorePlugin,
) -> anyhow::Result<InstalledPackage> {
    validate_segment(user_id, "user ID")?;
    validate_segment(&plugin.id, "plugin ID")?;
    validate_segment(&plugin.version, "plugin version")?;
    let package = plugin
        .package
        .as_ref()
        .context("this registry entry has no installable package")?;
    let bytes = download(&package.url, package.size).await?;
    verify_checksum(&bytes, &package.sha256)?;

    let data_dir = data_dir.to_path_buf();
    let user_id = user_id.to_string();
    let expected_id = plugin.id.clone();
    let expected_name = plugin.name.clone();
    let expected_version = plugin.version.clone();
    let expected_tier = plugin.tier.clone();
    let expected_permissions = plugin.permissions.clone();
    let expected_allowed_hosts = plugin.allowed_hosts.clone();
    tokio::task::spawn_blocking(move || {
        extract_and_validate(
            &data_dir,
            &user_id,
            &expected_id,
            &expected_name,
            &expected_version,
            &expected_tier,
            &expected_permissions,
            &expected_allowed_hosts,
            &bytes,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("plugin package task failed: {error}"))?
}

/// Returns the filesystem root for a user's installed plugin.
pub fn plugin_root(data_dir: &Path, user_id: &str, plugin_id: &str) -> PathBuf {
    data_dir
        .join("users")
        .join(user_id)
        .join("plugins")
        .join(plugin_id)
}

async fn download(url: &str, expected_size: Option<u64>) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to inspect plugin package {path}"))?;
        if metadata.len() > MAX_PACKAGE_BYTES as u64 {
            anyhow::bail!("plugin package exceeds the 50 MiB compressed limit");
        }
        let bytes =
            fs::read(path).with_context(|| format!("failed to read plugin package {path}"))?;
        validate_download_size(bytes.len(), expected_size)?;
        return Ok(bytes);
    }

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download plugin package {url}"))?
        .error_for_status()
        .with_context(|| format!("plugin package {url} returned an error"))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PACKAGE_BYTES as u64)
    {
        anyhow::bail!("plugin package exceeds the 50 MiB compressed limit");
    }

    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed while downloading plugin package")?;
        if bytes.len() + chunk.len() > MAX_PACKAGE_BYTES {
            anyhow::bail!("plugin package exceeds the 50 MiB compressed limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    validate_download_size(bytes.len(), expected_size)?;
    Ok(bytes)
}

fn validate_download_size(actual: usize, expected: Option<u64>) -> anyhow::Result<()> {
    if let Some(expected) = expected
        && actual as u64 != expected
    {
        anyhow::bail!("plugin package size mismatch: expected {expected} bytes, received {actual}");
    }
    Ok(())
}

fn verify_checksum(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.len() != 64
        || !expected
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        anyhow::bail!("plugin package SHA-256 must be 64 hexadecimal characters");
    }
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        anyhow::bail!("plugin package checksum mismatch");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn extract_and_validate(
    data_dir: &Path,
    user_id: &str,
    expected_id: &str,
    expected_name: &str,
    expected_version: &str,
    expected_tier: &str,
    expected_permissions: &[String],
    expected_allowed_hosts: &[String],
    bytes: &[u8],
) -> anyhow::Result<InstalledPackage> {
    let root = plugin_root(data_dir, user_id, expected_id);
    let versions = root.join("versions");
    fs::create_dir_all(&versions)?;
    set_directory_permissions(&root)?;
    set_directory_permissions(&versions)?;

    let staging = root.join(format!(".install-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    set_directory_permissions(&staging)?;

    let result = (|| {
        let decoder = GzDecoder::new(bytes);
        let mut archive = Archive::new(decoder);
        let mut seen = HashSet::new();
        let mut total_size = 0_u64;
        let mut file_count = 0_usize;

        for entry in archive.entries().context("invalid plugin tar archive")? {
            let mut entry = entry.context("invalid plugin archive entry")?;
            let entry_type = entry.header().entry_type();
            if !entry_type.is_file() {
                anyhow::bail!("plugin packages may contain regular files only");
            }

            let path = entry.path().context("invalid plugin archive path")?;
            let name = single_file_name(&path)?;
            if !matches!(
                name.as_str(),
                "manifest.toml" | "skill.md" | "tools.json" | "plugin.wasm"
            ) {
                anyhow::bail!("unexpected file in plugin package: {name}");
            }
            if !seen.insert(name.clone()) {
                anyhow::bail!("duplicate file in plugin package: {name}");
            }

            file_count += 1;
            if file_count > MAX_FILES {
                anyhow::bail!("plugin package contains too many files");
            }
            total_size = total_size.saturating_add(entry.size());
            if total_size > MAX_UNPACKED_BYTES {
                anyhow::bail!("plugin package exceeds the 200 MiB unpacked limit");
            }

            let destination = staging.join(name);
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)?;
            std::io::copy(&mut entry, &mut output)?;
            output.flush()?;
            output.sync_all()?;
            set_file_permissions(&destination)?;
        }

        for required in ["manifest.toml", "skill.md", "tools.json"] {
            if !seen.contains(required) {
                anyhow::bail!("plugin package is missing {required}");
            }
        }

        let manifest_raw = fs::read_to_string(staging.join("manifest.toml"))?;
        let manifest: Manifest =
            toml::from_str(&manifest_raw).context("invalid plugin manifest")?;
        validate_manifest(
            &manifest,
            expected_id,
            expected_name,
            expected_version,
            expected_tier,
            expected_permissions,
            expected_allowed_hosts,
        )?;
        if manifest.tier == "wasm" && !seen.contains("plugin.wasm") {
            anyhow::bail!("WASM plugin package is missing plugin.wasm");
        }

        let skill_md = fs::read_to_string(staging.join("skill.md"))?;
        let tools: PluginTools = serde_json::from_slice(&fs::read(staging.join("tools.json"))?)
            .context("invalid plugin tools.json")?;
        validate_tools(&tools)?;

        let version_path = versions.join(expected_version);
        if version_path.exists() {
            anyhow::bail!("plugin version {expected_version} is already present");
        }
        fs::rename(&staging, &version_path)?;
        sync_directory(&versions)?;

        Ok(InstalledPackage {
            manifest,
            skill_md,
            tools,
            version_path,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn validate_manifest(
    manifest: &Manifest,
    expected_id: &str,
    expected_name: &str,
    expected_version: &str,
    expected_tier: &str,
    expected_permissions: &[String],
    expected_allowed_hosts: &[String],
) -> anyhow::Result<()> {
    if manifest.id != expected_id {
        anyhow::bail!("package manifest ID does not match the registry");
    }
    if manifest.version != expected_version {
        anyhow::bail!("package manifest version does not match the registry");
    }
    if manifest.name != expected_name {
        anyhow::bail!("package manifest name does not match the registry");
    }
    if manifest.tier != expected_tier || !matches!(manifest.tier.as_str(), "wasm" | "bridge") {
        anyhow::bail!("package manifest tier does not match the registry");
    }
    let mut manifest_permissions = manifest.permissions.clone();
    let mut registry_permissions = expected_permissions.to_vec();
    manifest_permissions.sort();
    registry_permissions.sort();
    if manifest_permissions != registry_permissions {
        anyhow::bail!("package manifest permissions do not match the registry");
    }
    // Only enforce allowed_hosts when the registry explicitly declares them.
    // Existing registry entries without the field (empty) skip this check for
    // backwards compatibility; new entries that list hosts are validated strictly.
    if !expected_allowed_hosts.is_empty() {
        let mut manifest_hosts = manifest.allowed_hosts.clone();
        let mut registry_hosts = expected_allowed_hosts.to_vec();
        manifest_hosts.sort();
        registry_hosts.sort();
        if manifest_hosts != registry_hosts {
            anyhow::bail!("package manifest allowed_hosts do not match the registry");
        }
    }
    if let Some(minimum) = manifest.min_core_version.as_deref() {
        let minimum = Version::parse(minimum).context("invalid min_core_version")?;
        let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
        if current < minimum {
            anyhow::bail!("plugin requires helpcore {minimum} or newer");
        }
    }
    let brief = manifest.brief.trim();
    if brief.is_empty() {
        anyhow::bail!(
            "package manifest for '{}' is missing a `brief` field. \
             Add: brief = \"Use when the user asks to...\"",
            manifest.id
        );
    }
    if brief.len() > 200 {
        anyhow::bail!("package manifest brief must be 200 characters or fewer");
    }
    if brief.contains('\n') || brief.contains('\r') {
        anyhow::bail!("package manifest brief must be a single line");
    }
    Ok(())
}

fn validate_tools(tools: &PluginTools) -> anyhow::Result<()> {
    let mut names = HashSet::new();
    for tool in &tools.tools {
        if tool.name.trim().is_empty() {
            anyhow::bail!("plugin tool name cannot be empty");
        }
        if !names.insert(tool.name.as_str()) {
            anyhow::bail!("plugin tool names must be unique");
        }
        if !tool.input_schema.is_object() {
            anyhow::bail!("plugin tool input_schema must be a JSON object");
        }
    }
    Ok(())
}

fn validate_segment(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        anyhow::bail!("{label} is not safe for filesystem storage");
    }
    Ok(())
}

fn single_file_name(path: &Path) -> anyhow::Result<String> {
    let mut components = path.components();
    let Some(Component::Normal(name)) = components.next() else {
        anyhow::bail!("plugin package contains an unsafe path");
    };
    if components.next().is_some() {
        anyhow::bail!("plugin package files must be at the archive root");
    }
    Ok(name.to_string_lossy().into_owned())
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_directory_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn sync_directory(path: &Path) -> anyhow::Result<()> {
    let directory = fs::File::open(path)?;
    directory.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Cursor;
    use tar::Builder;

    fn package_bytes() -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut archive = Builder::new(encoder);
        let files = [
            (
                "manifest.toml",
                br#"id = "test-plugin"
name = "Test Plugin"
version = "1.0.0"
description = "Test"
tier = "bridge"
permissions = ["outbound_http"]
allowed_hosts = ["api.example.com"]
brief = "Use when testing package installation."
"#
                .as_slice(),
            ),
            ("skill.md", b"Use the test plugin.".as_slice()),
            (
                "tools.json",
                br#"{"tools":[{"name":"test","description":"Test","input_schema":{"type":"object"}}]}"#
                    .as_slice(),
            ),
        ];
        for (name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o600);
            header.set_cksum();
            archive
                .append_data(&mut header, name, Cursor::new(contents))
                .unwrap();
        }
        archive.finish().unwrap();
        archive.into_inner().unwrap().finish().unwrap()
    }

    fn store_plugin(url: String, sha256: String) -> StorePlugin {
        StorePlugin {
            id: "test-plugin".into(),
            name: "Test Plugin".into(),
            description: "Test".into(),
            brief: Some("Use when testing package installation.".into()),
            version: "1.0.0".into(),
            tier: "bridge".into(),
            author: "Test".into(),
            homepage: "https://example.com".into(),
            permissions: vec!["outbound_http".into()],
            allowed_hosts: Vec::new(),
            provides: Vec::new(),
            source: Some(super::super::registry::StorePluginSource {
                source_type: "url".into(),
                repo: None,
                source_ref: None,
                wasm_asset: None,
                url: None,
            }),
            setup_guide: None,
            package: Some(super::super::registry::StorePluginPackage {
                url,
                sha256,
                size: None,
            }),
        }
    }

    #[test]
    fn rejects_unsafe_segments() {
        assert!(validate_segment("../other", "plugin ID").is_err());
        assert!(validate_segment("plugin-name", "plugin ID").is_ok());
    }

    #[test]
    fn rejects_nested_archive_paths() {
        assert!(single_file_name(Path::new("../manifest.toml")).is_err());
        assert!(single_file_name(Path::new("folder/manifest.toml")).is_err());
        assert_eq!(
            single_file_name(Path::new("manifest.toml")).unwrap(),
            "manifest.toml"
        );
    }

    #[tokio::test]
    async fn verifies_and_installs_package_per_user() {
        let directory = tempfile::tempdir().unwrap();
        let archive_path = directory.path().join("plugin.tar.gz");
        let bytes = package_bytes();
        fs::write(&archive_path, &bytes).unwrap();
        let plugin = store_plugin(
            format!("file://{}", archive_path.display()),
            hex::encode(Sha256::digest(&bytes)),
        );
        let installed = install_store_package(directory.path(), "user-a", &plugin)
            .await
            .unwrap();
        assert_eq!(installed.manifest.id, "test-plugin");
        assert!(
            installed.version_path.starts_with(
                directory
                    .path()
                    .join("users/user-a/plugins/test-plugin/versions")
            )
        );
    }

    #[tokio::test]
    async fn rejects_checksum_mismatch_before_installing() {
        let directory = tempfile::tempdir().unwrap();
        let archive_path = directory.path().join("plugin.tar.gz");
        fs::write(&archive_path, package_bytes()).unwrap();
        let plugin = store_plugin(format!("file://{}", archive_path.display()), "0".repeat(64));
        assert!(
            install_store_package(directory.path(), "user-a", &plugin)
                .await
                .is_err()
        );
        assert!(!plugin_root(directory.path(), "user-a", "test-plugin").exists());
    }
}
