## Release Checklist

- Update `README.md`'s Usage section with the output of `xh --help`
- Update `CHANGELOG.md` (rename unreleased header to the current date, add any missing changes).
- Run `cargo update` to update dependencies.
- Bump up the version in `Cargo.toml` and run `cargo check` to update `Cargo.lock`.
- Run the following to update shell-completion files and man pages.
  ```sh
  cargo run --features=native-tls -- --generate complete-bash > completions/xh.bash
  cargo run --features=native-tls -- --generate complete-elvish > completions/xh.elv
  cargo run --features=native-tls -- --generate complete-fish > completions/xh.fish
  cargo run --features=native-tls -- --generate complete-nushell > completions/xh.nu
  cargo run --features=native-tls -- --generate complete-powershell > completions/_xh.ps1
  cargo run --features=native-tls -- --generate complete-zsh > completions/_xh
  cargo run --features=native-tls -- --generate man > doc/xh.1
  ```
- Commit changes and push them to remote.
- Add git tag e.g `git tag v0.9.0`.
- Push the local tags to remote i.e `git push --tags` which will start the CI release action.
- Publish to crates.io by running `cargo publish`.
