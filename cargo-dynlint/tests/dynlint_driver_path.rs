use dynlint_internal::{
    driver as dynlint_driver, env,
    rustup::{toolchain_path, SanitizeEnvironment},
    testing::new_template,
};
use std::fs::create_dir_all;
use tempfile::tempdir_in;

#[test]
fn dynlint_driver_path() {
    let tempdir = tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();

    new_template(tempdir.path()).unwrap();

    let dynlint_driver_path = tempdir.path().join("target/dynlint_drivers");

    create_dir_all(&dynlint_driver_path).unwrap();

    dynlint_internal::cargo::test("dynlint-template", false)
        .sanitize_environment()
        .current_dir(&tempdir)
        .envs([(env::DYNLINT_DRIVER_PATH, &*dynlint_driver_path)])
        .success()
        .unwrap();

    // smoelius: Verify that the driver can be run directly.
    // https://github.com/khulnasoft-lab/dynlint/issues/54
    let toolchain_path = toolchain_path(tempdir.path()).unwrap();
    let toolchain = toolchain_path.iter().last().unwrap();
    let mut command = dynlint_driver(
        &toolchain.to_string_lossy(),
        &dynlint_driver_path.join(toolchain).join("dynlint-driver"),
    )
    .unwrap();
    command.success().unwrap();
}
