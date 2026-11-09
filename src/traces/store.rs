//! The list of traces uploaded from this machine: one JSON record per line in `xmtr/traces.jsonl`
//! under the user's data directory, oldest first. On Unix only its owner can read the file.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::UploadAccepted;

/// One uploaded trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// What the gateway returned: the trace ID, the links and the admin key.
    #[serde(flatten)]
    pub trace: UploadAccepted,
    /// When the upload was accepted, in RFC 3339, such as `2026-11-09T04:12:33Z`.
    pub uploaded_at: String,
    /// The gateway the trace was uploaded to.
    pub api: String,
    /// The targets as given on the command line.
    pub targets: Vec<String>,
}

/// `xmtr/traces.jsonl` in the user's data directory.
pub fn path() -> Result<PathBuf> {
    let data = dirs::data_dir().context("locating the user's data directory")?;
    Ok(data.join("xmtr").join("traces.jsonl"))
}

fn directory(path: &Path) -> &Path {
    path.parent().expect("the list file is inside a directory")
}

/// Options that create a missing file, readable and writable by its owner only on Unix.
fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

fn line(record: &Record) -> String {
    let mut line = serde_json::to_string(record).expect("a record always encodes");
    line.push('\n');
    line
}

/// Adds `record` to the end of the list at `path`, creating the file and its directory.
pub fn append(path: &Path, record: &Record) -> Result<()> {
    let dir = directory(path);
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    private_file()
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(line(record).as_bytes()))
        .with_context(|| format!("writing {}", path.display()))?;
    return_to_owner(path)
}

/// The records in the list at `path`, oldest first; none when the file does not exist yet.
pub fn read(path: &Path) -> Result<Vec<Record>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("reading {} line {}", path.display(), index + 1))
        })
        .collect()
}

/// Removes trace `id` from the list at `path`; false when the list does not hold it. The list is
/// written to a new file that then replaces the old one, so an interrupted write loses nothing.
pub fn remove(path: &Path, id: &str) -> Result<bool> {
    let records = read(path)?;
    let kept: Vec<&Record> = records
        .iter()
        .filter(|record| record.trace.id != id)
        .collect();
    if kept.len() == records.len() {
        return Ok(false);
    }
    let text: String = kept.into_iter().map(line).collect();
    let dir = directory(path);
    let mut file = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a new list in {}", dir.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", file.path().display()))?;
    file.persist(path)
        .with_context(|| format!("replacing {}", path.display()))?;
    return_to_owner(path)?;
    Ok(true)
}

/// `sudo` keeps `HOME` on macOS, so a run as root writes the list into the data directory of the
/// user who started it. The list and its directory are then given to the owner of the data
/// directory, so the user's runs without `sudo` can still read and extend them.
#[cfg(unix)]
fn return_to_owner(path: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, chown};

    let dir = directory(path);
    let Some(data) = dir.parent() else {
        return Ok(());
    };
    let owner = fs::metadata(data).with_context(|| format!("reading {}", data.display()))?;
    let written = fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if written.uid() != 0 || owner.uid() == 0 {
        return Ok(());
    }
    for target in [dir, path] {
        chown(target, Some(owner.uid()), Some(owner.gid())).with_context(|| {
            format!(
                "giving {} to the owner of {}",
                target.display(),
                data.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn return_to_owner(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::traces::tests::{ID, accepted};

    /// A directory inside the ignored `target` directory, removed when the value is dropped.
    pub fn scratch() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-scratch");
        fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new()
            .prefix("traces-")
            .tempdir_in(base)
            .unwrap()
    }

    pub fn record(id: &str) -> Record {
        Record {
            trace: UploadAccepted {
                id: id.to_owned(),
                url: format!("https://xmsl.dev/xmtr/{id}"),
                ..accepted()
            },
            uploaded_at: "2026-11-09T04:12:33Z".to_owned(),
            api: "https://stage-info-api.xmsl.dev".to_owned(),
            targets: vec!["1.1.1.1".to_owned(), "cloudflare.com".to_owned()],
        }
    }

    #[test]
    fn appends_records_and_reads_them_back_in_order() {
        let dir = scratch();
        let path = dir.path().join("xmtr").join("traces.jsonl");
        assert_eq!(read(&path).unwrap(), []);
        append(&path, &record(ID)).unwrap();
        append(&path, &record("2-b")).unwrap();
        assert_eq!(read(&path).unwrap(), [record(ID), record("2-b")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn writes_one_json_object_per_line_with_the_documented_fields() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        append(&path, &record(ID)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut fields: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "adminApiUrl",
                "adminKey",
                "adminUrl",
                "api",
                "id",
                "pin",
                "targets",
                "uploadedAt",
                "url"
            ]
        );
    }

    #[test]
    fn removes_a_trace_and_keeps_the_others() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        for id in ["1-a", ID, "3-c"] {
            append(&path, &record(id)).unwrap();
        }
        assert!(remove(&path, ID).unwrap());
        assert_eq!(read(&path).unwrap(), [record("1-a"), record("3-c")]);
        assert!(!remove(&path, ID).unwrap());
        assert_eq!(read(&path).unwrap(), [record("1-a"), record("3-c")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn names_the_line_it_cannot_read() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        append(&path, &record(ID)).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"\n{not json\n")
            .unwrap();
        let error = format!("{:#}", read(&path).unwrap_err());
        assert!(error.contains("traces.jsonl line 3"), "{error}");
    }

    #[test]
    fn reads_records_without_admin_fields() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        fs::write(
            &path,
            r#"{"id":"1-a","pin":false,"url":"https://xmsl.dev/xmtr/1-a","uploadedAt":"2026-11-09T04:12:33Z","api":"https://stage-info-api.xmsl.dev","targets":["1.1.1.1"]}"#,
        )
        .unwrap();
        let records = read(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].trace.admin_key, None);
    }
}
