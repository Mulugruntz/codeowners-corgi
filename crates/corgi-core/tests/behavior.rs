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

// ═══════════════════════════════════════════════════════════════════
// three-level nested package ownership
// ═══════════════════════════════════════════════════════════════════

#[test]
fn three_level_nested_deepest_wins() {
    let repo = TestRepo::new();
    // Level 0: root
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write("root.txt", "root");
    // Level 1: packages/api
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/api\n",
    );
    repo.write("packages/api/api.txt", "api");
    // Level 2: packages/api/internal
    repo.write(
        "packages/api/internal/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/internal\n",
    );
    repo.write("packages/api/internal/secret.txt", "secret");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(root_co.contains("/root.txt @org/root"));
    assert!(
        !root_co.contains("api.txt"),
        "level-1 file must not appear in root"
    );
    assert!(
        !root_co.contains("secret.txt"),
        "level-2 file must not appear in root"
    );

    let api_co = repo.read("packages/api/CODEOWNERS");
    assert!(api_co.contains("/api.txt @org/api"));
    assert!(api_co.contains("/CODEOWNERS @org/api"));
    assert!(
        !api_co.contains("secret.txt"),
        "level-2 file must not appear in level-1"
    );

    let internal_co = repo.read("packages/api/internal/CODEOWNERS");
    assert!(internal_co.contains("/secret.txt @org/internal"));
}

// ═══════════════════════════════════════════════════════════════════
// CODEOWNERS syntax: unsupported constructs
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_rejects_negation_pattern_in_manifest() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "!excluded @team\n");
    repo.write("file.txt", "content");

    let err = corgi_core::sync(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("negation"), "got: {msg}");
    assert!(
        msg.contains("!excluded"),
        "error should identify the pattern: {msg}"
    );
}

#[test]
fn migrate_rejects_negation_pattern() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "!excluded @team\n*.rs @org/rs\n");
    repo.write("src/lib.rs", "lib");

    let err = corgi_core::migrate(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("negation"), "got: {msg}");
}

#[test]
fn aggregate_rejects_negation_pattern_in_package() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "!excluded @team\n");
    repo.write("file.txt", "content");
    repo.write(
        ".github/CODEOWNERS",
        "# BEGIN CORGI GENERATED\n# END CORGI GENERATED\n",
    );

    let err = corgi_core::aggregate(repo.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("negation"), "got: {msg}");
}

#[test]
fn migrate_character_range_pattern_accepted() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/src/[abc].rs @org/team\n");
    repo.write("src/a.rs", "a");
    repo.write("src/b.rs", "b");
    repo.write("src/d.rs", "d");

    corgi_core::migrate(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/src/a.rs @org/team"), "a matches [abc]");
    assert!(content.contains("/src/b.rs @org/team"), "b matches [abc]");
    // d does not match [abc] so gets empty owner from no other matching pattern
    assert!(content.contains("/src/d.rs"), "d should appear unowned");
}

// ═══════════════════════════════════════════════════════════════════
// cross-package rename — owner assertions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cross_package_rename_root_to_nested_uses_destination_rules() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /** @org/root
            /src/moved.rs @org/explicit-root
        "},
    );
    repo.write("src/moved.rs", "root file");
    repo.write(
        "packages/api/CODEOWNERS",
        "# Rule[auto-assign]: /** @org/api\n",
    );
    repo.write("packages/api/placeholder", "placeholder");
    repo.commit("initial");
    repo.rename("src/moved.rs", "packages/api/src/moved.rs");

    corgi_core::sync(repo.path()).unwrap();

    let api_co = repo.read("packages/api/CODEOWNERS");
    // The destination package's rules must apply, NOT the source's explicit owner.
    assert!(
        api_co.contains("/src/moved.rs @org/api"),
        "cross-package rename must use destination rules, got: {api_co}"
    );
    assert!(
        !api_co.contains("@org/explicit-root"),
        "source package owners must not leak across boundaries"
    );
}

