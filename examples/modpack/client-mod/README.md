# Hello Mod client (NeoForge 26.3)

A NeoForge 26.3 Java mod that registers the content of the native mod `mods/hello-mod` under the
same registry names. A NeoForge client with this mod joins an IronPumpkin server that loads
`hello-mod`, passes the registry sync and renders the custom content. A NeoForge client without it
is disconnected by its own registry check.

## Names

NeoForge mod ids must match `[a-z][a-z0-9_]{1,63}`, so the mod id cannot be `hello-mod`. The mod id
is `hello_mod`. The registry namespace is `hello-mod` (`HelloMod.NAMESPACE`), so the registry names
match the server exactly.

| Server (`hello-mod`)        | Client mod registry        | Client class              | Synced id |
| --------------------------- | -------------------------- | ------------------------- | --------- |
| block `hello-mod:greeter_lamp` | `minecraft:block`       | `RedstoneLampBlock` (`lit`, default `false`) | 1286 |
| item `hello-mod:greeter_lamp`  | `minecraft:item`        | `BlockItem` of the lamp, redstone creative tab | 1658 |
| entity type `hello-mod:greeter` | `minecraft:entity_type` | `Pig`, `PigRenderer`, pig attributes | 161 |

The ids are the first id after the vanilla entries of each registry, in NeoForge 26.3.0.64-beta
and in IronPumpkin. The block has the two states of a redstone lamp, like the server block. The
models use the vanilla redstone lamp textures. The mod registers nothing else, so the registry
snapshots of the client and of the server have the same entries.

## Build

Java 25 is the toolchain (`java.toolchain.languageVersion`). Gradle 9 itself needs a JVM 17 or
newer on `PATH` or in `JAVA_HOME`. Gradle downloads a JDK 25 through the foojay resolver when none
is installed, and only after it has started on that JVM. The build needs the network:
`services.gradle.org`, `plugins.gradle.org`, `maven.neoforged.net`, `repo.maven.apache.org`, the
Mojang servers, `api.foojay.io` and the host of the JDK vendor (Adoptium: `api.adoptium.net` and
the release downloads on `github.com`).

```sh
cd examples/modpack/client-mod
./gradlew build
```

The jar is `build/libs/hello-mod-client-1.0.0.jar`. The jar is reproducible: file timestamps are
off and the entry order is fixed.

## Install in the client

1. Install NeoForge `26.3.0.64-beta` for Minecraft 26.3 with the NeoForge installer ("Install
   client").
2. Copy `hello-mod-client-1.0.0.jar` to the `mods/` directory of the game directory
   (`~/.minecraft/mods/` for the vanilla launcher).
3. Start the `neoforge-26.3.0.64-beta` profile. The mod list shows "Hello Mod (IronPumpkin
   client)".
4. Join the IronPumpkin example modpack server. Then `/setblock ~ ~ ~ hello-mod:greeter_lamp`,
   the lamp in the redstone creative tab, and `/summon hello-mod:greeter`.

`./gradlew runClient` starts a development client with the mod, and `./gradlew runServer` a
development NeoForge server.
