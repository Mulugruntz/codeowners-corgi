mod support;

use indoc::indoc;
use support::TestRepo;

// ═══════════════════════════════════════════════════════════════════
// sync — basic operations
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_adds_new_files() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# header\n");
    repo.write("README.md", "readme");

    let status = corgi_core::sync(repo.path()).unwrap();

    assert_eq!(status, 1);
    let content = repo.read("CODEOWNERS");
    assert!(
        content.contains("/CODEOWNERS"),
        "CODEOWNERS itself should be listed"
    );
    assert!(content.contains("/README.md"), "new file should be added");
}

#[test]
fn sync_removes_deleted_entries() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/deleted.txt @old/team\n");
    repo.write("keep.txt", "keep");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(!content.contains("deleted.txt"));
    assert!(content.contains("/keep.txt"));
}

#[test]
fn sync_reports_unowned_files() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "");
    repo.write("keep.txt", "keep");

    let status = corgi_core::sync(repo.path()).unwrap();

    assert_eq!(status, 1, "unowned files should produce status 1");
}

#[test]
fn sync_returns_zero_when_fully_owned_repo_is_unchanged() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/CODEOWNERS @team\n/file.txt @team\n");
    repo.write("file.txt", "content");

    let status = corgi_core::sync(repo.path()).unwrap();

    assert_eq!(status, 0, "fully owned unchanged repo should return 0");
    assert_eq!(
        repo.read("CODEOWNERS"),
        "/CODEOWNERS @team\n/file.txt @team\n"
    );
}

#[test]
fn sync_is_byte_idempotent_after_full_ownership() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("file.txt", "content");

    corgi_core::sync(repo.path()).unwrap();
    let snapshot = repo.read("CODEOWNERS");

    // Second run should not modify the file.
    let status = corgi_core::sync(repo.path()).unwrap();
    assert_eq!(repo.read("CODEOWNERS"), snapshot);
    // Still returns 0 because nothing changed and everything is owned.
    assert_eq!(status, 0);
}

#[test]
fn sync_repeated_unresolved_preserves_status_one() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "");
    repo.write("file.txt", "content");

    let status1 = corgi_core::sync(repo.path()).unwrap();
    assert_eq!(status1, 1);
    let snapshot = repo.read("CODEOWNERS");

    let status2 = corgi_core::sync(repo.path()).unwrap();
    assert_eq!(status2, 1, "unresolved state should still return 1");
    assert_eq!(
        repo.read("CODEOWNERS"),
        snapshot,
        "file should be unchanged"
    );
}

