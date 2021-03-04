## Release Checklist

- Run `generate.sh` to update man pages and shell-completion files.
- Update `CHANGELOG.md` (rename unreleased header to the current date, add any missing changes).
- Bump up the version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
- Commit changes and push them to remote.
- Add git tag e.g `git tag v0.9.0`.
- Push the local tags to remote i.e git push --tags which will start the CI release action.
- publish to crates.io by running `cargo publish`.
