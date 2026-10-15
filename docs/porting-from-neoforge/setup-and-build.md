# Setting up and building a pack

A NeoForge mod is a Gradle project. The `net.neoforged.moddev` plugin builds a jar, `neoforge.mods.toml` describes the jar, and a `@Mod` class starts it. A server owner copies the jar into the `mods/` folder.

An IronPumpkin mod is a Rust crate. A modpack is a repository that you create from the pack template. Its `modpack.toml` pins one IronPumpkin commit and lists the mod crates. `cargo xtask build` fetches IronPumpkin, applies the patches of the mods and compiles one server binary. The server owner runs that binary.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Project** | A Gradle project with `net.neoforged.moddev` | A Rust crate in a Cargo workspace, in a pack repository |
| **Mod descriptor** | `neoforge.mods.toml` | The methods of the `NativeMod` trait. An optional `neoforge.mods.toml` through `NativeMod::mods_toml()` replaces them. See [Lifecycle](./lifecycle.md). |
| **Entry point** | A class with `@Mod` | A type that implements `NativeMod`, and `register_mod!` |
| **Dependency on the game** | `neoforge` at a version range | `ironpumpkin-mods`, and one pinned IronPumpkin commit in `modpack.toml` |
| **Build tool** | `./gradlew build` | `cargo xtask check` and `cargo xtask build` |
| **Output** | A jar for each mod | One server binary for the whole pack |
| **Distribution** | A mod site, and the jar in `mods/` | A GitHub release of the pack, with one binary for each platform |
| **Init order** | Dependency order from the `ordering` of the dependencies | The load order: dependency order from `ordering`, with the mod id as tie-break. See [Lifecycle](./lifecycle.md). |
| **Licence** | The author chooses | GPL-3.0-compatible, because the binary links the server |

`NativeMod` and `mods_toml()` replace these keys of `neoforge.mods.toml`:

| `neoforge.mods.toml` | `NativeMod` | Notes |
| :--- | :--- | :--- |
| `modId` | `id()`, or the `modId` of `mods_toml()` | Without a `mods_toml()`, the id uses `a-z`, `0-9`, `_`, `.` and `-`. With one, the `modId` matches the FML 12.0.8 pattern `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`. Two mods with the same id stop the server. |
| `namespace` | The `namespace` of `mods_toml()` | The registry namespace. The default is the mod id. |
| `displayName` | `display_name()`, or the key of `mods_toml()` | |
| `version` | `version()`, or the key of `mods_toml()` | `${file.jarVersion}` in the file becomes `version()`. |
| A mod that clients need | `client_required()` | The default is `true`. A client without the mod is disconnected at the configuration handshake. |
| `description`, `authors`, `[[dependencies.<modId>]]` | The same keys of `mods_toml()` | A mod without a `mods_toml()` has no dependencies. |
| `license` | The same key of `mods_toml()`, and `license` in the `Cargo.toml` of the crate | A `mods_toml()` needs a non-blank `license`. A missing or blank license stops the server with "Missing license". The server does not keep the value. |

---

## Code Comparison: a minimal mod

### 1. NeoForge implementation (Java)

File `neoforge.mods.toml`:

```toml
license = "GPL-3.0"

[[mods]]
modId = "hello_mod"
version = "1.0.0"
displayName = "Hello Mod"

[[dependencies.hello_mod]]
modId = "neoforge"
type = "required"
versionRange = "[26.3.0.64-beta,)"
ordering = "NONE"
side = "BOTH"
```

```java
@Mod("hello_mod")
public final class HelloMod {
    public HelloMod(IEventBus modBus) {
        // Register the DeferredRegisters and the listeners here.
    }
}
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`. It is the mod that the steps below build.

File `mods/greeter/Cargo.toml`:

```toml
[package]
name = "greeter"
version = "0.1.0"
edition = "2024"
license = "GPL-3.0"
publish = false

[dependencies]
ironpumpkin-mods = { path = "../../.ironpumpkin/crates/ironpumpkin-mods" }
```

