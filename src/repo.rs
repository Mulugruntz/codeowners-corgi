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
        let Some(begin_idx) = text.find(GENERATED_BEGIN) else {
            return Ok(GithubSections {
                prefix: text.to_string(),
                generated: None,
                suffix: String::new(),
            });
        };
        // Search for the end marker only after the begin marker so that a
        // matching literal in the local section (for example inside a
        // comment) can't be mistaken for the real end.
        let search_from = begin_idx + GENERATED_BEGIN.len();
        let Some(end_offset) = text[search_from..].find(GENERATED_END) else {
            return Err(CorgiError::Message(format!(
                "{} is missing '{}'",
                GITHUB_CODEOWNERS, GENERATED_END
            )));
        };
        let end_idx = search_from + end_offset;

        let prefix = text[..begin_idx].to_string();
        let end_line_end = text[end_idx..]
            .find('\n')
            .map(|offset| end_idx + offset + 1)
            .unwrap_or(text.len());
        let generated = text[begin_idx..end_line_end].to_string();
        let suffix = text[end_line_end..].to_string();
        Ok(GithubSections {
            prefix,
            generated: Some(generated),
            suffix,
        })
    }

    fn walk_files(&self) -> Result<Vec<Utf8PathBuf>> {
        let mut builder = WalkBuilder::new(&self.root);
        builder.hidden(false);
        builder.git_ignore(true);
        builder.git_global(true);
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
