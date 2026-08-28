use std::{collections::BTreeMap, path::PathBuf, process::Command};

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::{CorgiError, Result};

pub fn repo_relative_string(path: &Utf8Path) -> String {
    let mut rendered = String::from("/");
    for (index, component) in path.iter().enumerate() {
        if index > 0 {
            rendered.push('/');
        }
        rendered.push_str(component);
    }
    rendered
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

    parse_status_porcelain(&output.stdout)
}

/// Parse NUL-separated `git status --porcelain=v1 -z` output into a rename map.
///
/// Entries with an `R` or `C` status indicator consume two NUL-separated path
/// fields (the source and destination). The returned map is keyed by the *new*
/// path with the *old* path as the value.
fn parse_status_porcelain(raw: &[u8]) -> Result<BTreeMap<Utf8PathBuf, Utf8PathBuf>> {
    let mut map = BTreeMap::new();
    let mut parts = raw.split(|byte| *byte == 0).peekable();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn porcelain(records: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, r) in records.iter().enumerate() {
            out.extend_from_slice(r);
            if i + 1 < records.len() {
                out.push(0);
            }
        }
        // git status -z terminates each record with NUL
        out.push(0);
        out
    }

    #[test]
    fn parse_empty_output() {
        let map = parse_status_porcelain(b"").unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn parse_ordinary_file_no_rename() {
        let raw = porcelain(&[b"?? new.txt"]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn parse_staged_rename() {
        // "R  old.txt" NUL "new.txt"
        let raw = porcelain(&[b"R  old.txt", b"new.txt"]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(
            map.get(&Utf8PathBuf::from("new.txt")),
            Some(&Utf8PathBuf::from("old.txt"))
        );
    }

    #[test]
    fn parse_unstaged_rename() {
        let raw = porcelain(&[b" R old.txt", b"new.txt"]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(
            map.get(&Utf8PathBuf::from("new.txt")),
            Some(&Utf8PathBuf::from("old.txt"))
        );
    }

    #[test]
    fn parse_rename_with_spaces() {
        let raw = porcelain(&[b"R  old file.txt", b"new file.txt"]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(
            map.get(&Utf8PathBuf::from("new file.txt")),
            Some(&Utf8PathBuf::from("old file.txt"))
        );
    }

    #[test]
    fn parse_rename_with_unicode() {
        let raw = porcelain(&["R  über.rs".as_bytes(), "café.rs".as_bytes()]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(
            map.get(&Utf8PathBuf::from("café.rs")),
            Some(&Utf8PathBuf::from("über.rs"))
        );
    }

    #[test]
    fn parse_copy_status() {
        let raw = porcelain(&[b"C  src.txt", b"copy.txt"]);
        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(
            map.get(&Utf8PathBuf::from("copy.txt")),
            Some(&Utf8PathBuf::from("src.txt"))
        );
    }

    #[test]
    fn parse_missing_second_rename_path() {
        // A rename record with no following NUL-separated second path.
        // Raw bytes: just the record bytes, no trailing NUL after it.
        let raw = b"R  old.txt";
        let err = parse_status_porcelain(raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing renamed path"), "got: {msg}");
    }

    #[test]
    fn parse_malformed_short_record() {
        let raw = porcelain(&[b"XY"]);
        let err = parse_status_porcelain(&raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("malformed"), "got: {msg}");
    }

    #[test]
    fn parse_invalid_utf8() {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"?? ");
        raw.extend_from_slice(&[0xff, 0xfe]);
        raw.push(0);
        let err = parse_status_porcelain(&raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("UTF-8"), "got: {msg}");
    }

    #[test]
    fn parse_multiple_entries() {
        let mut raw = Vec::new();
        // modified file
        raw.extend_from_slice(b"M  keep.txt");
        raw.push(0);
        // rename
        raw.extend_from_slice(b"R  old.rs");
        raw.push(0);
        raw.extend_from_slice(b"new.rs");
        raw.push(0);
        // another untracked
        raw.extend_from_slice(b"?? other.txt");
        raw.push(0);

        let map = parse_status_porcelain(&raw).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&Utf8PathBuf::from("new.rs")),
            Some(&Utf8PathBuf::from("old.rs"))
        );
    }
}
