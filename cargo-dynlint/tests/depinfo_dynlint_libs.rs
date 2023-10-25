use assert_cmd::prelude::*;
use dynlint_internal::env;
use predicates::prelude::*;
use std::{env::set_var, process::Command};

#[ctor::ctor]
fn initialize() {
    set_var(env::CARGO_TERM_COLOR, "never");
}

#[test]
fn depinfo_dynlint_libs() {
    Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir("fixtures/depinfo_dynlint_libs")
        .args(["dynlint", "--lib", "question_mark_in_expression"])
        .assert()
        .stderr(predicate::str::contains(
            "\nwarning: using the `?` operator within an expression\n",
        ));

    Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir("fixtures/depinfo_dynlint_libs")
        .args(["dynlint", "--lib", "try_io_result"])
        .assert()
        .stderr(predicate::str::contains(
            "\nwarning: returning a `std::io::Result` could discard relevant context (e.g., files \
             or paths involved)\n",
        ));
}
