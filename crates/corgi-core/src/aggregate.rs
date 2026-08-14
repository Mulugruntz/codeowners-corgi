use crate::{
    codeowners::{EntryKind, Item, Manifest},
    error::{CorgiError, Result},
    git::repo_relative_string,
    repo::{
        GENERATED_BEGIN, GENERATED_END, GITHUB_CODEOWNERS, RepoContext,
        repo_relative_from_manifest_path,
    },
};

pub fn run(repo: &RepoContext) -> Result<i32> {
    let packages = repo.package_roots()?;
    let mut generated_entries = Vec::new();
    let github_path = camino::Utf8Path::new(GITHUB_CODEOWNERS);

    for package in &packages {
        if package.is_github {
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

    let existing = match repo.read_text(github_path) {
        Ok(text) => text,
        Err(CorgiError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let sections = repo.split_github_sections(&existing)?;
    let content = crate::sync::combine_github_sections(
        &sections.prefix,
        generated_body.as_deref(),
        &sections.suffix,
    );

    let changed = repo.write_if_changed(github_path, &content)?;
    Ok(if changed { 1 } else { 0 })
}
