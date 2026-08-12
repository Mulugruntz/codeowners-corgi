use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use indoc::indoc;
use tempfile::TempDir;

fn init_repo() -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    run_git(temp.path(), ["init"]);
    run_git(temp.path(), ["config", "user.email", "corgi@example.com"]);
    run_git(temp.path(), ["config", "user.name", "CORGI Test"]);
    temp
}

fn write(repo: &Path, relative: &str, content: &str) {
    let path = repo.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write file");
}

fn read(repo: &Path, relative: &str) -> String {
    fs::read_to_string(repo.join(relative)).expect("read file")
}

fn run_git<I, S>(repo: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git command");
    assert!(status.success());
}

fn run_corgi(repo: &Path, subcommand: &str) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("corgi").expect("cargo bin");
    command.current_dir(repo).arg(subcommand).assert()
}

#[test]
fn sync_handles_additions_deletions_unowned_and_idempotency() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        indoc! {"
            # header
            /deleted.txt @old/team
        "},
    );
    write(repo.path(), "keep.txt", "keep");

    let first = run_corgi(repo.path(), "sync");
    first.code(1);

    let expected = indoc! {"
        # header
        /CODEOWNERS
        /keep.txt
    "};
    assert_eq!(read(repo.path(), "CODEOWNERS"), expected);

    let snapshot = read(repo.path(), "CODEOWNERS");
    run_corgi(repo.path(), "sync").code(1);
    assert_eq!(read(repo.path(), "CODEOWNERS"), snapshot);
}

