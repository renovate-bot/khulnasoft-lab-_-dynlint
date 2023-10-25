# dynlint-link

`dynlint-link` is a wrapper around Rust's default linker (`cc`) to help create [Dynlint] libraries.

When you link a dynamic library with the same name as your package, `dynlint-link` creates a copy of your library with a filename that Dynlint recognizes, i.e.:

```
DLL_PREFIX LIBRARY_NAME '@' TOOLCHAIN DLL_SUFFIX
```

To use `dynlint-link`, install it:

```sh
cargo-install dynlint-link
```

And set it as the linker in your library's `.cargo/config.toml` file, e.g.:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "dynlint-link"
```

If your library uses `dynlint-link` and the [`dynlint_library!`] macro, then all you should have to do is implement the [`register_lints`] function. See the [examples] in this repository.

[`dynlint_library!`]: ../utils/linting
[`register_lints`]: https://doc.rust-lang.org/nightly/nightly-rustc/rustc_interface/interface/struct.Config.html#structfield.register_lints
[dynlint]: ..
[examples]: ../examples
