use std::{collections::BTreeMap, fs, path::Path};

use camino::{Utf8Path, Utf8PathBuf};
use git2::Repository;
use ignore::WalkBuilder;
use tempfile::NamedTempFile;

use crate::{
    error::{CorgiError, Result},
    git::repo_relative_string,
};

pub const CODEOWNERS_NAME: &str = "CODEOWNERS";
pub const GITHUB_CODEOWNERS: &str = ".github/CODEOWNERS";
pub const GENERATED_BEGIN: &str = "# BEGIN CORGI GENERATED";
pub const GENERATED_END: &str = "# END CORGI GENERATED";

pub struct RepoContext {
    root: Utf8PathBuf,
    _git: Repository,
}

#[derive(Clone, Debug)]
pub struct PackageInfo {
    pub root: Utf8PathBuf,
    pub manifest_path: Utf8PathBuf,
    pub is_github: bool,
}

#[derive(Clone, Debug)]
pub struct GithubSections {
    pub prefix: String,
    pub generated: Option<String>,
    pub suffix: String,
}

impl RepoContext {
    pub fn discover(start: &Path) -> Result<Self> {
        let git = Repository::discover(start)?;
        let Some(workdir) = git.workdir() else {
            return Err(CorgiError::Message(
                "bare repositories are not supported".into(),
            ));
        };
        let root = utf8_path(workdir)?;
        Ok(Self { root, _git: git })
    }

    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    pub fn package_roots(&self) -> Result<Vec<PackageInfo>> {
        let mut packages = Vec::new();
        let mut has_root_package = false;
        for file in self.walk_files()? {
            if file.file_name() == Some(CODEOWNERS_NAME) {
                let root = file.parent().map(Utf8Path::to_path_buf).unwrap_or_default();
                if root.as_str().is_empty() {
                    has_root_package = true;
                }
                packages.push(PackageInfo {
                    manifest_path: file,
                    is_github: root.as_str() == ".github",
                    root,
                });
            }
        }
        // GitHub reads `.github/CODEOWNERS` for the whole repository. When it is
        // the only CODEOWNERS in the repo (no root `/CODEOWNERS`), promote its
        // package root to the repository root so `sync` covers every managed
        // file. When both files exist, keep the .github package scoped to
        // `.github/` and let the root package own the rest.
        if !has_root_package {
            for package in packages.iter_mut() {
                if package.is_github {
                    package.root = Utf8PathBuf::new();
                }
            }
        }
        packages.sort_by(|left, right| left.root.as_str().cmp(right.root.as_str()));
        Ok(packages)
    }

