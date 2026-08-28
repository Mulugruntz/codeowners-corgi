use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn init_repo() -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    run_git(temp.path(), ["init"]);
    run_git(temp.path(), ["config", "user.email", "corgi@test.example"]);
    run_git(temp.path(), ["config", "user.name", "CORGI Test"]);
    run_git(temp.path(), ["checkout", "-b", "main"]);
    temp
}

fn write(repo: &Path, relative: &str, content: &str) {
    let path = repo.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write file");
}

fn run_git<I, S>(repo: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_corgi(repo: &Path, subcommand: &str) -> assert_cmd::assert::Assert {
    let mut command = Command::cargo_bin("corgi").expect("cargo bin");
    command.current_dir(repo).arg(subcommand).assert()
}

// ═══════════════════════════════════════════════════════════════════
// --help and --version
// ═══════════════════════════════════════════════════════════════════

#[test]
fn help_flag() {
    Command::cargo_bin("corgi")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("corgi")
                .and(predicate::str::contains("sync"))
                .and(predicate::str::contains("aggregate"))
                .and(predicate::str::contains("migrate")),
        );
}

#[test]
fn version_flag() {
    Command::cargo_bin("corgi")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("corgi"));
}

// ═══════════════════════════════════════════════════════════════════
// unknown command / missing subcommand
// ═══════════════════════════════════════════════════════════════════

#[test]
fn unknown_subcommand_fails() {
    Command::cargo_bin("corgi")
        .unwrap()
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn no_subcommand_fails() {
    Command::cargo_bin("corgi").unwrap().assert().failure();
}

// ═══════════════════════════════════════════════════════════════════
// exit code mapping: 0 / 1 / 2
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_exit_zero_when_fully_owned() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        "/CODEOWNERS @team\n/file.txt @team\n",
    );
    write(repo.path(), "file.txt", "content");

    run_corgi(repo.path(), "sync").code(0);
}

#[test]
fn sync_exit_one_when_changed_or_unowned() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "");
    write(repo.path(), "file.txt", "content");

    run_corgi(repo.path(), "sync").code(1);
}

#[test]
fn sync_exit_two_on_fatal_error() {
    let repo = init_repo();
    // Manifest with patterns → sync should fail with error.
    write(repo.path(), "CODEOWNERS", "*.rs @team\n");
    write(repo.path(), "src/lib.rs", "lib");

    run_corgi(repo.path(), "sync")
        .code(2)
        .stderr(predicate::str::contains("wildcard"));
}

#[test]
fn aggregate_exit_zero_when_unchanged() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "/README.md @org/root\n");
    write(repo.path(), "README.md", "readme");
    write(
        repo.path(),
        ".github/CODEOWNERS",
        "# BEGIN CORGI GENERATED\n/README.md @org/root\n# END CORGI GENERATED\n",
    );

    run_corgi(repo.path(), "aggregate").code(0);
}

#[test]
fn aggregate_exit_one_when_changed() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "/README.md @org/root\n");
    write(repo.path(), "README.md", "readme");
    write(
        repo.path(),
        ".github/CODEOWNERS",
        "# BEGIN CORGI GENERATED\n/stale @stale\n# END CORGI GENERATED\n",
    );

    run_corgi(repo.path(), "aggregate").code(1);
}

#[test]
fn migrate_exit_zero_when_no_patterns() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "/file.txt @team\n");
    write(repo.path(), "file.txt", "content");

    run_corgi(repo.path(), "migrate").code(0);
}

// ═══════════════════════════════════════════════════════════════════
// smoke: sync dispatch works end-to-end
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_smoke_end_to_end() {
    let repo = init_repo();
    write(
        repo.path(),
        "CODEOWNERS",
        "# Rule[auto-assign]: /** @team\n",
    );
    write(repo.path(), "file.txt", "content");

    run_corgi(repo.path(), "sync").code(1);

    let content = fs::read_to_string(repo.path().join("CODEOWNERS")).unwrap();
    assert!(content.contains("/file.txt @team"));
}

// ═══════════════════════════════════════════════════════════════════
// stderr on unowned files
// ═══════════════════════════════════════════════════════════════════

#[test]
fn sync_stderr_unowned_summary() {
    let repo = init_repo();
    write(repo.path(), "CODEOWNERS", "");
    write(repo.path(), "unowned.txt", "content");

    run_corgi(repo.path(), "sync").code(1).stderr(
        predicate::str::contains("unowned files remain")
            .and(predicate::str::contains("auto-assign")),
    );
}