#[test]
fn sync_respects_nested_roots_gitignore_and_auto_assign_rules() {
    let repo = init_repo();
    write(
        repo.path(),
        ".gitignore",
        "ignored.log\npackages/api/ignored.txt\n",
    );
    write(
        repo.path(),
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/root
        "},
    );
    write(repo.path(), "README.md", "root");
    write(repo.path(), "src/lib.rs", "lib");
    write(repo.path(), "ignored.log", "ignored");
    write(
        repo.path(),
        "packages/api/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/api
        "},
    );
    write(repo.path(), "packages/api/src/main.rs", "fn main() {}");
    write(repo.path(), "packages/api/ignored.txt", "ignored");

    run_corgi(repo.path(), "sync").code(1);

    assert_eq!(
        read(repo.path(), "CODEOWNERS"),
        indoc! {"
            /.gitignore
            /CODEOWNERS
            /README.md
            # Rule[auto-assign]: /src/** @org/root
            /src/lib.rs @org/root
        "}
    );
    assert_eq!(
        read(repo.path(), "packages/api/CODEOWNERS"),
        indoc! {"
            /CODEOWNERS
            # Rule[auto-assign]: /src/** @org/api
            /src/main.rs @org/api
        "}
    );
}

#[test]
fn sync_preserves_comments_on_git_renames() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        indoc! {"
            # keep with file
            /old.txt @org/team
        "},
    );
    write(repo.path(), "old.txt", "hello");
    run_git(repo.path(), ["add", "."]);
    run_git(repo.path(), ["commit", "-m", "initial"]);
    run_git(repo.path(), ["mv", "old.txt", "new.txt"]);

    run_corgi(repo.path(), "sync").code(1);

    assert_eq!(
        read(repo.path(), "CODEOWNERS"),
        indoc! {"
            # keep with file
            /CODEOWNERS
            /new.txt @org/team
        "}
    );
}

#[test]
fn sync_uses_most_specific_auto_assign_rule_without_overwriting_existing_owner() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /src/** @org/root
            # Rule[auto-assign]: /src/special/** @org/special
            /src/existing.rs @org/manual
        "},
    );
    write(repo.path(), "src/existing.rs", "existing");
    write(repo.path(), "src/new.rs", "new");
    write(repo.path(), "src/special/feature.rs", "feature");

    run_corgi(repo.path(), "sync").code(1);

    assert_eq!(
        read(repo.path(), "CODEOWNERS"),
        indoc! {"
            /CODEOWNERS
            # Rule[auto-assign]: /src/** @org/root
            /src/existing.rs @org/manual
            /src/new.rs @org/root
            # Rule[auto-assign]: /src/special/** @org/special
            /src/special/feature.rs @org/special
        "}
    );
}

#[test]
fn migrate_converts_patterns_with_last_match_wins_and_is_idempotent() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        indoc! {"
            /src/lib.rs @org/lib
            /src/** @org/backend
            *.md @org/docs
        "},
    );
    write(repo.path(), "README.md", "docs");
    write(repo.path(), "src/lib.rs", "lib");
    write(repo.path(), "src/api.rs", "api");

    run_corgi(repo.path(), "migrate").code(1);

    let expected = indoc! {"
        # Rule[auto-assign]: *.md @org/docs
        /CODEOWNERS
        /README.md @org/docs
        # Rule[auto-assign]: /src/** @org/backend
        /src/api.rs @org/backend
        /src/lib.rs @org/backend
    "};
    assert_eq!(read(repo.path(), "CODEOWNERS"), expected);

    let snapshot = read(repo.path(), "CODEOWNERS");
    run_corgi(repo.path(), "migrate").code(0);
    assert_eq!(read(repo.path(), "CODEOWNERS"), snapshot);
}

#[test]
fn aggregate_preserves_local_github_section_and_ignores_previous_generated_output() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "/README.md @org/root\n");
    write(repo.path(), "README.md", "readme");
    write(
        repo.path(),
        "packages/web/CODEOWNERS",
        "/src/main.ts @org/web\n",
    );
    write(repo.path(), "packages/web/src/main.ts", "console.log('x');");
    write(
        repo.path(),
        ".github/CODEOWNERS",
        indoc! {"
            # Local .github ownership/rules:
            /.github/workflows/ci.yml @org/platform

            # BEGIN CORGI GENERATED
            /stale.txt @org/stale
            # END CORGI GENERATED
        "},
    );
    write(repo.path(), ".github/workflows/ci.yml", "name: ci\n");

    run_corgi(repo.path(), "aggregate").code(1);

    let expected = indoc! {"
        # Local .github ownership/rules:
        /.github/workflows/ci.yml @org/platform

        # BEGIN CORGI GENERATED
        /README.md @org/root
        /packages/web/src/main.ts @org/web
        # END CORGI GENERATED
    "};
    assert_eq!(read(repo.path(), ".github/CODEOWNERS"), expected);
    run_corgi(repo.path(), "aggregate").code(0);
}

#[test]
fn sync_updates_local_github_section_without_reaggregating() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "/README.md @org/root\n");
    write(repo.path(), "README.md", "readme");
    write(
        repo.path(),
        ".github/CODEOWNERS",
        indoc! {"
            # Rule[auto-assign]: /.github/workflows/** @org/platform

            # BEGIN CORGI GENERATED
            /stale.txt @org/stale
            # END CORGI GENERATED
        "},
    );
    write(repo.path(), ".github/workflows/ci.yml", "name: ci\n");

    run_corgi(repo.path(), "sync").code(1);

    assert_eq!(
        read(repo.path(), ".github/CODEOWNERS"),
        indoc! {"
            /.github/CODEOWNERS
            # Rule[auto-assign]: /.github/workflows/** @org/platform
            /.github/workflows/ci.yml @org/platform

            # BEGIN CORGI GENERATED
            /stale.txt @org/stale
            # END CORGI GENERATED
        "}
    );
}

#[test]
fn sync_handles_spaces_and_unicode_paths() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        "# Rule[auto-assign]: /src/** @org/team\n",
    );
    write(repo.path(), "src/über file.rs", "fn main() {}\n");

    run_corgi(repo.path(), "sync").code(1);

    assert_eq!(
        read(repo.path(), "CODEOWNERS"),
        "/CODEOWNERS\n# Rule[auto-assign]: /src/** @org/team\n/src/über\\ file.rs @org/team\n"
    );
}