File `mods/greeter/src/lib.rs`:

```rust
use ironpumpkin_mods::{ModInit, NativeMod, register_mod, tracing::info};

struct Greeter;

impl NativeMod for Greeter {
    fn id(&self) -> &'static str {
        "greeter"
    }

    fn display_name(&self) -> &'static str {
        "Greeter"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    // A server-only mod: a client without it can join.
    fn client_required(&self) -> bool {
        false
    }

    fn init(&self, _cx: &mut ModInit) {
        info!("[greeter] init");
    }
}

register_mod!(Greeter);
```

---

## From the template to a running binary

You need git and Rust stable, version 1.96 or newer. A debug build of the pack compiles the whole server. A release build takes a long time and about 9 GB of disk. Cargo uses one `target/` directory for the whole pack.

### 1. Create the pack repository

Open `EdenNetworkItalia/ironpumpkin-pack-template` on GitHub and select **Use this template**. Name the new repository `my-pack`. Clone it:

```sh
git clone https://github.com/<you>/my-pack.git
cd my-pack
```

The template has no mod and no patch. Its files are:

| File | Use |
| :--- | :--- |
| `modpack.toml` | The pack name, the IronPumpkin commit and the list of mods |
| `Cargo.toml` | The workspace of the build tool. It has `exclude = [".ironpumpkin"]`. |
| `xtask/` | The build tool: `cargo xtask check`, `build` and `name` |
| `bin/Cargo.lock`, `bin/Cargo.lock.commit` | The lock file of the pack binary, and the IronPumpkin commit that it was seeded from |
| `.github/workflows/build.yml` | The `Build modpack` workflow |
| `.cargo/config.toml` | The `xtask` alias, and `target/` as the target directory |

### 2. Check the template

