#! /bin/bash

# shellcheck disable=SC2001

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

SCRIPTS="$(dirname "$(realpath "$0")")"
WORKSPACE="$(realpath "$SCRIPTS"/..)"

cd "$WORKSPACE"

cargo build -p cargo-dynlint
CARGO_DYNLINT="$PWD/target/debug/cargo-dynlint"

EXAMPLE_DIRS="$(find examples -mindepth 2 -maxdepth 2 -type d ! -name .cargo ! -name src)"

# smoelius: Remove `experimental` examples.
EXAMPLE_DIRS="$(echo "$EXAMPLE_DIRS" | sed 's,\<examples/experimental/[^/]*\>[[:space:]]*,,')"

# smoelius: `clippy` must be run separately because, for any lint not loaded alongside of it, rustc
# complains about the clippy-specific flags.
EXAMPLE_DIRS="$(echo "$EXAMPLE_DIRS" | sed 's,\<examples/testing/clippy\>[[:space:]]*,,')"

# smoelius: Remove `straggler`, as it uses a different toolchain than the other examples.
EXAMPLE_DIRS="$(echo "$EXAMPLE_DIRS" | sed 's,\<examples/testing/straggler\>[[:space:]]*,,')"

EXAMPLES="$(echo "$EXAMPLE_DIRS" | xargs -n 1 basename | tr '\n' ' ')"

RESTRICTION_DIRS="$(echo examples/restriction/*)"
RESTRICTIONS="$(echo "$RESTRICTION_DIRS" | xargs -n 1 basename | tr '\n' ' ')"

EXPERIMENTAL_DIRS="$(echo examples/experimental/*)"

# smoelius: `overscoped_allow` must be run after other lints have been run. (See its documentation.)
EXAMPLES="$(echo "$EXAMPLES" | sed 's/\<overscoped_allow\>[[:space:]]*//')"
RESTRICTIONS="$(echo "$RESTRICTIONS" | sed 's/\<overscoped_allow\>[[:space:]]*//')"

RESTRICTIONS_AS_FLAGS="$(echo "$RESTRICTIONS" | sed 's/\<[^[:space:]]\+\>/--lib &/g')"

DIRS=". driver utils/linting examples/general examples/supplementary examples/testing/clippy $RESTRICTION_DIRS $EXPERIMENTAL_DIRS"

force_check() {
    find "$WORKSPACE"/target -name .fingerprint -path '*/dynlint/target/nightly-*' -exec rm -r {} \; || true
}

for FLAGS in "--lib general --lib supplementary $RESTRICTIONS_AS_FLAGS" '--lib clippy'; do
    # smoelius: `cargo clean` can't be used here because it would remove cargo-dynlint.
    # smoelius: The commented command doesn't do anything now that all workspaces in the
    # repository share a top-level target directory. Is the command still necessary?
    # smoelius: Yes, the next command is necessary to force `cargo check` to run.
    force_check

    DYNLINT_RUSTFLAGS='-D warnings'
    if [[ "$FLAGS" = '--lib clippy' ]]; then
        # smoelius: `-W clippy::all` helps for checking those lints with `overscoped_allow`.
        DYNLINT_RUSTFLAGS="$DYNLINT_RUSTFLAGS
            -W clippy::all
            -W clippy::pedantic
            -W clippy::nursery
            -A clippy::option-if-let-else
            -A clippy::missing-errors-doc
            -A clippy::missing-panics-doc
            -A clippy::significant-drop-tightening
        "
    fi
    export DYNLINT_RUSTFLAGS
    echo "DYNLINT_RUSTFLAGS='$DYNLINT_RUSTFLAGS'"

    # smoelius: `--all-targets` cannot be used here. It would cause the command to fail on the
    # lint examples.
    COMMAND="$CARGO_DYNLINT dynlint $FLAGS -- --all-features --tests"

    for DIR in $DIRS; do
        pushd "$DIR"
        bash -c "$COMMAND"
        popd
    done

    force_check

    # smoelius: For `overscoped_allow`.
    DYNLINT_RUSTFLAGS="$(echo "$DYNLINT_RUSTFLAGS" | sed 's/-D warnings\>//g;s/-W\>/--force-warn/g')"
    if [[ "$FLAGS" != '--lib clippy' ]]; then
        DYNLINT_RUSTFLAGS="$DYNLINT_RUSTFLAGS $(echo "$EXAMPLES" | sed 's/\<[^[:space:]]\+\>/--force-warn &/g')"
    fi
    export DYNLINT_RUSTFLAGS
    echo "DYNLINT_RUSTFLAGS='$DYNLINT_RUSTFLAGS'"

    find . -name warnings.json -delete

    for DIR in $DIRS; do
        pushd "$DIR"
        bash -c "$COMMAND --message-format=json" >> warnings.json
        popd
    done

    force_check

    DYNLINT_RUSTFLAGS='-D warnings'
    export DYNLINT_RUSTFLAGS
    echo "DYNLINT_RUSTFLAGS='$DYNLINT_RUSTFLAGS'"

    # smoelius: All libraries must be named to enable their respective `cfg_attr`.
    COMMAND="$CARGO_DYNLINT dynlint $FLAGS --lib overscoped_allow -- --all-features --tests"

    for DIR in $DIRS; do
        pushd "$DIR"
        bash -c "$COMMAND"
        popd
    done
done
