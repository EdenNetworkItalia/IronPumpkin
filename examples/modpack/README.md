# Example modpack

A Cargo workspace that compiles the IronPumpkin server with the native mods of the pack.
`mods/hello-mod` registers `/hello`, which answers with the mod id.

- `src/main.rs` references each mod crate (`use hello_mod as _;`) and calls `pumpkin::run()`.
  A mod crate that the binary does not reference is not linked, and its mod does not load.
- `mods/` holds the mod crates. Each one is a workspace member and a dependency of the binary.

## Build and run

Run cargo from this directory: `.cargo/config.toml` then points the build at the target
directory of the server workspace, so the server is not built a second time.

Seed the lock file from the server workspace first, so the dependency versions and the compiled
artifacts match. The lock file is not committed.

```sh
cp ../../Cargo.lock Cargo.lock
cargo build
cd /path/to/server/dir && /path/to/IronPumpkin/target/debug/example-modpack
```

The log shows `[ironpumpkin] loaded 1 native mod: hello-mod`. Type `hello` on the console to
run the command.
