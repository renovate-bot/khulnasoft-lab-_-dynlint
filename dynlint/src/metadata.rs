use crate::{
    error::warn,
    toml::{self, DetailedTomlDependency},
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use cargo::{
    core::{
        source::MaybePackage, Dependency, Features, Package as CargoPackage, PackageId, QueryKind,
        Source, SourceId,
    },
    util::Config,
};
use cargo_metadata::{Error, Metadata, MetadataCommand, Package as MetadataPackage};
use dynlint_internal::{env, library_filename, rustup::SanitizeEnvironment};
use glob::glob;
use if_chain::if_chain;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::path::{Path, PathBuf};

type Object = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Debug)]
pub struct Package {
    metadata: &'static Metadata,
    pub root: PathBuf,
    pub id: PackageId,
    pub lib_name: String,
    pub toolchain: String,
}

impl Eq for Package {}

impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        (&self.root, &self.id, &self.lib_name, &self.toolchain)
            == (&other.root, &other.id, &other.lib_name, &other.toolchain)
    }
}

impl Ord for Package {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.root, &self.id, &self.lib_name, &self.toolchain).cmp(&(
            &other.root,
            &other.id,
            &other.lib_name,
            &other.toolchain,
        ))
    }
}

impl PartialOrd for Package {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Package {
    pub fn target_directory(&self) -> PathBuf {
        self.metadata
            .target_directory
            .join("dynlint/libraries")
            .join(&self.toolchain)
            .into_std_path_buf()
    }

