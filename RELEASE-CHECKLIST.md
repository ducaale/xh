## Release Checklist

- Update `README.md`'s Usage section with the output of `xh --help`
- Make sure that all options/flags have corresponding negation in the `NEGATION_FLAGS` list found
  in `cli.rs`. The following command should help in getting an up-to-date list.
  ```
  cargo expand --all-features cli | rg -o 'with_name\s*\("([^"]*)"\)' -r '    "--no-$1",' | rg -v 'raw-' | sort
  ```
- Update `CHANGELOG.md` (rename unreleased header to the current date, add any missing changes).
- Bump up the version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
- Run `generate.sh` to update man pages and shell-completion files.
- Commit changes and push them to remote.
- Add git tag e.g `git tag v0.9.0`.
- Push the local tags to remote i.e git push --tags which will start the CI release action.
- Publish to crates.io by running `cargo publish`.
