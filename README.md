# cargo-manifmt

[rustfmt] for your [Cargo.toml].

⚠️**Warning: cargo-manifmt is still under active development.** ⚠️

Running cargo-manifmt may silently corrupt your Cargo.toml file. Commit before
running cargo-manifmt, and inspect the resulting diff by hand.

## Usage

Installation:

```shell
$ cargo install cargo-manifmt
```

Then, from within a Cargo workspace, run:

```shell
$ cargo manifmt
```

All Cargo.toml manifests within the workspace will be reformatted in place
according to cargo-manifmt's hardcoded style guide. There are intentionally
no configuration options.

## Features

* Sorts package metadata into a consistent order that places the most important
  keys at the time.
* Sorts dependencies alphabetically within each group.
* Rewrites standard "caret" version contraints to be fully-specified, e.g.,
  rewrites `foo-dep = "1"` to `foo-dep = "1.0.0"`.
* Elides keys whose values are the default.
* Elides targets that can be automatically inferred from the repository layout.

## Limitations

* Comments are only preserved if they appear on their own lines above an entry
  in a features or dependencies table. If you have comments elsewhere in your
  Cargo.toml, cargo-manifmt will silently remove them!

* cargo-manifmt does not yet understand all entries in a Cargo.toml, and may
  inadvertently remove configuration it does not understand. This is a bug,
  of course, so please file an issue!

[Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
[rustfmt]: https://github.com/rust-lang/rustfmt
