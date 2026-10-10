# Example modpack

A Cargo workspace that compiles the IronPumpkin server with the native mods of the pack.
`mods/hello-mod` registers `/hello`, which answers with the mod id, and three kinds of content
through the builders of `ironpumpkin_mods::content`:

| Name | Kind | Vanilla clients see |
|:--|:--|:--|
| `hello-mod:greeter_lamp` | block, with a `lit` property | `minecraft:redstone_lamp` |
| `hello-mod:greeter_lamp` | item that places the block | `minecraft:redstone_lamp` |
| `hello-mod:greeter` | entity type | `minecraft:pig` |

- `src/main.rs` references each mod crate (`use hello_mod as _;`) and calls `pumpkin::run()`.
  A mod crate that the binary does not reference is not linked, and its mod does not load.
- `mods/` holds the mod crates. Each one is a workspace member and a dependency of the binary.
- `client-mod/` is the NeoForge 26.3 Java mod that a NeoForge client installs to join this pack:
  it registers the content of `hello-mod` under the same names (see its README).
- A mod crate depends on `ironpumpkin-mods` only. That crate re-exports the server types a mod
  uses: `ironpumpkin_mods::command`, `::event`, `::text`, `::permission`, `::world` and the
  others listed in its crate documentation.

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

The log shows `[ironpumpkin] loaded 1 native mod: hello-mod (client_required = true)`. Type
`hello` on the console to run the command. The content phase writes the block, the item and the
entity type to `<world>/ironpumpkin/content_registry.json`.
