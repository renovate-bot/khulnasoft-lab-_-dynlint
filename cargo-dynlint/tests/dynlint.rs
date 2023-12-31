use anyhow::Result;
use assert_cmd::Command;
use cargo_metadata::{Dependency, Metadata, MetadataCommand};
use dynlint_internal::{cargo::current_metadata, env};
use once_cell::sync::Lazy;
use regex::Regex;
use sedregex::find_and_replace;
use semver::Version;
use similar_asserts::SimpleDiff;
use std::{
    env::{set_current_dir, set_var},
    ffi::OsStr,
    fs::{read_to_string, write},
    io::{stderr, Write},
    path::{Component, Path},
    str::FromStr,
    sync::Mutex,
};
use tempfile::tempdir;

static METADATA: Lazy<Metadata> = Lazy::new(|| current_metadata().unwrap());

#[ctor::ctor]
fn initialize() {
    set_current_dir("..").unwrap();
    set_var(env::CARGO_TERM_COLOR, "never");
}

#[test]
fn versions_are_equal() {
    for package in &METADATA.packages {
        assert_eq!(
            env!("CARGO_PKG_VERSION"),
            package.version.to_string(),
            "{}",
            package.name
        );
    }
}

#[test]
fn nightly_crates_have_same_version_as_workspace() {
    for path in ["driver", "utils/linting"] {
        let metadata = MetadataCommand::new()
            .current_dir(path)
            .no_deps()
            .exec()
            .unwrap();
        let package = metadata.root_package().unwrap();
        assert_eq!(env!("CARGO_PKG_VERSION"), package.version.to_string());
    }
}

#[test]
fn versions_are_exact_and_match() {
    for package in &METADATA.packages {
        for Dependency { name: dep, req, .. } in &package.dependencies {
            if dep.starts_with("dynlint") {
                assert!(
                    req.to_string().starts_with('='),
                    "`{}` dependency on `{dep}` is not exact",
                    package.name
                );
                assert!(
                    req.matches(&Version::parse(env!("CARGO_PKG_VERSION")).unwrap()),
                    "`{}` dependency on `{dep}` does not match `{}`",
                    package.name,
                    env!("CARGO_PKG_VERSION"),
                );
            }
        }
    }
}

#[test]
fn requirements_do_not_include_patch_versions() {
    let metadata = ["driver", "utils/linting"].map(|path| {
        MetadataCommand::new()
            .current_dir(path)
            .no_deps()
            .exec()
            .unwrap()
    });

    for metadata in std::iter::once(&*METADATA).chain(metadata.iter()) {
        for package in &metadata.packages {
            for Dependency { name: dep, req, .. } in &package.dependencies {
                if dep.starts_with("dynlint") {
                    continue;
                }
                assert!(
                    req.comparators
                        .iter()
                        .all(|comparator| comparator.patch.is_none()),
                    "`{}` requirement on `{dep}` includes patch version: {req}",
                    package.name
                );
            }
        }
    }
}

#[test]
fn workspace_and_cargo_dynlint_readmes_are_equivalent() {
    let workspace_readme = readme_contents(".").unwrap();

    let cargo_dynlint_readme = readme_contents("cargo-dynlint").unwrap();

    let lifted_cargo_dynlint_readme = find_and_replace(
        &cargo_dynlint_readme,
        &[r"s/(?m)^(\[[^\]]*\]: *\.)\./${1}/g"],
    )
    .unwrap();

    compare_lines(&workspace_readme, &lifted_cargo_dynlint_readme);
}

#[test]
fn cargo_dynlint_and_dynlint_readmes_are_equal() {
    let cargo_dynlint_readme = readme_contents("cargo-dynlint").unwrap();

    let dynlint_readme = readme_contents("dynlint").unwrap();

    compare_lines(&cargo_dynlint_readme, &dynlint_readme);
}

#[test]
fn hack_feature_powerset() {
    Command::new("cargo")
        .env(env::RUSTFLAGS, "-D warnings")
        .args(["hack", "--feature-powerset", "check"])
        .assert()
        .success();
}