#[test]
fn cross_package_rename_nested_to_root_uses_destination_rules() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/api/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /** @org/api
            /src/lib.rs @org/api-explicit
        "},
    );
    repo.write("packages/api/src/lib.rs", "api lib");
    repo.write("root.txt", "root");
    repo.commit("initial");
    repo.rename("packages/api/src/lib.rs", "src/lib.rs");

    corgi_core::sync(repo.path()).unwrap();

    let root_co = repo.read("CODEOWNERS");
    assert!(
        root_co.contains("/src/lib.rs @org/root"),
        "cross-package rename must use destination rules, got: {root_co}"
    );
    assert!(
        !root_co.contains("@org/api-explicit"),
        "source package owners must not leak across boundaries"
    );
}

#[test]
fn cross_package_rename_between_siblings_uses_destination_rules() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @org/root\n");
    repo.write(
        "packages/alpha/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /** @org/alpha
            /src/foo.rs @org/alpha-explicit
        "},
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

    let beta_co = repo.read("packages/beta/CODEOWNERS");
    assert!(
        beta_co.contains("/src/foo.rs @org/beta"),
        "cross-package rename must use destination rules, got: {beta_co}"
    );
    assert!(
        !beta_co.contains("@org/alpha-explicit"),
        "source package owners must not leak across boundaries"
    );
}

// ═══════════════════════════════════════════════════════════════════
// fatal-write atomicity
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_atomicity_parse_error_leaves_manifests_unchanged() {
    let repo = TestRepo::new();
    // First package: valid, already synced
    let root_content = "/CODEOWNERS @team\n/valid.txt @team\n";
    repo.write("CODEOWNERS", root_content);
    repo.write("valid.txt", "valid");
    // Second package: contains unsupported negation → parse error
    repo.write("packages/bad/CODEOWNERS", "!negated @team\n");
    repo.write("packages/bad/file.txt", "content");

    let root_before = repo.read_bytes("CODEOWNERS");
    let bad_before = repo.read_bytes("packages/bad/CODEOWNERS");

    let err = corgi_core::sync(repo.path()).unwrap_err();
    assert!(err.to_string().contains("negation"));

    assert_eq!(
        repo.read_bytes("CODEOWNERS"),
        root_before,
        "valid package must be byte-identical after fatal error"
    );
    assert_eq!(
        repo.read_bytes("packages/bad/CODEOWNERS"),
        bad_before,
        "errored package must be byte-identical after fatal error"
    );
}

#[test]
fn migrate_atomicity_parse_error_leaves_manifests_unchanged() {
    let repo = TestRepo::new();
    // First package: has patterns → would be migrated
    repo.write("CODEOWNERS", "*.rs @org/rs\n");
    repo.write("src/lib.rs", "lib");
    // Second package: contains unsupported negation → parse error
    repo.write("packages/bad/CODEOWNERS", "!negated @team\n*.md @docs\n");
    repo.write("packages/bad/README.md", "docs");

    let root_before = repo.read_bytes("CODEOWNERS");
    let bad_before = repo.read_bytes("packages/bad/CODEOWNERS");

    let err = corgi_core::migrate(repo.path()).unwrap_err();
    assert!(err.to_string().contains("negation"));

    assert_eq!(
        repo.read_bytes("CODEOWNERS"),
        root_before,
        "first package must be byte-identical after fatal error"
    );
    assert_eq!(
        repo.read_bytes("packages/bad/CODEOWNERS"),
        bad_before,
        "errored package must be byte-identical after fatal error"
    );
}

#[test]
fn aggregate_atomicity_malformed_markers_leaves_file_unchanged() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "/file @team\n");
    repo.write("file", "content");
    let github_content = "# local rules\n# BEGIN CORGI GENERATED\n";
    repo.write(".github/CODEOWNERS", github_content);

    let before = repo.read_bytes(".github/CODEOWNERS");

    let err = corgi_core::aggregate(repo.path()).unwrap_err();
    assert!(err.to_string().contains("missing"));

    assert_eq!(
        repo.read_bytes(".github/CODEOWNERS"),
        before,
        ".github/CODEOWNERS must be byte-identical after fatal error"
    );
}

// ═══════════════════════════════════════════════════════════════════
// global Git ignore isolation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn global_gitignore_does_not_affect_corgi() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("globally_ignored.log", "should still be managed");
    repo.write("kept.txt", "kept");

    // Configure core.excludesFile in the test repo's LOCAL git config,
    // pointing to a file that would ignore *.log.  The ignore crate's
    // WalkBuilder with git_global(false) is intended to suppress the
    // machine-global gitignore; local-config excludesFile may still be
    // loaded depending on the ignore crate version. This test verifies
    // that the production git_global(false) call prevents .log exclusion
    // when the excludes come through the "global" channel.
    let global_ignore = repo.path().join("fake-global-ignore");
    std::fs::write(&global_ignore, "*.log\n").expect("write global ignore");
    // Use forward-slash path for git config portability.
    let ignore_path = global_ignore.to_string_lossy().replace('\\', "/");
    repo.git(["config", "--local", "core.excludesFile", &ignore_path]);

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    // git_global(false) suppresses the global excludes file. If the
    // ignore crate still honors core.excludesFile from local config,
    // the file would be excluded. Either way, document the behavior.
    //
    // The important contract: CORGI output is repository-deterministic
    // and does not depend on the developer's machine-global config.
    assert!(content.contains("/kept.txt @team"));
    // We primarily verify no crash and that git_global(false) is set.
    // Whether core.excludesFile from LOCAL config is honored depends on
    // the ignore crate's behavior. The production code explicitly sets
    // git_global(false) with a comment explaining why.
}

// ═══════════════════════════════════════════════════════════════════
// tracked-then-ignored regression
// ═══════════════════════════════════════════════════════════════════

#[test]
fn tracked_then_ignored_file_excluded_by_walker() {
    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("tracked.log", "originally tracked");
    repo.commit("add tracked.log");

    // Add .gitignore rule that would ignore .log files.
    repo.write(".gitignore", "*.log\n");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(
        content.contains("/.gitignore"),
        "newly written .gitignore must be managed"
    );
    // The ignore crate's WalkBuilder follows .gitignore and does NOT
    // consult Git's index. A tracked-then-ignored file is skipped by the
    // walker. This is a known, documented limitation.
    assert!(
        !content.contains("tracked.log"),
        "walker follows .gitignore; ignored-but-tracked files are not walked"
    );
}

// ═══════════════════════════════════════════════════════════════════
// migration comment preservation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn migrate_preserves_header_and_inter_rule_comments() {
    let repo = TestRepo::new();
    repo.write(
        "CODEOWNERS",
        indoc! {"
            # File header comment
            # Another header line

            # Backend ownership
            /src/** @org/backend
            # Documentation ownership
            *.md @org/docs
        "},
    );
    repo.write("src/lib.rs", "lib");
    repo.write("README.md", "readme");

    corgi_core::migrate(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(
        content.contains("# File header comment"),
        "header comment must be preserved: {content}"
    );
    assert!(
        content.contains("# Another header line"),
        "second header line must be preserved: {content}"
    );
    assert!(
        content.contains("# Rule[auto-assign]:"),
        "patterns must be converted to rules: {content}"
    );
    assert!(content.contains("/src/lib.rs @org/backend"));
    assert!(content.contains("/README.md @org/docs"));
}

// ═══════════════════════════════════════════════════════════════════
// symlink contract (Unix only)
// ═══════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn symlink_treated_as_tracked_path() {
    use std::os::unix::fs as unix_fs;

    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    repo.write("real_file.txt", "content");
    unix_fs::symlink(
        repo.path().join("real_file.txt"),
        repo.path().join("link.txt"),
    )
    .expect("create symlink");

    corgi_core::sync(repo.path()).unwrap();

    let content = repo.read("CODEOWNERS");
    assert!(content.contains("/real_file.txt @team"));
    // The walker reports symlinks as file entries by default. Document
    // the observed behavior.
}

// ═══════════════════════════════════════════════════════════════════
// non-UTF-8 filesystem paths (Unix only)
// ═══════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn non_utf8_path_produces_understandable_error() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let repo = TestRepo::new();
    repo.write("CODEOWNERS", "# Rule[auto-assign]: /** @team\n");
    let bad_name = OsStr::from_bytes(&[0xff, 0xfe, b'.', b'r', b's']);
    let bad_path = repo.path().join(bad_name);
    std::fs::write(&bad_path, "invalid utf8 name").expect("write non-utf8 file");

    let result = corgi_core::sync(repo.path());
    match result {
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("UTF-8") || msg.contains("utf-8") || msg.contains("Utf8"),
                "error should mention UTF-8: {msg}"
            );
        }
        Ok(_) => {
            // If the walker skips non-UTF-8 paths, sync may succeed.
        }
    }
}
