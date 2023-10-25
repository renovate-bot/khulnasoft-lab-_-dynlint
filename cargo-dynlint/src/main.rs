use clap::{crate_version, ArgAction, Parser};
use std::{
    ffi::{OsStr, OsString},
    fmt::Debug,
};

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo", display_name = "cargo")]
struct Opts {
    #[clap(subcommand)]
    subcmd: CargoSubCommand,
}

#[derive(Debug, Parser)]
enum CargoSubCommand {
    Dynlint(Dynlint),
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[clap(
    version = crate_version!(),
    args_conflicts_with_subcommands = true,
    after_help = r#"ENVIRONMENT VARIABLES:

DYNLINT_DRIVER_PATH (default: $HOME/.dynlint_drivers) is the directory where Dynlint stores rustc
drivers.

DYNLINT_LIBRARY_PATH (default: none) is a colon-separated list of directories where Dynlint searches
for libraries.

DYNLINT_RUSTFLAGS (default: none) is a space-separated list of flags that Dynlint passes to `rustc`
when checking the packages in the workspace.

METADATA EXAMPLE:

    [workspace.metadata.dynlint]
    libraries = [
        { git = "https://github.com/khulnasoft-lab/dynlint", pattern = "examples/*/*" },
        { path = "libs/*" },
    ]
"#,
)]
// smoelius: Please keep the field `name_opts` first, and the fields `subcmd`, `names`, and `args`
// last. Please keep all other fields sorted.
struct Dynlint {
    #[clap(flatten)]
    name_opts: NameOpts,

    #[clap(long, hide = true)]
    allow_downgrade: bool,

    #[clap(long, hide = true)]
    bisect: bool,

    #[clap(long, help = "Automatically apply lint suggestions")]
    fix: bool,

    #[clap(long, hide = true)]
    force: bool,

    #[clap(long, hide = true)]
    isolate: bool,

    #[clap(long, help = "Continue if `cargo check` fails")]
    keep_going: bool,

    #[clap(long, hide = true)]
    list: bool,

    #[clap(
        long,
        value_name = "path",
        help = "Path to Cargo.toml. Note: if the manifest uses metadata, then `--manifest-path \
                <path>` must appear before `--`, not after."
    )]
    manifest_path: Option<String>,

    #[clap(long = "new", hide = true)]
    new_path: Option<String>,

    #[clap(long, help = "Do not check other packages within the workspace")]
    no_deps: bool,

    #[clap(
        action = ArgAction::Append,
        number_of_values = 1,
        short,
        long = "package",
        value_name = "spec",
        help = "Package to check"
    )]
    packages: Vec<String>,

    #[clap(long, value_name = "path", help = "Path to pipe stderr to")]
    pipe_stderr: Option<String>,

    #[clap(long, value_name = "path", help = "Path to pipe stdout to")]
    pipe_stdout: Option<String>,

    #[clap(
        global = true,
        short,
        long,
        help = "Do not show warnings or progress running commands besides `cargo check` and \
                `cargo fix`"
    )]
    quiet: bool,

    #[clap(long, hide = true)]
    rust_version: Option<String>,

    #[clap(long = "upgrade", hide = true)]
    upgrade_path: Option<String>,

    #[clap(long, help = "Check all packages in the workspace")]
    workspace: bool,

    #[clap(subcommand)]
    subcmd: Option<DynlintSubCommand>,

    #[clap(hide = true)]
    names: Vec<String>,

    #[clap(last = true, help = "Arguments for `cargo check`")]
    args: Vec<String>,
}

#[derive(Debug, Parser)]
enum DynlintSubCommand {
    #[clap(
        about = "List libraries or lints",
        long_about = "If no libraries are named, list the name, toolchain, and location of all \
discovered libraries.

If at least one library is named, list the name, level, and description of all lints in all named \
libraries.

Combine with `--all` to list all lints in all discovered libraries."
    )]
    List {
        #[clap(flatten)]
        name_opts: NameOpts,
    },

    #[clap(
        about = "Create a new library package",
        long_about = "Create a new library package at <PATH>"
    )]
    New {
        #[clap(long, help = "Put the package in its own workspace")]
        isolate: bool,

        #[clap(help = "Path to library package")]
        path: String,
    },

    #[clap(
        about = "Upgrade library package",
        long_about = "Upgrade the library package at <PATH> to the latest version of \
                      `clippy_utils`"
    )]
    Upgrade {
        #[clap(long, hide = true)]
        allow_downgrade: bool,

        #[clap(
            long,
            help = "Unix only/experimental: Update dependencies and search for the most recent \
                    applicable toolchain"
        )]
        bisect: bool,

        #[clap(
            long,
            value_name = "version",
            help = "Upgrade to the version of `clippy_utils` with tag `rust-<version>`"
        )]
        rust_version: Option<String>,

        #[clap(help = "Path to library package")]
        path: String,
    },
}

#[derive(Debug, Parser)]
struct NameOpts {
    #[clap(long, help = "Load all discovered libraries")]
    all: bool,

    #[clap(
        action = ArgAction::Append,
        number_of_values = 1,
        long = "lib",
        value_name = "name",
        help = "Library name to load lints from. A file with a name of the form \"DLL_PREFIX \
        <name> '@' TOOLCHAIN DLL_SUFFIX\" is searched for in the directories listed in \
        DYNLINT_LIBRARY_PATH, and in the `target/release` directories produced by building the \
        current workspace's metadata entries (see example below)."
    )]
    libs: Vec<String>,

    #[clap(long, help = "Do not build metadata entries")]
    no_build: bool,

    #[clap(long, help = "Ignore metadata entirely")]
    no_metadata: bool,

    #[clap(
        action = ArgAction::Append,
        number_of_values = 1,
        long = "path",
        value_name = "path",
        help = "Library path to load lints from"
    )]
    paths: Vec<String>,
}

