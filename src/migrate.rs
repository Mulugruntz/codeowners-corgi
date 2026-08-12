use std::collections::BTreeMap;

use crate::{
    codeowners::{Entry, EntryKind, Item, Manifest, Rule, matches_pattern},
    error::Result,
    repo::{RepoContext, manifest_display_path},
    sync::run as sync_run,
};

pub fn run(repo: &RepoContext) -> Result<i32> {
    let packages = repo.package_roots()?;
    if packages.is_empty() {
        return Ok(0);
    }
    let managed_files = repo.managed_files(&packages)?;
    let mut changed = false;

    for package in &packages {
        let raw_text = repo.read_text(&package.manifest_path)?;
        let (local_text, generated, suffix) = if package.is_github {
            let sections = repo.split_github_sections(&raw_text)?;
            (sections.prefix, sections.generated, sections.suffix)
        } else {
            (raw_text, None, String::new())
        };

        let manifest = Manifest::parse(&local_text)?;
        if !manifest.has_patterns() {
            continue;
        }

        let Some(files) = managed_files.get(&package.root) else {
            continue;
        };
        let mut rule_items = Vec::new();
        let mut exact_template = BTreeMap::<String, Entry>::new();
        let mut ordered_entries = Vec::new();

        for item in &manifest.items {
            match item {
                Item::Rule(rule) => rule_items.push(Item::Rule(rule.clone())),
                Item::Entry(entry) => {
                    ordered_entries.push(entry.clone());
                    match entry.kind {
                        EntryKind::Pattern => rule_items.push(Item::Rule(Rule {
                            leading: entry.leading.clone(),
                            pattern: entry.path.clone(),
                            owners: entry.owners.clone(),
                        })),
                        EntryKind::Exact => {
                            exact_template.insert(entry.path.clone(), entry.clone());
                        }
                    }
                }
            }
        }

        let mut exact_items = Vec::new();
        for file in files {
            let display_path = manifest_display_path(package, file);
            let owners = effective_owners(&ordered_entries, &display_path)?;
            let mut entry = exact_template.remove(&display_path).unwrap_or(Entry {
                leading: Vec::new(),
                path: display_path.clone(),
                owners: Vec::new(),
                kind: EntryKind::Exact,
            });
            entry.path = display_path;
            entry.owners = owners;
            entry.kind = EntryKind::Exact;
            exact_items.push(Item::Entry(entry));
        }

        let mut next_manifest = Manifest {
            header: manifest.header.clone(),
            items: [rule_items, exact_items].concat(),
            footer: manifest.footer.clone(),
        };
        next_manifest.sort_items();
        let rendered_local = next_manifest.render();
        let rendered = if package.is_github {
            let mut content = rendered_local.clone();
            if let Some(generated) = generated.as_deref() {
                if !content.is_empty() && !content.ends_with("\n\n") {
                    if !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push('\n');
                }
                content.push_str(generated);
                if !suffix.is_empty() {
                    if !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&suffix);
                }
            }
            content
        } else {
            rendered_local
        };

        changed |= repo.write_if_changed(&package.manifest_path, &rendered)?;
    }

    if changed { sync_run(repo) } else { Ok(0) }
}

fn effective_owners(entries: &[Entry], path: &str) -> Result<Vec<String>> {
    let mut owners = Vec::new();
    for entry in entries {
        let matches = match entry.kind {
            EntryKind::Exact => entry.path == path,
            EntryKind::Pattern => matches_pattern(&entry.path, path)?,
        };
        if matches {
            owners = entry.owners.clone();
        }
    }
    Ok(owners)
}
