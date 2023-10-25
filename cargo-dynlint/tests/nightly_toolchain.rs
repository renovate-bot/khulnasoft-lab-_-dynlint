// smoelius: On Windows, `rustup update nightly` generates "could not create link" errors similar to
// this one: https://github.com/rust-lang/rustup/issues/1316
#![cfg(not(target_os = "windows"))]

use anyhow::Result;
use dynlint_internal::Command;

#[test]
fn nightly_toolchain() {
    update_nightly().unwrap();

    #[allow(let_underscore_drop)]
    let _ = dynlint::driver_builder::get(&dynlint::Dynlint::default(), "nightly").unwrap();
}

fn update_nightly() -> Result<()> {
    Command::new("rustup")
        .args(["update", "--no-self-update", "nightly"])
        .success()
}
