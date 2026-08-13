use std::{collections::BTreeMap, path::PathBuf, process::Command};

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{CorgiError, Result};

pub fn repo_relative_string(path: &Utf8Path) -> String {
    format!("/{}", path.as_str())
}

pub fn rename_map(repo_root: &Utf8Path) -> Result<BTreeMap<Utf8PathBuf, Utf8PathBuf>> {
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .arg("--find-renames")
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        return Err(CorgiError::Message(format!(
            "git status failed with status {}",
            output.status
        )));
    }

    let mut map = BTreeMap::new();
    let mut parts = output.stdout.split(|byte| *byte == 0).peekable();
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }

        if record.len() < 4 {
            return Err(CorgiError::Parse("malformed git status output".into()));
        }

        let status = &record[..2];
        let first_path = bytes_to_utf8(&record[3..])?;
        if matches!(status[0], b'R' | b'C') || matches!(status[1], b'R' | b'C') {
            let Some(second_path) = parts.next() else {
                return Err(CorgiError::Parse(
                    "missing renamed path in git status output".into(),
                ));
            };
            let second_path = bytes_to_utf8(second_path)?;
            map.insert(
                Utf8PathBuf::from(second_path),
                Utf8PathBuf::from(first_path),
            );
        }
    }

    Ok(map)
}

fn bytes_to_utf8(bytes: &[u8]) -> Result<String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        CorgiError::Utf8Path(PathBuf::from(String::from_utf8_lossy(bytes).into_owned()))
    })
}