#[test]
fn markdown_does_not_use_inline_links() {
    for entry in walkdir(false) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension() != Some(OsStr::new("md"))
            || path.file_name() == Some(OsStr::new("CHANGELOG.md"))
        {
            continue;
        }
        let markdown = read_to_string(path).unwrap();
        assert!(
            !Regex::new(r"\[[^\]]*\]\(").unwrap().is_match(&markdown),
            "`{}` uses inline links",
            path.canonicalize().unwrap().to_string_lossy()
        );
    }
}

#[test]
fn markdown_reference_links_are_sorted() {
    let re = Regex::new(r"^\[[^\]]*\]:").unwrap();
    for entry in walkdir(true) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension() != Some(OsStr::new("md"))
            || path.file_name() == Some(OsStr::new("CHANGELOG.md"))
        {
            continue;
        }
        let markdown = read_to_string(path).unwrap();
        let links = markdown
            .lines()
            .filter(|line| re.is_match(line))
            .collect::<Vec<_>>();
        let mut links_sorted = links.clone();
        links_sorted.sort_unstable();
        assert!(
            links_sorted == links,
            "contents of `{}` are not what was expected:\n{}\n",
            path.canonicalize().unwrap().to_string_lossy(),
            links_sorted.join("\n")
        );
    }
}

#[test]
fn markdown_reference_links_are_valid_and_used() {
    const CODE: &str = "`[^`]*`";
    const CODE_BLOCK: &str = "```([^`]|`[^`]|``[^`])*```";
    let ref_re = Regex::new(&format!(r#"(?m){CODE}|{CODE_BLOCK}|\[([^\]]*)\]([^:]|$)"#)).unwrap();
    let link_re = Regex::new(r"(?m)^\[([^\]]*)\]:").unwrap();
    for entry in walkdir(true) {
        let entry = entry.unwrap();
        let path = entry.path();
        // smoelius: The ` ["\n```"] ` in `missing_doc_comment_openai`'s readme causes problems, and
        // I haven't found a good solution/workaround.
        if path.extension() != Some(OsStr::new("md"))
            || path.file_name() == Some(OsStr::new("CHANGELOG.md"))
            || path.ends_with("examples/README.md")
            || path
                .components()
                .any(|c| c == Component::Normal(OsStr::new("missing_doc_comment_openai")))
        {
            continue;
        }
        let markdown = read_to_string(path).unwrap();
        let mut refs = ref_re
            .captures_iter(&markdown)
            .filter_map(|captures| {
                // smoelius: 2 because 1 is the parenthesized expression in `CODE_BLOCK`.
                captures.get(2).map(|m| {
                    m.as_str()
                        .replace('\r', "")
                        .replace('\n', " ")
                        .to_lowercase()
                })
            })
            .collect::<Vec<_>>();

        // smoelius: The use of `to_lowercase` in the next statement is a convenience and should
        // eventually be removed. `prettier` 2.8.2 stopped lowercasing link labels. But as of this
        // writing, the latest version of the Prettier VS Code extension (9.10.4) still appears to
        // use `prettier` 2.8.0.
        //
        // References:
        // - https://github.com/prettier/prettier/pull/13155
        // - https://github.com/prettier/prettier/blob/main/CHANGELOG.md#282
        // - https://github.com/prettier/prettier-vscode/blob/main/CHANGELOG.md#9103
        let mut links = link_re
            .captures_iter(&markdown)
            .map(|captures| captures.get(1).unwrap().as_str().to_lowercase())
            .collect::<Vec<_>>();

        refs.sort_unstable();
        refs.dedup();

        links.sort_unstable();
        links.dedup();

        assert_eq!(refs, links, "failed for {path:?}");
    }
}

// smoelius: `markdown_link_check` must use absolute paths because `npx markdown-link-check` is run
// from a temporary directory.
#[cfg(not(target_os = "windows"))]
#[test]
fn markdown_link_check() {
    let tempdir = tempdir().unwrap();

    Command::new("npm")
        .args(["install", "markdown-link-check"])
        .current_dir(&tempdir)
        .assert()
        .success();

    // smoelius: https://github.com/rust-lang/crates.io/issues/788
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/markdown_link_check.json");

    for entry in walkdir(true) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension() != Some(OsStr::new("md")) {
            continue;
        }

        let path_buf = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path);

        Command::new("npx")
            .args([
                "markdown-link-check",
                "--config",
                &config.to_string_lossy(),
                &path_buf.to_string_lossy(),
            ])
            .current_dir(&tempdir)
            .assert()
            .success();
    }
}