    pub fn path(&self) -> PathBuf {
        self.target_directory()
            .join("release")
            .join(library_filename(&self.lib_name, &self.toolchain))
    }
}

#[derive(Debug, Deserialize)]
struct Library {
    pattern: Option<String>,
    #[serde(flatten)]
    details: DetailedTomlDependency,
}

pub fn workspace_metadata_packages(opts: &crate::Dynlint) -> Result<Vec<Package>> {
    if_chain! {
        if let Some(metadata) = cargo_metadata(opts)?;
        if let Some(object) = dynlint_metadata(opts)?;
        then {
            dynlint_metadata_packages(opts, metadata, object)
        } else {
            Ok(vec![])
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub fn dynlint_metadata(opts: &crate::Dynlint) -> Result<Option<&'static Object>> {
    if_chain! {
        if let Some(metadata) = cargo_metadata(opts)?;
        if let serde_json::Value::Object(object) = &metadata.workspace_metadata;
        if let Some(value) = object.get("dynlint");
        then {
            if let serde_json::Value::Object(subobject) = value {
                Ok(Some(subobject))
            } else {
                bail!("`dynlint` value must be a map")
            }
        } else {
            Ok(None)
        }
    }
}

static CARGO_METADATA: OnceCell<Option<Metadata>> = OnceCell::new();

fn cargo_metadata(opts: &crate::Dynlint) -> Result<Option<&'static Metadata>> {
    CARGO_METADATA
        .get_or_try_init(|| {
            if opts.no_metadata {
                return Ok(None);
            }

            let mut command = MetadataCommand::new();

            if let Some(path) = &opts.manifest_path {
                command.manifest_path(path);
            }

            if let Some(path) = &opts.manifest_path {
                command.manifest_path(path);
            }

            match command.exec() {
                Ok(metadata) => Ok(Some(metadata)),
                Err(err) => {
                    if opts.manifest_path.is_none() {
                        if_chain! {
                            if let Error::CargoMetadata { stderr } = err;
                            if let Some(line) = stderr.lines().next();
                            if !line.starts_with("error: could not find `Cargo.toml`");
                            then {
                                warn(opts, line.strip_prefix("error: ").unwrap_or(line));
                            }
                        }
                        Ok(None)
                    } else {
                        Err(err.into())
                    }
                }
            }
        })
        .map(Option::as_ref)
}

fn dynlint_metadata_packages(
    opts: &crate::Dynlint,
    metadata: &'static Metadata,
    object: &Object,
) -> Result<Vec<Package>> {
    let libraries = object
        .iter()
        .map(|(key, value)| {
            if key == "libraries" {
                let libraries = serde_json::from_value::<Vec<Library>>(value.clone())?;
                library_packages(opts, metadata, &libraries)
            } else {
                bail!("Unknown key `{}`", key)
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(libraries.into_iter().flatten().collect())
}

fn library_packages(
    opts: &crate::Dynlint,
    metadata: &'static Metadata,
    libraries: &[Library],
) -> Result<Vec<Package>> {
    let config = Config::default()?;

    let packages = libraries
        .iter()
        .map(|library| library_package(opts, metadata, &config, library))
        .collect::<Result<Vec<_>>>()
        .with_context(|| "Could not build metadata entries")?;

    Ok(packages.into_iter().flatten().collect())
}

fn library_package(
    opts: &crate::Dynlint,
    metadata: &'static Metadata,
    config: &Config,
    library: &Library,
) -> Result<Vec<Package>> {
    let dep = dependency(opts, metadata, config, library)?;

    // smoelius: The dependency root cannot be canonicalized here. It could contain a `glob` pattern
    // (e.g., `*`), because Dynlint allows `path` entries to contain `glob` patterns.
    let dependency_root = dependency_root(config, &dep)?;

    let pattern = if let Some(pattern) = &library.pattern {
        dependency_root.join(pattern)
    } else {
        #[allow(clippy::redundant_clone)]
        dependency_root.clone()
    };

    let entries = glob(&pattern.to_string_lossy())?;

    let paths = entries
        .map(|entry| {
            entry.map_err(Into::into).and_then(|path| {
                if let Some(pattern) = &library.pattern {
                    let path_buf = path
                        .canonicalize()
                        .with_context(|| format!("Could not canonicalize {path:?}"))?;
                    // smoelius: On Windows, the dependency root must be canonicalized to ensure it
                    // has a path prefix.
                    let dependency_root = dependency_root
                        .canonicalize()
                        .with_context(|| format!("Could not canonicalize {dependency_root:?}"))?;
                    ensure!(
                        path_buf.starts_with(&dependency_root),
                        "Pattern `{pattern}` could refer to `{}`, which is outside of `{}`",
                        path_buf.to_string_lossy(),
                        dependency_root.to_string_lossy()
                    );
                }
                Ok(path)
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    ensure!(
        !paths.is_empty(),
        "No paths matched `{}`",
        pattern.to_string_lossy()
    );

    // smoelius: Collecting the package ids before building reveals missing/unparsable `Cargo.toml`
    // files sooner.

    // smoelius: Why are we doing this complicated dance at all? Because we want to leverage Cargo's
    // download cache. But we also want to support git repositories with libraries that use
    // different compiler versions. And we have to work around the fact that "all projects within a
    // workspace are intended to be built with the same version of the compiler"
    // (https://github.com/rust-lang/rustup/issues/1399#issuecomment-383376082).

    // smoelius: Experiments suggest that a considerable amount of Dynlint's start up time is spent
    // in the following "loop," and a considerable (though not necessarily dominant) fraction of
    // that is spent in `active_toolchain`.
    let packages = paths
        .into_iter()
        .map(|path| {
            if path.is_dir() {
                // smoelius: Ignore subdirectories that do not contain packages.
                let Ok(package) = package_with_root(&path) else {
                    return Ok(None);
                };
                let package_id = package_id(&package, dep.source_id())?;
                let lib_name = package_library_name(&package)?;
                let toolchain = dynlint_internal::rustup::active_toolchain(&path)?;
                Ok(Some(Package {
                    metadata,
                    root: path,
                    id: package_id,
                    lib_name,
                    toolchain,
                }))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(packages.into_iter().flatten().collect())
}

fn dependency(
    opts: &crate::Dynlint,
    metadata: &Metadata,
    config: &Config,
    library: &Library,
) -> Result<Dependency> {
    let mut unused_keys = library.details.unused_keys();
    #[allow(clippy::format_collect)]
    if !unused_keys.is_empty() {
        unused_keys.sort_unstable();
        bail!(
            "Unknown library keys:{}",
            unused_keys
                .iter()
                .map(|name| format!("\n    {name}"))
                .collect::<String>()
        );
    }

    let name_in_toml = "library";

    let mut deps = vec![];
    let root = PathBuf::from(&metadata.workspace_root);
    let source_id = SourceId::for_path(&root)?;
    let mut nested_paths = vec![];
    let mut warnings = vec![];
    let features = Features::new(&[], config, &mut warnings, source_id.is_path())?;
    let mut cx = toml::Context::new(
        &mut deps,
        source_id,
        &mut nested_paths,
        config,
        &mut warnings,
        None,
        &root,
        &features,
    );

    let kind = None;

    let dependency = library.details.to_dependency(name_in_toml, &mut cx, kind)?;

    if !warnings.is_empty() {
        warn(opts, &warnings.join("\n"));
    }

    Ok(dependency)
}

fn dependency_root(config: &Config, dep: &Dependency) -> Result<PathBuf> {
    let source_id = dep.source_id();

    if source_id.is_path() {
        if let Some(path) = source_id.local_path() {
            Ok(path)
        } else {
            bail!("Path source should have a local path: {}", source_id)
        }
    } else if source_id.is_git() {
        git_dependency_root(config, dep)
    } else {
        bail!("Only git and path entries are supported: {}", source_id)
    }
}

fn git_dependency_root(config: &Config, dep: &Dependency) -> Result<PathBuf> {
    let _lock = config.acquire_package_cache_lock()?;

    #[allow(clippy::default_trait_access)]
    let mut source = dep.source_id().load(config, &Default::default())?;

    let package_id = sample_package_id(dep, &mut *source)?;

    if let MaybePackage::Ready(package) = source.download(package_id)? {
        git_dependency_root_from_package(config, &*source, &package)
    } else {
        bail!(format!("`{}` is not ready", package_id.name()))
    }
}

#[cfg_attr(dynlint_lib = "general", allow(non_local_effect_before_error_return))]
#[cfg_attr(dynlint_lib = "overscoped_allow", allow(overscoped_allow))]
fn sample_package_id(dep: &Dependency, source: &mut dyn Source) -> Result<PackageId> {
    let mut package_id: Option<PackageId> = None;

    while {
        let poll = source.query(dep, QueryKind::Fuzzy, &mut |summary| {
            if package_id.is_none() {
                package_id = Some(summary.package_id());
            }
        })?;
        if poll.is_pending() {
            source.block_until_ready()?;
            package_id.is_none()
        } else {
            false
        }
    } {}

    package_id.ok_or_else(|| anyhow!("Found no packages in `{}`", dep.source_id()))
}

fn git_dependency_root_from_package<'a>(
    config: &'a Config,
    source: &(dyn Source + 'a),
    package: &CargoPackage,
) -> Result<PathBuf> {
    let package_root = package.root();

    if source.source_id().is_git() {
        let git_path = config.git_path();
        let git_path = config.assert_package_cache_locked(&git_path);
        ensure!(
            package_root.starts_with(git_path.join("checkouts")),
            "Unexpected path: {}",
            package_root.to_string_lossy()
        );
        let n = git_path.components().count() + 3;
        Ok(package_root.components().take(n).collect())
    } else if source.source_id().is_path() {
        unreachable!()
    } else {
        unimplemented!()
    }
}

fn package_with_root(package_root: &Path) -> Result<MetadataPackage> {
    // smoelius: For the long term, we should investigate having a "cache" that maps paths to
    // `cargo_metadata::Metadata`.
    let metadata = MetadataCommand::new()
        .current_dir(package_root)
        .no_deps()
        .exec()?;

    dynlint_internal::cargo::package_with_root(&metadata, package_root)
}

fn package_id(package: &MetadataPackage, source_id: SourceId) -> Result<PackageId> {
    PackageId::new(&package.name, &package.version, source_id)
}

pub fn package_library_name(package: &MetadataPackage) -> Result<String> {
    package
        .targets
        .iter()
        .find_map(|target| {
            if target.kind.iter().any(|kind| kind == "cdylib") {
                Some(target.name.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            anyhow!(
                "Could not find `cdylib` target for package `{}`",
                package.id
            )
        })
}

pub fn build_library(opts: &crate::Dynlint, package: &Package) -> Result<PathBuf> {
    let target_dir = package.target_directory();

    let path = package.path();

    if !opts.no_build {
        // smoelius: Clear `RUSTFLAGS` so that changes to it do not cause workspace metadata entries
        // to be rebuilt.
        dynlint_internal::cargo::build(
            &format!("workspace metadata entry `{}`", package.id.name()),
            opts.quiet,
        )
        .sanitize_environment()
        .env_remove(env::RUSTFLAGS)
        .current_dir(&package.root)
        .args(["--release", "--target-dir", &target_dir.to_string_lossy()])
        .success()?;

        let exists = path
            .try_exists()
            .with_context(|| format!("Could not determine whether {path:?} exists"))?;

        ensure!(exists, "Could not find {path:?} despite successful build");
    }

    Ok(path)
}

// smoelius: `pkg_dir` and `target_short_hash` are based on functions with the same names in
// https://github.com/rust-lang/cargo/blob/master/src/cargo/core/compiler/context/compilation_files.rs

#[cfg(any())]
mod disabled {
    fn pkg_dir(package_root: &Path, pkg_id: PackageId) -> String {
        let name = pkg_id.name();
        format!("{}-{}", name, target_short_hash(package_root, pkg_id))
    }

    const METADATA_VERSION: u8 = 2;

    fn target_short_hash(package_root: &Path, pkg_id: PackageId) -> String {
        // smoelius: For now, the package root is the workspace root.
        let hashable = pkg_id.stable_hash(package_root);
        cargo::util::short_hash(&(METADATA_VERSION, hashable))
    }
}