#[allow(deprecated)]
impl From<Dynlint> for dynlint::Dynlint {
    fn from(opts: Dynlint) -> Self {
        let opts = process_deprecated_options(opts);
        let Dynlint {
            name_opts:
                NameOpts {
                    all,
                    libs,
                    no_build,
                    no_metadata,
                    paths,
                },
            allow_downgrade,
            bisect,
            fix,
            force,
            isolate,
            keep_going,
            list,
            manifest_path,
            new_path,
            no_deps,
            packages,
            pipe_stderr,
            pipe_stdout,
            quiet,
            rust_version,
            upgrade_path,
            workspace,
            subcmd: _,
            names,
            args,
        } = opts;
        Self {
            all,
            allow_downgrade,
            bisect,
            fix,
            force,
            isolate,
            keep_going,
            libs,
            list,
            manifest_path,
            new_path,
            no_build,
            no_deps,
            no_metadata,
            packages,
            paths,
            pipe_stderr,
            pipe_stdout,
            quiet,
            rust_version,
            upgrade_path,
            workspace,
            names,
            args,
        }
    }
}

fn process_deprecated_options(mut opts: Dynlint) -> Dynlint {
    if opts.list {
        dynlint::__warn(
            &dynlint::Dynlint::default(),
            "`--list` is deprecated. Use subcommand `list`.",
        );
    }
    if opts.new_path.is_some() {
        dynlint::__warn(
            &dynlint::Dynlint::default(),
            "`--new` is deprecated. Use subcommand `new`.",
        );
    }
    if opts.upgrade_path.is_some() {
        dynlint::__warn(
            &dynlint::Dynlint::default(),
            "`--upgrade` is deprecated. Use subcommand `upgrade`.",
        );
    }
    if !opts.names.is_empty() {
        dynlint::__warn(
            &dynlint::Dynlint::default(),
            "Referring to libraries by bare name is deprecated. Use `--lib` or `--path`.",
        );
    }
    if let Some(subcmd) = opts.subcmd.take() {
        match subcmd {
            DynlintSubCommand::List { name_opts } => {
                opts.name_opts.absorb(name_opts);
                opts.list = true;
            }
            DynlintSubCommand::New { isolate, path } => {
                opts.isolate |= isolate;
                opts.new_path = Some(path);
            }
            DynlintSubCommand::Upgrade {
                allow_downgrade,
                bisect,
                rust_version,
                path,
            } => {
                opts.allow_downgrade |= allow_downgrade;
                opts.bisect |= bisect;
                opts.rust_version = rust_version;
                opts.upgrade_path = Some(path);
            }
        }
    }
    opts
}

impl NameOpts {
    pub fn absorb(&mut self, other: Self) {
        self.all |= other.all;
        self.libs.extend(other.libs);
        self.no_build |= other.no_build;
        self.no_metadata |= other.no_metadata;
        self.paths.extend(other.paths);
    }
}

fn main() -> dynlint::ColorizedResult<()> {
    env_logger::try_init().unwrap_or_else(|error| {
        dynlint::__warn(
            &dynlint::Dynlint::default(),
            &format!("`env_logger` already initialized: {error}"),
        );
    });

    let args: Vec<_> = std::env::args().map(OsString::from).collect();

    cargo_dynlint(&args)
}

fn cargo_dynlint<T: AsRef<OsStr>>(args: &[T]) -> dynlint::ColorizedResult<()> {
    match Opts::parse_from(args).subcmd {
        CargoSubCommand::Dynlint(opts) => dynlint::run(&dynlint::Dynlint::from(opts)),
    }
    .map_err(dynlint::ColorizedError::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::prelude::*;
    use clap::CommandFactory;
    use predicates::prelude::*;

    #[test]
    fn verify_cli() {
        Opts::command().debug_assert();
    }

    #[test]
    fn usage() {
        std::process::Command::cargo_bin("cargo-dynlint")
            .unwrap()
            .args(["dynlint", "--help"])
            .assert()
            .success()
            .stdout(predicates::str::contains("Usage: cargo dynlint"));
    }

    #[test]
    fn version() {
        std::process::Command::cargo_bin("cargo-dynlint")
            .unwrap()
            .args(["dynlint", "--version"])
            .assert()
            .success()
            .stdout(format!("cargo-dynlint {}\n", env!("CARGO_PKG_VERSION")));
    }

    /// `no_env_logger_warning` fails if [`std::process::Command::new`] is replaced with
    /// [`assert_cmd::cargo::CommandCargoExt::cargo_bin`]. I don't understand why.
    ///
    /// [`assert_cmd::cargo::CommandCargoExt::cargo_bin`]: https://docs.rs/assert_cmd/latest/assert_cmd/cargo/trait.CommandCargoExt.html#tymethod.cargo_bin
    /// [`std::process::Command::new`]: https://doc.rust-lang.org/std/process/struct.Command.html#method.new
    #[test]
    fn no_env_logger_warning() {
        std::process::Command::new("cargo")
            .args(["run", "--bin", "cargo-dynlint"])
            // std::process::Command::cargo_bin("cargo-dynlint").unwrap()
            .assert()
            .failure()
            .stderr(predicates::str::contains("`env_logger` already initialized").not());
    }
}
