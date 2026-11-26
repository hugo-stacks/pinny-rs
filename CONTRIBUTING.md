# Contributing

## Dependencies

Pinny is a proc-macro crate: every crate that uses it also compiles everything
Pinny depends on. Keep the dependency tree as small as possible.

- Prefer a few lines of standard-library code over a new crate. For example,
  `ConfigError` implements `Display` and `From` by hand rather than with
  `thiserror`, and tags are validated by `is_valid_tag` rather than with
  `regex`.
- When a crate is needed, disable its default features and enable only the
  ones in use. `toml` is built with just `parse`, which keeps `serde` out.
- The same applies to dev-dependencies. They do not reach consumers, but they
  still grow `Cargo.lock` and the set of crates to audit. For example, the
  config tests create their temporary directories with a small `TempDir`
  helper rather than with `tempfile`.
- Write tests so they can run in parallel: pass paths as arguments instead of
  setting environment variables, so no crate is needed to serialize them.
- A pull request that adds a dependency should explain why the standard
  library is not enough.
