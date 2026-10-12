use std::fs;
#[cfg(windows)]
use std::io::Cursor;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};
use ureq::Agent;

/// Target triple this crate was compiled for; release assets are named after it.
pub const TARGET: &str = env!("XMTR_TARGET");

const DEFAULT_RELEASES: &str = "https://github.com/Alice39s/xmtr/releases";
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Release page base URL; `XMTR_RELEASES` points the installer and `xmtr self update` at a mirror.
pub fn releases_url() -> String {
    std::env::var("XMTR_RELEASES")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map_or_else(
            || DEFAULT_RELEASES.to_owned(),
            |value| value.trim().trim_end_matches('/').to_owned(),
        )
}

pub const fn exe_name() -> &'static str {
    if cfg!(windows) { "xmtr.exe" } else { "xmtr" }
}

pub fn archive_name() -> String {
    if cfg!(windows) {
        format!("xmtr-{TARGET}.zip")
    } else {
        format!("xmtr-{TARGET}.tar.gz")
    }
}

pub fn agent(user_agent: &str) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .user_agent(user_agent)
        .build()
        .into()
}

/// Tag of the newest release, read from the redirect of `<releases>/latest`.
pub fn latest_tag(agent: &Agent) -> Result<String> {
    let url = format!("{}/latest", releases_url());
    let response = agent
        .get(&url)
        .config()
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .call()
        .with_context(|| format!("requesting {url}"))?;
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            anyhow!(
                "{url} answered HTTP {} without a redirect to a release",
                response.status().as_u16()
            )
        })?;
    let tag = location
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|tag| !tag.is_empty() && *tag != "releases")
        .ok_or_else(|| anyhow!("{url} redirected to {location}, which names no release"))?;
    Ok(tag.to_owned())
}

pub fn is_newer(tag: &str, current: &str) -> Result<bool> {
    let parse = |value: &str| {
        semver::Version::parse(value.trim_start_matches('v'))
            .with_context(|| format!("{value} is not a version"))
    };
    Ok(parse(tag)? > parse(current)?)
}

fn download(agent: &Agent, url: &str) -> Result<Vec<u8>> {
    agent
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?
        .body_mut()
        .with_config()
        .limit(MAX_ASSET_BYTES)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))
}

/// SHA-256 recorded for `file` in a `sha256sum` listing.
fn expected_sha256<'a>(sums: &'a str, file: &str) -> Result<&'a str> {
    sums.lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            (name.trim_start().trim_start_matches('*') == file).then_some(hash)
        })
        .ok_or_else(|| anyhow!("SHA256SUMS has no entry for {file}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Downloads `file` from release `tag` and checks it against the release's SHA256SUMS.
pub fn fetch_verified(agent: &Agent, tag: &str, file: &str) -> Result<Vec<u8>> {
    let base = format!("{}/download/{tag}", releases_url());
    let sums = String::from_utf8(download(agent, &format!("{base}/SHA256SUMS"))?)
        .context("SHA256SUMS is not UTF-8")?;
    let expected = expected_sha256(&sums, file)?.to_ascii_lowercase();
    let bytes = download(agent, &format!("{base}/{file}"))?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        bail!("{file} does not match SHA256SUMS: expected {expected}, got {actual}");
    }
    Ok(bytes)
}

/// The xmtr executable inside a release archive.
#[cfg(windows)]
pub fn extract_binary(archive: &[u8]) -> Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).context("opening the zip archive")?;
    let mut entry = zip
        .by_name(exe_name())
        .with_context(|| format!("the archive has no {}", exe_name()))?;
    let mut binary = Vec::new();
    entry.read_to_end(&mut binary)?;
    Ok(binary)
}

#[cfg(not(windows))]
pub fn extract_binary(archive: &[u8]) -> Result<Vec<u8>> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries().context("reading the tar archive")? {
        let mut entry = entry?;
        if entry
            .path()?
            .file_name()
            .is_some_and(|name| name == exe_name())
        {
            let mut binary = Vec::new();
            entry.read_to_end(&mut binary)?;
            return Ok(binary);
        }
    }
    bail!("the archive has no {}", exe_name())
}

/// Downloads, verifies and unpacks the xmtr binary of release `tag` for this target.
pub fn fetch_binary(agent: &Agent, tag: &str) -> Result<Vec<u8>> {
    extract_binary(&fetch_verified(agent, tag, &archive_name())?)
}

/// Writes `bytes` to a new executable file next to `destination` and returns its path.
pub fn write_staged(destination: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let staged = destination.with_extension("new");
    fs::write(&staged, bytes).with_context(|| format!("writing {}", staged.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("marking {} executable", staged.display()))?;
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sha256sum_listings() {
        let sums = "abc  xmtr-x86_64-unknown-linux-musl.tar.gz\ndef *xmtr-installer-x86_64-pc-windows-gnu.exe\n";
        assert_eq!(
            expected_sha256(sums, "xmtr-x86_64-unknown-linux-musl.tar.gz").unwrap(),
            "abc"
        );
        assert_eq!(
            expected_sha256(sums, "xmtr-installer-x86_64-pc-windows-gnu.exe").unwrap(),
            "def"
        );
        assert!(expected_sha256(sums, "xmtr-aarch64-apple-darwin.tar.gz").is_err());
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.1", "0.2.0").unwrap());
        assert!(!is_newer("v0.2.0", "0.2.0").unwrap());
        assert!(!is_newer("v0.1.9", "0.2.0").unwrap());
    }

    #[test]
    fn hashes_like_sha256sum() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
