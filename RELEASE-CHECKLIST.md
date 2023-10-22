## Release Checklist

- Update `README.md`'s Usage section with the output of `xh --help`
- Update `CHANGELOG.md` (rename unreleased header to the current date, add any missing changes).
- Run `cargo update` to update dependencies.
- Bump up the version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
- Run the following to update man pages and shell-completion files.
  ```sh
  cargo run --all-features -- generate-completions completions && cargo run --all-features -- generate-manpages doc
  ```
- Commit changes and push them to remote.
- Add git tag e.g `git tag v0.9.0`.
- Push the local tags to remote i.e `git push --tags` which will start the CI release action.
- Publish to crates.io by running `cargo publish`.