    pub fn managed_files(
        &self,
        packages: &[PackageInfo],
    ) -> Result<BTreeMap<Utf8PathBuf, Vec<Utf8PathBuf>>> {
        let mut by_package = packages
            .iter()
            .map(|package| (package.root.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let files = self.walk_files()?;
        for file in files {
            let Some(best_root) = packages
                .iter()
                .filter(|package| path_has_prefix(&file, &package.root))
                .max_by_key(|package| package.root.as_str().len())
                .map(|package| package.root.clone())
            else {
                continue;
            };

            by_package.entry(best_root).or_default().push(file);
        }
        for files in by_package.values_mut() {
            files.sort();
        }

        Ok(by_package)
    }

    pub fn read_text(&self, repo_relative: &Utf8Path) -> Result<String> {
        Ok(fs::read_to_string(self.root.join(repo_relative))?)
    }

    pub fn write_if_changed(&self, repo_relative: &Utf8Path, content: &str) -> Result<bool> {
        let path = self.root.join(repo_relative);
        let existing = fs::read_to_string(&path).ok();
        if existing.as_deref() == Some(content) {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            let mut temp = NamedTempFile::new_in(parent)?;
            use std::io::Write;
            temp.write_all(content.as_bytes())?;
            temp.flush()?;
            temp.persist(&path).map_err(|error| error.error)?;
        } else {
            fs::write(&path, content)?;
        }

        Ok(true)
    }

    pub fn split_github_sections(&self, text: &str) -> Result<GithubSections> {
        split_github_sections(text)
    }

    fn walk_files(&self) -> Result<Vec<Utf8PathBuf>> {
        let mut builder = WalkBuilder::new(&self.root);
        builder.hidden(false);
        builder.git_ignore(true);
        // Machine-global Git ignores must not affect CORGI output; only
        // repository-local ignore sources produce deterministic results.
        builder.git_global(false);
        builder.git_exclude(true);
        let mut files = Vec::new();
        for entry in builder.build() {
            let entry = entry.map_err(|error| CorgiError::Message(error.to_string()))?;
            if !entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let absolute = utf8_path(entry.path())?;
            let relative = absolute
                .strip_prefix(&self.root)
                .map_err(|_| CorgiError::Message("failed to strip repository prefix".into()))?
                .to_path_buf();
            if relative.as_str() == ".git" || relative.starts_with(".git/") {
                continue;
            }
            files.push(relative);
        }

        files.sort();
        Ok(files)
    }
}

/// Split `.github/CODEOWNERS` content into prefix, generated section, and suffix.
///
/// Markers must appear as entire lines (ignoring trailing whitespace / CR).
/// Substring matches embedded inside a larger line are ignored.
fn split_github_sections(text: &str) -> Result<GithubSections> {
    let mut begin_line_start: Option<usize> = None;
    let mut end_line_end: Option<usize> = None;

    let mut pos = 0;
    for line in text.lines() {
        let line_start = pos;
        // Advance pos past this line and its terminator.
        pos += line.len();
        if pos < text.len() && text.as_bytes()[pos] == b'\n' {
            pos += 1;
        } else if pos + 1 < text.len()
            && text.as_bytes()[pos] == b'\r'
            && text.as_bytes()[pos + 1] == b'\n'
        {
            pos += 2;
        }
        let line_end = pos; // one past the newline (or end of text)

        let trimmed = line.trim_end();
        if trimmed == GENERATED_BEGIN {
            if begin_line_start.is_some() {
                return Err(CorgiError::Message(format!(
                    "{} contains duplicate '{}' markers",
                    GITHUB_CODEOWNERS, GENERATED_BEGIN
                )));
            }
            begin_line_start = Some(line_start);
        } else if trimmed == GENERATED_END {
            if end_line_end.is_some() {
                return Err(CorgiError::Message(format!(
                    "{} contains duplicate '{}' markers",
                    GITHUB_CODEOWNERS, GENERATED_END
                )));
            }
            end_line_end = Some(line_end);
        }
    }

    match (begin_line_start, end_line_end) {
        (None, None) => Ok(GithubSections {
            prefix: text.to_string(),
            generated: None,
            suffix: String::new(),
        }),
        (Some(_), None) => Err(CorgiError::Message(format!(
            "{} is missing '{}'",
            GITHUB_CODEOWNERS, GENERATED_END
        ))),
        (None, Some(_)) => Err(CorgiError::Message(format!(
            "{} has '{}' without preceding '{}'",
            GITHUB_CODEOWNERS, GENERATED_END, GENERATED_BEGIN
        ))),
        (Some(begin), Some(end)) => {
            if begin >= end {
                return Err(CorgiError::Message(format!(
                    "{} has '{}' before '{}'",
                    GITHUB_CODEOWNERS, GENERATED_END, GENERATED_BEGIN
                )));
            }
            let prefix = text[..begin].to_string();
            let generated = text[begin..end].to_string();
            let suffix = text[end..].to_string();
            Ok(GithubSections {
                prefix,
                generated: Some(generated),
                suffix,
            })
        }
    }
}

pub fn manifest_display_path(package: &PackageInfo, repo_relative: &Utf8Path) -> String {
    if package.is_github {
        return repo_relative_string(repo_relative);
    }

    if package.root.as_str().is_empty() {
        return repo_relative_string(repo_relative);
    }

    let relative = repo_relative
        .strip_prefix(&package.root)
        .expect("package prefix");
    repo_relative_string(relative)
}

pub fn repo_relative_from_manifest_path(
    package: &PackageInfo,
    manifest_path: &str,
) -> Result<Utf8PathBuf> {
    let trimmed = manifest_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(CorgiError::Parse("empty CODEOWNERS path".into()));
    }

    if package.is_github || package.root.as_str().is_empty() {
        return Ok(Utf8PathBuf::from(trimmed));
    }

    Ok(package.root.join(trimmed))
}

fn path_has_prefix(path: &Utf8Path, prefix: &Utf8Path) -> bool {
    prefix.as_str().is_empty() || path == prefix || path.starts_with(prefix)
}

fn utf8_path(path: &Path) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path.to_path_buf()).map_err(CorgiError::Utf8Path)
}

