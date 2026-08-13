use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use crate::{
    codeowners::{Entry, EntryKind, Item, Manifest, Rule, select_auto_rule},
    error::{CorgiError, Result},
    git::rename_map,
    repo::{RepoContext, manifest_display_path, repo_relative_from_manifest_path},
};

pub fn run(repo: &RepoContext) -> Result<i32> {
    let packages = repo.package_roots()?;
    if packages.is_empty() {
        return Ok(0);
    }

    let managed_files = repo.managed_files(&packages)?;
    let renames = rename_map(repo.root())?;

    let mut parsed = BTreeMap::new();
    let mut existing_entries = BTreeMap::<Utf8PathBuf, Entry>::new();

    for package in &packages {
        let text = repo.read_text(&package.manifest_path)?;
        let manifest = if package.is_github {
            let sections = repo.split_github_sections(&text)?;
            Manifest::parse(&sections.prefix)?
        } else {
            Manifest::parse(&text)?
        };

        if manifest.has_patterns() {
            return Err(CorgiError::Message(format!(
                "{} contains wildcard ownership entries; run `corgi migrate` first",
                package.display_name()
            )));
        }

        for entry in manifest.exact_entries() {
            let repo_relative = repo_relative_from_manifest_path(package, &entry.path)?;
            existing_entries.insert(repo_relative, entry.clone());
        }
        parsed.insert(package.root.clone(), manifest);
    }

    let mut used_existing = BTreeSet::new();
    let mut changed = false;
    let mut unowned = false;
    let mut unowned_by_manifest = BTreeMap::<String, Vec<String>>::new();

    for package in &packages {
        let Some(files) = managed_files.get(&package.root) else {
            continue;
        };
        let original = parsed.get(&package.root).expect("parsed manifest");
        let manifest_name = package.display_name();
        let rules = original.rules().cloned().collect::<Vec<Rule>>();
        let rule_items = original
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rule(rule) => Some(Item::Rule(rule.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut new_items = Vec::new();
        new_items.extend(rule_items);

        for file in files {
            let display_path = manifest_display_path(package, file);
            let entry = if let Some(existing) = existing_entries.get(file) {
                used_existing.insert(file.clone());
                let mut preserved = existing.clone();
                preserved.path = display_path.clone();
                preserved
            } else if let Some((old_path, mut preserved)) =
                find_renamed_entry(file, &renames, &existing_entries, &used_existing)?
            {
                used_existing.insert(old_path);
                preserved.path = display_path.clone();
                preserved
            } else {
                let owners = select_auto_rule(rules.iter(), &display_path)?
                    .map(|rule| rule.owners)
                    .unwrap_or_default();
                Entry {
                    leading: Vec::new(),
                    path: display_path.clone(),
                    owners,
                    kind: EntryKind::Exact,
                }
            };

            if entry.owners.is_empty() {
                unowned = true;
                unowned_by_manifest
                    .entry(manifest_name.clone())
                    .or_default()
                    .push(display_path.clone());
            }
            new_items.push(Item::Entry(entry));
        }

        let mut next_manifest = Manifest {
            header: original.header.clone(),
            items: new_items,
            footer: original.footer.clone(),
        };
        next_manifest.sort_items();

        let rendered_local = next_manifest.render();
        let rendered = if package.is_github {
            let current = repo.read_text(&package.manifest_path)?;
            let sections = repo.split_github_sections(&current)?;
            combine_github_sections(
                &rendered_local,
                sections.generated.as_deref(),
                &sections.suffix,
            )
        } else {
            rendered_local
        };

        changed |= repo.write_if_changed(&package.manifest_path, &rendered)?;
    }

    if unowned {
        print_unowned_summary(&unowned_by_manifest);
    }

    if changed || unowned { Ok(1) } else { Ok(0) }
}

fn print_unowned_summary(unowned_by_manifest: &BTreeMap<String, Vec<String>>) {
    eprintln!("unowned files remain after sync:");
    for (manifest, paths) in unowned_by_manifest {
        eprintln!("  {manifest}:");
        for path in paths {
            eprintln!("    - {path}");
        }
    }
    eprintln!("add explicit owners or matching `# Rule[auto-assign]: ...` entries");
}

fn find_renamed_entry(
    new_path: &Utf8PathBuf,
    renames: &BTreeMap<Utf8PathBuf, Utf8PathBuf>,
    existing_entries: &BTreeMap<Utf8PathBuf, Entry>,
    used_existing: &BTreeSet<Utf8PathBuf>,
) -> Result<Option<(Utf8PathBuf, Entry)>> {
    for (old_path, renamed_path) in renames {
        if renamed_path != new_path || used_existing.contains(old_path) {
            continue;
        }
        if let Some(entry) = existing_entries.get(old_path) {
            return Ok(Some((old_path.clone(), entry.clone())));
        }
    }

    Ok(None)
}

pub fn combine_github_sections(local: &str, generated: Option<&str>, suffix: &str) -> String {
    let mut content = local.to_string();
    if let Some(generated) = generated {
        if !content.is_empty() && !content.ends_with("\n\n") {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
        }
        content.push_str(generated);
    }
    if !suffix.is_empty() {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(suffix);
    }
    if generated.is_none() && suffix.is_empty() {
        while content.ends_with("\n\n") {
            content.pop();
        }
    }
    if content.is_empty() || content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    }
}
