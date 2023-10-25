use dynlint_internal::{cargo::cargo_home, env};
use std::{fs::OpenOptions, io::Write, path::Path};

fn main() {
    let cargo_home = cargo_home().unwrap();
    let out_dir = env::var(env::OUT_DIR).unwrap();

    #[allow(unknown_lints, env_cargo_path)]
    let dynlint_manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let dynlint_driver_manifest_dir = if dynlint_manifest_dir.starts_with(cargo_home)
        || dynlint_manifest_dir
            .parent()
            .map_or(false, |path| path.ends_with("target/package"))
        || env::var(env::DOCS_RS).is_ok()
    {
        "None".to_owned()
    } else {
        let path_buf = dynlint_manifest_dir.join("../driver");

        // smoelius: Ensure the path exists at build time.
        assert!(path_buf.is_dir(), "{path_buf:?} is not a directory");

        format!(
            r#"Some("{}")"#,
            path_buf.to_string_lossy().replace('\\', "\\\\")
        )
    };

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(Path::new(&out_dir).join("dynlint_driver_manifest_dir.rs"))
        .unwrap();
    writeln!(
        file,
        "const DYNLINT_DRIVER_MANIFEST_DIR: Option<&str> = {dynlint_driver_manifest_dir};"
    )
    .unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