impl PackageInfo {
    pub fn display_name(&self) -> String {
        if self.root.as_str().is_empty() {
            CODEOWNERS_NAME.to_string()
        } else {
            self.manifest_path.as_str().to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo_relative_string;

    // ── split_github_sections ─────────────────────────────────────

    #[test]
    fn split_no_generated_section() {
        let text = "# local content\n/.github/ci.yml @team\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, text);
        assert!(s.generated.is_none());
        assert!(s.suffix.is_empty());
    }

    #[test]
    fn split_valid_generated_section() {
        let text = "# local\n\n# BEGIN CORGI GENERATED\n/file @team\n# END CORGI GENERATED\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, "# local\n\n");
        assert_eq!(
            s.generated.as_deref(),
            Some("# BEGIN CORGI GENERATED\n/file @team\n# END CORGI GENERATED\n")
        );
        assert!(s.suffix.is_empty());
    }

    #[test]
    fn split_content_after_generated_section() {
        let text = "# prefix\n# BEGIN CORGI GENERATED\n/x @a\n# END CORGI GENERATED\n# suffix\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, "# prefix\n");
        assert!(s.generated.is_some());
        assert_eq!(s.suffix, "# suffix\n");
    }

    #[test]
    fn split_empty_generated_section() {
        let text = "# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, "");
        assert_eq!(
            s.generated.as_deref(),
            Some("# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n")
        );
    }

    #[test]
    fn split_missing_end_marker() {
        let text = "# BEGIN CORGI GENERATED\n/file @team\n";
        let err = split_github_sections(text).unwrap_err();
        assert!(err.to_string().contains("missing"), "got: {err}");
    }

    #[test]
    fn split_missing_begin_marker() {
        let text = "/file @team\n# END CORGI GENERATED\n";
        let err = split_github_sections(text).unwrap_err();
        assert!(err.to_string().contains("without preceding"), "got: {err}");
    }

    #[test]
    fn split_end_before_begin() {
        let text = "# END CORGI GENERATED\n# BEGIN CORGI GENERATED\n";
        let err = split_github_sections(text).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("without preceding") || msg.contains("before"),
            "got: {msg}"
        );
    }

    #[test]
    fn split_duplicate_begin_markers() {
        let text = "# BEGIN CORGI GENERATED\n# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n";
        let err = split_github_sections(text).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {err}");
    }

    #[test]
    fn split_duplicate_end_markers() {
        let text = "# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n# END CORGI GENERATED\n";
        let err = split_github_sections(text).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {err}");
    }

    #[test]
    fn split_marker_embedded_mid_line_is_ignored() {
        // A line containing the marker text but not as the entire line
        // must not be treated as a marker.
        let text = "# see # BEGIN CORGI GENERATED for details\n/file @team\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, text);
        assert!(s.generated.is_none());
    }

    #[test]
    fn split_empty_file() {
        let s = split_github_sections("").unwrap();
        assert_eq!(s.prefix, "");
        assert!(s.generated.is_none());
    }

    #[test]
    fn split_no_trailing_newline_after_end() {
        let text = "# BEGIN CORGI GENERATED\n/file @team\n# END CORGI GENERATED";
        let s = split_github_sections(text).unwrap();
        assert!(s.generated.is_some());
        assert!(s.suffix.is_empty());
    }

    #[test]
    fn split_crlf_markers() {
        let text =
            "# prefix\r\n# BEGIN CORGI GENERATED\r\n/x @a\r\n# END CORGI GENERATED\r\n# suffix\r\n";
        let s = split_github_sections(text).unwrap();
        assert_eq!(s.prefix, "# prefix\r\n");
        assert!(s.generated.is_some());
        assert_eq!(s.suffix, "# suffix\r\n");
    }

    // ── path_has_prefix ──────────────────────────────────────────

    #[test]
    fn path_has_prefix_empty_matches_all() {
        assert!(path_has_prefix(
            Utf8Path::new("src/lib.rs"),
            Utf8Path::new("")
        ));
    }

    #[test]
    fn path_has_prefix_exact() {
        assert!(path_has_prefix(
            Utf8Path::new("packages/api"),
            Utf8Path::new("packages/api")
        ));
    }

    #[test]
    fn path_has_prefix_nested() {
        assert!(path_has_prefix(
            Utf8Path::new("packages/api/src/lib.rs"),
            Utf8Path::new("packages/api")
        ));
    }

    #[test]
    fn path_has_prefix_no_match() {
        assert!(!path_has_prefix(
            Utf8Path::new("src/lib.rs"),
            Utf8Path::new("packages/api")
        ));
    }

    // ── manifest_display_path ────────────────────────────────────

    #[test]
    fn display_path_root_package() {
        let package = PackageInfo {
            root: Utf8PathBuf::new(),
            manifest_path: Utf8PathBuf::from("CODEOWNERS"),
            is_github: false,
        };
        let result = manifest_display_path(&package, Utf8Path::new("src/lib.rs"));
        assert_eq!(result, "/src/lib.rs");
    }

    #[test]
    fn display_path_nested_package() {
        let package = PackageInfo {
            root: Utf8PathBuf::from("packages/api"),
            manifest_path: Utf8PathBuf::from("packages/api/CODEOWNERS"),
            is_github: false,
        };
        let result = manifest_display_path(&package, Utf8Path::new("packages/api/src/lib.rs"));
        assert_eq!(result, "/src/lib.rs");
    }

    #[test]
    fn display_path_github_package() {
        let package = PackageInfo {
            root: Utf8PathBuf::from(".github"),
            manifest_path: Utf8PathBuf::from(".github/CODEOWNERS"),
            is_github: true,
        };
        let result = manifest_display_path(&package, Utf8Path::new(".github/workflows/ci.yml"));
        assert_eq!(result, "/.github/workflows/ci.yml");
    }

    // ── repo_relative_from_manifest_path ─────────────────────────

    #[test]
    fn repo_relative_root_package() {
        let package = PackageInfo {
            root: Utf8PathBuf::new(),
            manifest_path: Utf8PathBuf::from("CODEOWNERS"),
            is_github: false,
        };
        let result = repo_relative_from_manifest_path(&package, "/src/lib.rs").unwrap();
        assert_eq!(result, Utf8PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn repo_relative_nested_package() {
        let package = PackageInfo {
            root: Utf8PathBuf::from("packages/api"),
            manifest_path: Utf8PathBuf::from("packages/api/CODEOWNERS"),
            is_github: false,
        };
        let result = repo_relative_from_manifest_path(&package, "/src/lib.rs").unwrap();
        assert_eq!(result, Utf8PathBuf::from("packages/api/src/lib.rs"));
    }

    #[test]
    fn repo_relative_empty_path_fails() {
        let package = PackageInfo {
            root: Utf8PathBuf::new(),
            manifest_path: Utf8PathBuf::from("CODEOWNERS"),
            is_github: false,
        };
        assert!(repo_relative_from_manifest_path(&package, "/").is_err());
    }

    // ── repo_relative_string ─────────────────────────────────────

    #[test]
    fn repo_relative_string_simple() {
        assert_eq!(
            repo_relative_string(Utf8Path::new("src/lib.rs")),
            "/src/lib.rs"
        );
    }

    #[test]
    fn repo_relative_string_single_component() {
        assert_eq!(
            repo_relative_string(Utf8Path::new("README.md")),
            "/README.md"
        );
    }

    // ── PackageInfo::display_name ────────────────────────────────

    #[test]
    fn display_name_root() {
        let p = PackageInfo {
            root: Utf8PathBuf::new(),
            manifest_path: Utf8PathBuf::from("CODEOWNERS"),
            is_github: false,
        };
        assert_eq!(p.display_name(), "CODEOWNERS");
    }

    #[test]
    fn display_name_nested() {
        let p = PackageInfo {
            root: Utf8PathBuf::from("packages/api"),
            manifest_path: Utf8PathBuf::from("packages/api/CODEOWNERS"),
            is_github: false,
        };
        assert_eq!(p.display_name(), "packages/api/CODEOWNERS");
    }
}
