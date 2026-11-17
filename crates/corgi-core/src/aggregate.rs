use camino::Utf8Path;

use crate::{
    codeowners::{EntryKind, Item, Manifest},
    error::{CorgiError, Result},
    git::repo_relative_string,
    repo::{
        GENERATED_BEGIN, GENERATED_END, Platform, RepoContext, repo_relative_from_manifest_path,
    },
};

pub fn run(
    repo: &RepoContext,
    output_path: Option<&str>,
    platform: Option<Platform>,
) -> Result<i32> {
    let packages = repo.package_roots()?;
    let mut generated_entries = Vec::new();

    for package in &packages {
        // Skip platform-specific aggregate output directories. `.github/CODEOWNERS`
        // is the aggregate output for GitHub, `.gitlab/CODEOWNERS` for GitLab,
        // `.bitbucket/CODEOWNERS` for Bitbucket. Only root and subdirectory
        // manifests are processed as source inputs.
        if matches!(package.root.as_str(), ".github" | ".gitlab" | ".bitbucket") {
            continue;
        }
        let manifest = Manifest::parse(&repo.read_text(&package.manifest_path)?)?;
        for item in manifest.items {
            if let Item::Entry(entry) = item {
                if entry.kind != EntryKind::Exact {
                    return Err(CorgiError::Message(format!(
                        "{} contains wildcard ownership entries; run `corgi migrate` first",
                        package.display_name()
                    )));
                }
                let repo_relative = repo_relative_from_manifest_path(package, &entry.path)?;
                let mut line = repo_relative_string(&repo_relative);
                for owner in entry.owners {
                    line.push(' ');
                    line.push_str(&owner);
                }
                generated_entries.push(line);
            }
        }
    }

    generated_entries.sort();
    let generated_body = (!generated_entries.is_empty()).then(|| {
        format!(
            "{GENERATED_BEGIN}\n{}\n{GENERATED_END}\n",
            generated_entries.join("\n")
        )
    });

    // Resolve the platform: use the caller-supplied value, fall back to
    // auto-detection when the CLI did not pass an explicit platform.
    let platform = platform.unwrap_or_else(|| repo.detect_platform());

    let output_path = output_path.unwrap_or_else(|| platform.default_codeowners_path());
    let output = Utf8Path::new(output_path);

    // Merge semantics are tied to the *platform*, not the output path.
    // GitHub uses prefix/suffix merging; other platforms overwrite directly.
    let content = if platform == Platform::GitHub {
        let existing = match repo.read_text(output) {
            Ok(text) => text,
            Err(CorgiError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                String::new()
            }
            Err(error) => return Err(error),
        };
        let sections = repo.split_github_sections(&existing)?;
        crate::sync::combine_github_sections(
            &sections.prefix,
            generated_body.as_deref(),
            &sections.suffix,
        )
    } else {
        generated_body.unwrap_or_else(String::new)
    };

    let changed = repo.write_if_changed(output, &content)?;
    Ok(if changed { 1 } else { 0 })
}
