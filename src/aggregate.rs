use crate::{
    codeowners::{EntryKind, Item, Manifest},
    error::{CorgiError, Result},
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
                let mut line = format!("/{}", repo_relative.as_str());
                for owner in entry.owners {
                    line.push(' ');
                    line.push_str(&owner);
                }
                generated_entries.push(line);
            }
        }
    }

    generated_entries.sort();
    let generated_body = if generated_entries.is_empty() {
        format!("{GENERATED_BEGIN}\n{GENERATED_END}\n")
    } else {
        format!(
            "{GENERATED_BEGIN}\n{}\n{GENERATED_END}\n",
            generated_entries.join("\n")
        )
    };

    let existing = repo.read_text(github_path).unwrap_or_default();
    let sections = repo.split_github_sections(&existing)?;
    let mut content = sections.prefix;
    if !content.is_empty() && !content.ends_with("\n\n") {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
    }
    content.push_str(&generated_body);
    if !sections.suffix.is_empty() {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&sections.suffix);
    }

    let changed = repo.write_if_changed(github_path, &content)?;
    Ok(if changed { 1 } else { 0 })
}
