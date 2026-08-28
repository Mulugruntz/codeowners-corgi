use std::collections::{BTreeMap, BTreeSet};

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    codeowners::{Entry, EntryKind, Item, Manifest, Rule, select_auto_rule},
    error::{CorgiError, Result},
    git::rename_map,
    repo::{
        PackageInfo, RepoContext, manifest_display_path, path_has_prefix,
        repo_relative_from_manifest_path,
    },
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

    // Phase 2: compute all outputs before writing any files so that a
    // validation or computation failure cannot leave a partial write.
    let mut used_existing = BTreeSet::new();
    let mut unowned = false;
    let mut unowned_by_manifest = BTreeMap::<String, Vec<String>>::new();
    let mut planned_writes = Vec::new();

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
            } else if let Some((old_path, mut preserved)) = find_renamed_entry(
                file,
                &renames,
                &existing_entries,
                &used_existing,
                &package.root,
                &packages,
            )? {
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

        planned_writes.push((package.manifest_path.clone(), rendered));
    }

    // Phase 3: write all computed outputs.
    let mut changed = false;
    for (path, content) in &planned_writes {
        changed |= repo.write_if_changed(path, content)?;
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
    current_root: &Utf8Path,
    packages: &[PackageInfo],
) -> Result<Option<(Utf8PathBuf, Entry)>> {
    for (old_path, renamed_path) in renames {
        if renamed_path != new_path || used_existing.contains(old_path) {
            continue;
        }
        // Only preserve ownership for renames within the same package.
        // Cross-package renames must use the destination package's rules
        // instead of leaking the source package's ownership.
        let source_root = packages
            .iter()
            .filter(|p| path_has_prefix(old_path, &p.root))
            .max_by_key(|p| p.root.as_str().len())
            .map(|p| p.root.as_path());
        if source_root != Some(current_root) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_local_only() {
        let result = combine_github_sections("/file @team\n", None, "");
        assert_eq!(result, "/file @team\n");
    }

    #[test]
    fn combine_local_and_generated() {
        let result = combine_github_sections(
            "/file @team\n",
            Some("# BEGIN CORGI GENERATED\n/x @a\n# END CORGI GENERATED\n"),
            "",
        );
        assert_eq!(
            result,
            "/file @team\n\n# BEGIN CORGI GENERATED\n/x @a\n# END CORGI GENERATED\n"
        );
    }

    #[test]
    fn combine_local_generated_and_suffix() {
        let result = combine_github_sections(
            "# local\n",
            Some("# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n"),
            "# suffix\n",
        );
        assert!(result.contains("# local"));
        assert!(result.contains("# BEGIN CORGI GENERATED"));
        assert!(result.contains("# suffix"));
    }

    #[test]
    fn combine_empty_local_with_generated() {
        let result = combine_github_sections(
            "",
            Some("# BEGIN CORGI GENERATED\n/x @a\n# END CORGI GENERATED\n"),
            "",
        );
        assert_eq!(
            result,
            "# BEGIN CORGI GENERATED\n/x @a\n# END CORGI GENERATED\n"
        );
    }

    #[test]
    fn combine_preserves_suffix() {
        let result = combine_github_sections(
            "",
            Some("# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n"),
            "# important suffix\n",
        );
        assert!(result.ends_with("# important suffix\n"));
    }

    #[test]
    fn combine_no_generated_strips_trailing_blank_lines() {
        let result = combine_github_sections("/file @team\n\n", None, "");
        assert_eq!(result, "/file @team\n");
    }

    #[test]
    fn combine_deterministic() {
        let a = combine_github_sections(
            "# local\n",
            Some("# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n"),
            "",
        );
        let b = combine_github_sections(
            "# local\n",
            Some("# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n"),
            "",
        );
        assert_eq!(a, b);
    }
}
