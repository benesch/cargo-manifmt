# cargo-manifmt

[rustfmt] for your [Cargo.toml].

⚠️**Warning: cargo-manifmt is still under active development.** ⚠️

Running cargo-manifmt may silently corrupt your Cargo.toml file. Commit first,
and inspect the resulting diff by hand.

## Usage

Installation:

```shell
$ cargo install cargo-manifmt
```

Then, from within a Cargo workspace, run:

```shell
$ cargo tomlfmt
```

All Cargo.toml manifests within the workspace will be reformatted in place
according to cargo-manifmt's hardcoded style guide. There are intentionally
no configuration options.

## Limitations

* Comments are not preserved. If you have comments in your Cargo.toml,
  cargo-manifmt will remove them.

* cargo-manifmt does not yet understand all entries in a Cargo.toml, and may
  inadvertently remove configuration it does not understand. This is a bug,
  of course, so please file an issue!

[Cargo.toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
[rustfmt]: https://github.com/rust-lang/rustfmt