// ═══════════════════════════════════════════════════════════════════
// sync — auto-assignment rules
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_auto_assign_rule_match() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /src/** @org/backend\n");
    repo.write("src/lib.rs", "lib");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/lib.rs @org/backend"));
}

#[test]
fn sync_most_specific_rule_wins() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/root
            # Rule[auto-assign]: /src/special/** @org/special
        "},
    );
    repo.write("src/main.rs", "main");
    repo.write("src/special/feature.rs", "feature");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/main.rs @org/root"));
    assert!(content.contains("/src/special/feature.rs @org/special"));
}

#[test]
fn sync_does_not_overwrite_existing_owner() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/root
            /src/existing.rs @org/manual
        "},
    );
    repo.write("src/existing.rs", "existing");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/existing.rs @org/manual"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — renames
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_preserves_owner_on_rename_within_package() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# keep with file\n/old.txt @org/team\n");
    repo.write("old.txt", "hello");
    repo.commit("initial");
    repo.rename("old.txt", "new.txt");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/new.txt @org/team"));
    assert!(!content.contains("old.txt"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — nested package ownership
// ═══════════════════════════════════════════════════════════════════

#[test]
fn nested_package_takes_ownership_from_ancestor() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /src/** @org/root\n");
    repo.write("src/lib.rs", "lib");
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /src/** @org/api\n",
    );
    repo.write("packages/api/src/main.rs", "fn main() {}");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(root_co.contains("/src/lib.rs @org/root"));
    assert!(
        !root_co.contains("packages/api"),
        "nested files should not appear in root"
    );

    let api_co = repo.read("packages/api/CODEOWNERS");
    assert!(api_co.contains("/src/main.rs @org/api"));
}

#[test]
fn file_at_package_root_boundary() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/api\n",
    );
    repo.write("packages/api/README.md", "api readme");
    repo.write("README.md", "root readme");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(root_co.contains("/README.md @org/root"));
    assert!(!root_co.contains("packages/api/README.md"));

    let api_co = repo.read("packages/api/CODEOWNERS");
    assert!(api_co.contains("/README.md @org/api"));
}

#[test]
fn sibling_packages_independent_ownership() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/alpha/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/alpha\n",
    );
    repo.write(
        "packages/beta/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/beta\n",
    );
    repo.write("packages/alpha/lib.rs", "alpha");
    repo.write("packages/beta/lib.rs", "beta");
    repo.write("root.rs", "root");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(root_co.contains("/root.rs @org/root"));
    assert!(!root_co.contains("alpha/lib.rs"));
    assert!(!root_co.contains("beta/lib.rs"));

    let alpha_co = repo.read("packages/alpha/CODEOWNERS");
    assert!(alpha_co.contains("/lib.rs @org/alpha"));

    let beta_co = repo.read("packages/beta/CODEOWNERS");
    assert!(beta_co.contains("/lib.rs @org/beta"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — .gitignore
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_respects_root_gitignore() {
    let repo = TestRepo::new();
    repo.write(".gitignore", "ignored.log\n");
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("kept.txt", "kept");
    repo.write("ignored.log", "ignored");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/kept.txt"));
    assert!(!content.contains("ignored.log"));
}

#[test]
fn sync_respects_nested_gitignore() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("src/nested/.gitignore", "*.tmp\n");
    repo.write("src/nested/keep.rs", "keep");
    repo.write("src/nested/temp.tmp", "temp");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/nested/keep.rs"));
    assert!(!content.contains("temp.tmp"));
}

#[test]
fn sync_respects_git_info_exclude() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("kept.txt", "kept");
    repo.write("excluded.tmp", "excluded");
    // .git/info/exclude is repo-local and should be honored.
    repo.write(".git/info/exclude", "excluded.tmp\n");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/kept.txt"));
    assert!(!content.contains("excluded.tmp"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — filesystem edge cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_handles_hidden_files() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write(".hidden", "hidden");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/.hidden @team"));
}

#[test]
fn sync_handles_spaces_in_paths() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("my file.txt", "content");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains(r"/my\ file.txt @team"));
}

#[test]
fn sync_handles_unicode_paths() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /src/** @team\n");
    repo.write("src/über file.rs", "fn main() {}\n");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains(r"/src/über\ file.rs @team"));
}

#[test]
fn sync_handles_deeply_nested_files() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("a/b/c/d/e/f.txt", "deep");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/a/b/c/d/e/f.txt @team"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — .github/CODEOWNERS
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_updates_local_github_section_without_reaggregating() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /.github/workflows/** @org/platform
            # BEGIN CORGI GENERATED
            /stale.txt @org/stale
            # END CORGI GENERATED
        "},
    );
    repo.write(".github/workflows/ci.yml", "name: ci\n");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    // Local section is updated.
    assert!(content.contains("/.github/workflows/ci.yml @org/platform"));
    // Generated section is preserved unchanged.
    assert!(content.contains("/stale.txt @org/stale"));
}

#[test]
fn sync_treats_dotgithub_as_root_when_sole_manifest() {
    let repo = TestRepo::new();
    repo.write(
        ".github/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/backend
            # BEGIN CORGI GENERATED
            # END CORGI GENERATED
        "},
    );
    repo.write("README.md", "readme");
    repo.write("src/lib.rs", "lib");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    assert!(content.contains("/src/lib.rs @org/backend"));
    assert!(content.contains("/README.md"));
}

// ═══════════════════════════════════════════════════════════════════
// sync — error handling
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_rejects_manifest_with_patterns() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "*.rs @team\n");
    repo.write("src/lib.rs", "lib");

    let err = corgi_core::sync(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("wildcard"), "got: {msg}");
    assert!(msg.contains("migrate"), "got: {msg}");
}

// ═══════════════════════════════════════════════════════════════════
// migrate
// ═══════════════════════════════════════════════════════════════════

#[test]
fn migrate_converts_patterns_with_last_match_wins() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            /src/lib.rs @org/lib
            /src/** @org/backend
            *.md @org/docs
        "},
    );
    repo.write("README.md", "docs");
    repo.write("src/lib.rs", "lib");
    repo.write("src/api.rs", "api");

    let status = corgi_core::migrate(repo.path()).unwrap();
    assert_eq!(status, 1);

    let content = repo.read("CODEOWNERS");
    // /src/** is last matching for /src/lib.rs, so @org/backend wins.
    assert!(content.contains("/src/lib.rs @org/backend"));
    assert!(content.contains("/src/api.rs @org/backend"));
    assert!(content.contains("/README.md @org/docs"));
}

#[test]
fn migrate_is_idempotent() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "*.md @org/docs\n");
    repo.write("README.md", "docs");

    corgi_core::migrate(repo.path()).unwrap();
    let snapshot = repo.read("CODEOWNERS");

    let status = corgi_core::migrate(repo.path()).unwrap();
    assert_eq!(status, 0);
    assert_eq!(repo.read("CODEOWNERS"), snapshot);
}

#[test]
fn migrate_skips_manifest_without_patterns() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/file.txt @team\n");
    repo.write("file.txt", "content");

    let status = corgi_core::migrate(repo.path()).unwrap();
    assert_eq!(status, 0);
}

// ═══════════════════════════════════════════════════════════════════
// aggregate
// ═══════════════════════════════════════════════════════════════════

#[test]
fn aggregate_no_existing_generated_section() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        "# Local section\n/.github/ci.yml @org/platform\n",
    );
    repo.write(".github/ci.yml", "ci");

    let status = corgi_core::aggregate(repo.path()).unwrap();
    assert_eq!(status, 1);

    let content = repo.read(".github/CODEOWNERS");
    assert!(content.contains("# Local section"));
    assert!(content.contains("# BEGIN CORGI GENERATED"));
    assert!(content.contains("/README.md @org/root"));
    assert!(content.contains("# END CORGI GENERATED"));
}

#[test]
fn aggregate_replaces_existing_generated_section() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        indoc! {"
            # Local
            # BEGIN CORGI GENERATED
            /stale.txt @org/stale
            # END CORGI GENERATED
        "},
    );

    corgi_core::aggregate(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    assert!(
        !content.contains("stale.txt"),
        "old generated content should be removed"
    );
    assert!(content.contains("/README.md @org/root"));
}

#[test]
fn aggregate_preserves_prefix() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        indoc! {"
            # Important local rules
            /.github/ci.yml @org/platform
            # BEGIN CORGI GENERATED
            # END CORGI GENERATED
        "},
    );
    repo.write(".github/ci.yml", "ci");

    corgi_core::aggregate(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    assert!(content.contains("# Important local rules"));
    assert!(content.contains("/.github/ci.yml @org/platform"));
}

#[test]
fn aggregate_preserves_suffix() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        "# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n# suffix comment\n",
    );

    corgi_core::aggregate(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    assert!(content.contains("# suffix comment"));
}

#[test]
fn aggregate_is_idempotent() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/README.md @org/root\n");
    repo.write("README.md", "readme");
    repo.write(
        ".github/CODEOWNERS",
        "# Local\n# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n",
    );

    corgi_core::aggregate(repo.path()).unwrap();
    let snapshot = repo.read(".github/CODEOWNERS");

    let status = corgi_core::aggregate(repo.path()).unwrap();
    assert_eq!(status, 0);
    assert_eq!(repo.read(".github/CODEOWNERS"), snapshot);
}

#[test]
fn aggregate_deterministic_ordering() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/z.txt @org/z\n/a.txt @org/a\n");
    repo.write("a.txt", "a");
    repo.write("z.txt", "z");
    repo.write(
        ".github/CODEOWNERS",
        "# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n",
    );

    corgi_core::aggregate(repo.path()).unwrap();

    let content = repo.read(".github/CODEOWNERS");
    let a_pos = content.find("/a.txt").expect("/a.txt");
    let z_pos = content.find("/z.txt").expect("/z.txt");
    assert!(a_pos < z_pos, "entries should be sorted: {content}");
}

#[test]
fn aggregate_malformed_markers_returns_error() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/file @team\n");
    repo.write("file", "content");
    repo.write(".github/CODEOWNERS", "# BEGIN CORGI GENERATED\n");

    let err = corgi_core::aggregate(repo.path()).unwrap_err();
    assert!(err.to_string().contains("missing"), "got: {err}");
}

// ═══════════════════════════════════════════════════════════════════
// cross-package renames
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rename_inside_same_package_preserves_ownership() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/src/a.rs @org/team\n");
    repo.write("src/a.rs", "content");
    repo.commit("initial");
    repo.rename("src/a.rs", "src/b.rs");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/b.rs @org/team"));
    assert!(!content.contains("/src/a.rs"));
}

#[test]
fn rename_root_to_nested_package_uses_nested_ownership() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /** @org/root
            /src/foo.rs @org/explicit-root
        "},
    );
    repo.write("src/foo.rs", "root file");
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/api\n",
    );
    repo.write("packages/api/placeholder", "placeholder");
    repo.commit("initial");
    repo.rename("src/foo.rs", "packages/api/src/foo.rs");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    // The file is no longer in the root package.
    assert!(!root_co.contains("foo.rs"));

    let api_co = repo.read("packages/api/CODEOWNERS");
    // The nested package should own the file now via its rules.
    assert!(api_co.contains("/src/foo.rs"));
}

#[test]
fn rename_nested_to_root_package_uses_root_ownership() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/api\n/src/lib.rs @org/api-explicit\n",
    );
    repo.write("packages/api/src/lib.rs", "api lib");
    repo.write("root.txt", "root");
    repo.commit("initial");
    repo.rename("packages/api/src/lib.rs", "src/lib.rs");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(root_co.contains("/src/lib.rs"));

    let api_co = repo.read("packages/api/CODEOWNERS");
    assert!(!api_co.contains("lib.rs"));
}

#[test]
fn rename_between_sibling_packages() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/alpha/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/alpha\n/src/foo.rs @org/alpha-explicit\n",
    );
    repo.write(
        "packages/beta/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/beta\n",
    );
    repo.write("packages/alpha/src/foo.rs", "alpha");
    repo.write("packages/beta/placeholder", "beta");
    repo.write("root.txt", "root");
    repo.commit("initial");
    repo.rename("packages/alpha/src/foo.rs", "packages/beta/src/foo.rs");

    corgi_core::sync(repo.path()).unwrap();

    let alpha_co = repo.read("packages/alpha/CODEOWNERS");
    assert!(
        !alpha_co.contains("foo.rs"),
        "alpha should no longer own the file"
    );

    let beta_co = repo.read("packages/beta/CODEOWNERS");
    assert!(
        beta_co.contains("/src/foo.rs"),
        "beta should now own the file"
    );
}

// ═══════════════════════════════════════════════════════════════════
// no CODEOWNERS → returns 0
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_returns_zero_with_no_codeowners() {
    let repo = TestRepo::new();
    repo.write("README.md", "readme");

    let status = corgi_core::sync(repo.path()).unwrap();
    assert_eq!(status, 0);
}

#[test]
fn aggregate_returns_zero_with_no_packages() {
    let repo = TestRepo::new();
    repo.write("README.md", "readme");
    // With no CODEOWNERS at all and no .github/CODEOWNERS, aggregate may
    // create an empty file. Verify it does not panic.
    let _status = corgi_core::aggregate(repo.path()).unwrap();
}

#[test]
fn migrate_returns_zero_with_no_codeowners() {
    let repo = TestRepo::new();
    repo.write("README.md", "readme");

    let status = corgi_core::migrate(repo.path()).unwrap();
    assert_eq!(status, 0);
}
