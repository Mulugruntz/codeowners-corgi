use std::{fs, path::Path, process::Command};

use tempfile::TempDir;

/// A temporary Git repository for integration tests.
///
/// Initializes `git init` with a deterministic local identity so tests never
/// depend on the developer's global Git configuration.
pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        git(dir.path(), ["init"]);
        git(dir.path(), ["config", "user.email", "corgi@test.example"]);
        git(dir.path(), ["config", "user.name", "CORGI Test"]);
        // Ensure deterministic default branch name.
        git(dir.path(), ["checkout", "-b", "main"]);
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.dir.path().join(relative)).expect("read file")
    }

    pub fn git<I, S>(&self, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        git(self.dir.path(), args);
    }

    pub fn commit(&self, message: &str) {
        self.git(["add", "."]);
        self.git(["commit", "-m", message, "--allow-empty"]);
    }

    pub fn rename(&self, from: &str, to: &str) {
        let to_path = self.dir.path().join(to);
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        self.git(["mv", from, to]);
    }
}

fn git<I, S>(repo: &Path, args: I)
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
