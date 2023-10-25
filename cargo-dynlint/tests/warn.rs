use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn no_libraries_were_found() {
    let tempdir = tempdir().unwrap();

    std::process::Command::new("cargo")
        .current_dir(&tempdir)
        .args([
            "init",
            "--name",
            tempdir
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_start_matches('.'),
        ])
        .assert()
        .success();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::eq("Warning: No libraries were found.\n"));

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "list"])
        .assert()
        .success()
        .stderr(predicate::eq("Warning: No libraries were found.\n"));
}

#[test]
fn nothing_to_do() {
    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .args(["dynlint"])
        .assert()
        .success()
        .stderr(predicate::eq(
            "Warning: Nothing to do. Did you forget `--all`?\n",
        ));
}