The template pins IronPumpkin commit `a2753a0ecd1c59e98dd00bc3a37f4db47923d690`. That commit has the content builders, the lifecycle, the config API and the events that this guide uses. A new pack keeps the pin. You change it only to [move to a newer IronPumpkin](#the-rules-of-a-pack).

```sh
cargo xtask check
```

The first run compiles the build tool. The tool then fetches IronPumpkin at the pinned commit into `.ironpumpkin/`, and it prints:

```text
[xtask] fetching IronPumpkin a2753a0ecd1c59e98dd00bc3a37f4db47923d690 from https://github.com/EdenNetworkItalia/IronPumpkin
[xtask] my-pack: IronPumpkin a2753a0ecd1c, 0 mod(s), 0 patch(es) applied
```

### 3. Add the mod

Create `mods/greeter/Cargo.toml` and `mods/greeter/src/lib.rs` with the files of the Code Comparison above. Add the mod to `modpack.toml`. The key is the package name of the crate:

File `modpack.toml`:

```toml
[mods.greeter]
path = "mods/greeter"
```

A mod has one source: `path`, `git` with `rev`, or `version` from crates.io. A `rev` is a commit hash or a tag, not a branch. A crates.io mod still depends on `ironpumpkin-mods` by git URL, because that crate is not on crates.io.

### 4. Check the pack and commit the lock files

```sh
cargo xtask check
```

```text
     Locking 1 package to latest Rust 1.98.1 compatible version
      Adding greeter v0.1.0 (/path/to/my-pack/mods/greeter)
[xtask] my-pack: IronPumpkin a2753a0ecd1c, 1 mod(s), 0 patch(es) applied
```

The `Locking` line names your Rust version, and the `Adding` line shows your path. The template commits `bin/Cargo.lock`, seeded from the IronPumpkin lock file of the pinned commit. The tool adds the dependencies of the mods to it. The tool also writes `bin/Cargo.toml` and `bin/src/mods.rs`. Git ignores both files. `bin/src/mods.rs` holds one `use greeter as _;` for each mod, so the linker keeps the mod.

Commit `bin/Cargo.lock` and `bin/Cargo.lock.commit` with `modpack.toml` and the mod:

```sh
git add modpack.toml mods bin/Cargo.lock bin/Cargo.lock.commit
git commit -m "feat: add the greeter mod"
```

### 5. Build

Build a debug binary to test the pack:

```sh
cargo xtask build --debug
```

The tool runs `cargo xtask check` first, then builds the pack binary. The first build compiles the whole server and takes several minutes. The build script of `pumpkin-data` also downloads the Minecraft server jar from Mojang and extracts the vanilla data pack into `.ironpumpkin/assets/datapack`. Cargo shows this as warnings. The build needs network access to `piston-meta.mojang.com` and `piston-data.mojang.com` for it.

```text
warning: pumpkin-data@0.2.0+26.3-26.51: Vanilla datapack missing at /path/to/my-pack/.ironpumpkin/assets/datapack/data/minecraft. Downloading Minecraft 26.3 server jar...
warning: pumpkin-data@0.2.0+26.3-26.51: Fetching version manifest from https://piston-meta.mojang.com/mc/game/version_manifest_v2.json...
warning: pumpkin-data@0.2.0+26.3-26.51: Fetching version package metadata from https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json...
warning: pumpkin-data@0.2.0+26.3-26.51: Downloading server jar from https://piston-data.mojang.com/v1/objects/33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c/server.jar...
warning: pumpkin-data@0.2.0+26.3-26.51: Extracting datapack from server jar...
warning: pumpkin-data@0.2.0+26.3-26.51: Datapack extraction complete.
    Finished `dev` profile [unoptimized] target(s) in 8m 11s
[xtask] built /path/to/my-pack/target/debug/my-pack
```

The `future version of Rust` warning of the build comes from a dependency of the server. Ignore it.

Build the release binary for a server that players use:

```sh
cargo xtask build
```

The release build uses `--locked`. It fails when the committed `bin/Cargo.lock` does not match `modpack.toml`. It also uses link-time optimization, so it takes longer: 17 minutes on a 24-thread workstation, with a warm cache for the dependencies from crates.io. The binary is about 134 MB.

```text
    Finished `release` profile [optimized] target(s) in 17m 41s
[xtask] built /path/to/my-pack/target/release/my-pack
```

### 6. Run

Run the binary in a server directory, as you run the IronPumpkin binary:

```sh
mkdir ../my-pack-server
cd ../my-pack-server
../my-pack/target/debug/my-pack
```

The first start writes `pumpkin.toml`, `world/`, `plugins/`, `data/` and `logs/` in the server directory. The log shows the mod. Timestamps and colours are left out here:

```text
INFO Starting IronPumpkin 0.2.0+26.3-26.51 Java Minecraft (Protocol 777) | 1.26.51 Bedrock (Protocol 2193)
INFO [greeter] init
INFO [ironpumpkin] loaded 1 native mod: greeter (client_required = false)
INFO Server is now running. Connect using port: Java Edition: 0.0.0.0:25565 | Bedrock Edition: 0.0.0.0:19132
```

The line `[ironpumpkin] loaded 1 native mod` proves that the binary links the mod. The `[greeter] init` line comes from `NativeMod::init`. Press Ctrl+C to stop the server. The release binary starts the same way and logs the same two mod lines.

### 7. Release

A tag that starts with `v` starts the `Build modpack` workflow. It builds the release binary on Linux and on Windows and publishes a GitHub release. The release has `<pack>-<tag>-x86_64-linux`, `<pack>-<tag>-x86_64-windows.exe` and `SHA256SUMS`. A pull request runs `cargo xtask check` and a debug build, and it fails when the committed `bin/Cargo.lock` does not match `modpack.toml`.

---

## The rules of a pack

**One copy of the server**

The binary must contain one copy of each server crate. The mods register in `ironpumpkin-mods`, and a second copy is one that the server never reads. A mod depends on `ironpumpkin-mods` by path into `.ironpumpkin/`, or by the IronPumpkin git URL:

```toml
ironpumpkin-mods = { git = "https://github.com/EdenNetworkItalia/IronPumpkin" }
```

The build points the server crates that the binary links at `.ironpumpkin/`. The build fails in two cases. A server crate resolves to more than one package, for example when a mod depends on IronPumpkin from a fork URL. A mod depends by git URL on a server crate that the binary does not link, such as `pumpkin-plugin-api`. Depend on `ironpumpkin-mods` instead. It re-exports the server types that a mod needs.

**The workspace `exclude`**

The root `Cargo.toml` has `exclude = [".ironpumpkin"]`. Keep that line. A path mod can have a package name that sorts before `pumpkin`, and `greeter` is such a name. Cargo then uses the root as the workspace of the IronPumpkin crates. Without the line, `cargo xtask check` fails with this error:

```text
error: failed to load manifest for dependency `pumpkin`

Caused by:
  failed to parse manifest at `/path/to/my-pack/.ironpumpkin/crates/pumpkin/Cargo.toml`

Caused by:
  error inheriting `edition` from workspace root manifest's `workspace.package.edition`

Caused by:
  `workspace.package.edition` was not defined
error: cargo metadata failed: exit status: 101
```

**Two orders**

The patches apply in the byte order of the keys of `modpack.toml`, which are the package names of the mod crates. The server calls `init` in the load order: dependency order, with the mod id as tie-break. The two orders are independent. See [Mixins, reflection and access transformers](./mixins-and-patches.md).

**The lock files**

`bin/Cargo.lock` makes the build reproducible, so it is committed. `cargo xtask build` (release) uses `--locked`. It fails when the committed lock does not match `modpack.toml`. Run `cargo xtask check` after each change of `modpack.toml`, of a mod `Cargo.toml` or of a patch, and commit the lock files.

**Move to a newer IronPumpkin**

Read the newest hash with `git ls-remote https://github.com/EdenNetworkItalia/IronPumpkin master`. Set `[ironpumpkin] commit` to a new full hash, run `cargo xtask check` and commit the two lock files. A patch is bound to one commit, so write it again for the new commit.

**Licence**

The IronPumpkin server is GPL-3.0. A built binary links the server, so it is a GPL-3.0 work. Give the source of the pack to whoever receives the binary. The rules are:

- A mod linked into the binary needs a GPL-3.0-compatible licence.
- A source patch is a derivative work of the server. Distribute it under terms that are compatible with GPL-3.0.
- A port is also a derivative work of the Java mod. The licence of the Java mod must allow the port. Keep its copyright notice, as the WITS port does for its MIT licence.
- The `ironpumpkin-mods` API crate is MIT OR Apache-2.0. It has no server source and no NeoForge source.
- A Wasm plugin talks to the server through the WIT boundary only. It has no such constraint.

---

## Do and don't

**Do**

- Keep the IronPumpkin commit of the template. When you move it, pin a full 40-character commit hash that has the API that the mod uses.
- Run `cargo xtask check` after each change, and commit `bin/Cargo.lock` and `bin/Cargo.lock.commit`.
- Depend on `ironpumpkin-mods` only.
- Use the same key in `modpack.toml` as the package name of the mod crate.
- Keep `exclude = [".ironpumpkin"]` in the root `Cargo.toml`.
- Give the mod a `neoforge.mods.toml` with a `license`, the `modId` and the registry `namespace` of the Java mod. Without a file, use the registry namespace as the `NativeMod::id`, when the id charset allows it.
- Set `client_required` to `false` for a mod with only server logic.
- Give the source of the pack, of each mod and of each patch to whoever gets the binary.

**Don't**

- Do not edit `bin/Cargo.toml` or `bin/src/mods.rs`. The tool writes both on each run.
- Do not commit `.ironpumpkin/` or `target/`.
- Do not use a branch name as `rev` or as the IronPumpkin commit.
- Do not depend on IronPumpkin from a fork URL. The build then has two copies of the server.
- Do not depend on `ironpumpkin-mods` with a crates.io version. The crate is not on crates.io.
- Do not panic in `NativeMod::init`. The panic hook of the server writes a crash report and exits at once.
- Do not use a mod id that another mod in the pack uses. The server stops at startup.