#[test]
fn msrv() {
    for package in &METADATA.packages {
        if package.rust_version.is_none() {
            continue;
        }
        let manifest_dir = package.manifest_path.parent().unwrap();
        Command::new("cargo")
            .args([
                "msrv",
                "verify",
                "--",
                "cargo",
                "check",
                "--no-default-features",
            ])
            .current_dir(manifest_dir)
            .assert()
            .success();
    }
}

#[cfg(not(windows))]
#[test]
fn prettier_all_but_examples_and_template() {
    Command::new("prettier")
        .args([
            "--check",
            "--ignore-path",
            "cargo-dynlint/tests/prettier_ignore.txt",
            "**/*.md",
            "**/*.yml",
        ])
        .assert()
        .success();
}

#[cfg(not(windows))]
#[test]
fn prettier_examples_and_template() {
    preserves_cleanliness("prettier", true, || {
        Command::new("prettier")
            .args(["--write", "examples/**/*.md", "internal/template/**/*.md"])
            .assert()
            .success();
    });
}

// smoelius: `supply_chain` is the only test that uses `supply_chain.json`. So there is no race.
#[cfg_attr(dynlint_lib = "general", allow(non_thread_safe_call_in_test))]
#[cfg_attr(dynlint_lib = "overscoped_allow", allow(overscoped_allow))]
#[test]
fn supply_chain() {
    Command::new("cargo")
        .args(["supply-chain", "update"])
        .assert()
        .success();

    let assert = Command::new("cargo")
        .args(["supply-chain", "json", "--no-dev"])
        .assert()
        .success();

    let stdout_actual = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let value = serde_json::Value::from_str(stdout_actual).unwrap();
    let stdout_normalized = serde_json::to_string_pretty(&value).unwrap();

    let path = Path::new("cargo-dynlint/tests/supply_chain.json");

    let stdout_expected = read_to_string(path).unwrap();

    if env::enabled("BLESS") {
        write(path, stdout_normalized).unwrap();
    } else {
        assert!(
            stdout_expected == stdout_normalized,
            "{}",
            SimpleDiff::from_str(&stdout_expected, &stdout_normalized, "left", "right")
        );
    }
}

fn readme_contents(dir: impl AsRef<Path>) -> Result<String> {
    #[allow(unknown_lints, env_cargo_path)]
    read_to_string(dir.as_ref().join("README.md")).map_err(Into::into)
}

fn compare_lines(left: &str, right: &str) {
    assert_eq!(left.lines().count(), right.lines().count());

    for (left, right) in left.lines().zip(right.lines()) {
        assert_eq!(left, right);
    }
}

// smoelius: Skip examples directory for now.
fn walkdir(include_examples: bool) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    #[allow(unknown_lints, env_cargo_path)]
    walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(move |entry| {
            entry.path().file_name() != Some(OsStr::new("target"))
                && (include_examples || entry.path().file_name() != Some(OsStr::new("examples")))
        })
}

static MUTEX: Mutex<()> = Mutex::new(());

fn preserves_cleanliness(test_name: &str, ignore_blank_lines: bool, f: impl FnOnce()) {
    let _lock = MUTEX.lock().unwrap();

    if cfg!(not(feature = "strict")) && dirty(false).is_some() {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `{test_name}` test as repository is dirty"
        )
        .unwrap();
        return;
    }

    f();

    if let Some(stdout) = dirty(ignore_blank_lines) {
        panic!("{}", stdout);
    }

    // smoelius: If the repository is not dirty with `ignore_blank_lines` set to true, but would be
    // dirty otherwise, then restore the repository's contents.
    if ignore_blank_lines && dirty(false).is_some() {
        Command::new("git")
            .args(["checkout", "."])
            .assert()
            .success();
    }
}

fn dirty(ignore_blank_lines: bool) -> Option<String> {
    let mut command = Command::new("git");
    command.arg("diff");
    if ignore_blank_lines {
        command.arg("--ignore-blank-lines");
    }
    let output = command.output().unwrap();

    // smoelius: `--ignore-blank-lines` does not work with `--exit-code`. So instead check whether
    // stdout is empty.
    if output.stdout.is_empty() {
        None
    } else {
        Some(String::from_utf8(output.stdout).unwrap())
    }
}
