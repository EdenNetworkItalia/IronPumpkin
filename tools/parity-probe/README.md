# Differential parity harness

NeoForge itself is the oracle for the NeoForge semantics of IronPumpkin. The same probe exists
twice: a Java mod on a real NeoForge 26.3.0.64-beta dedicated server, and a Rust native mod on
IronPumpkin. Both listen to the same events and write a normalized trace. A scripted client
performs the same actions on both servers, and `diff.py` reports the differences.

Slice 1 covers the events that fire before the play phase: lifecycle, registration, config,
payload registration, the configuration phase of one client and the server start and stop. The
driver is the headless client `tools/pumpkin-neoforge-client` in `record` mode, which ends at
`finish_configuration`. The client also has a play state (`play` mode, used by slice 2): see the
[client README](../pumpkin-neoforge-client/README.md).

| Path | Contents |
|:--|:--|
| `java/` | The Java probe: a NeoForge 26.3 mod, Gradle setup of `examples/modpack/client-mod`. |
| `rust/` | The Rust probe: the native mod `parityprobe` and the server binary `parity-probe-server`. A plain `cargo build` at the repository root does not build it. Use `-p parity-probe-rust`. |
| `diff.py` | Compares two traces. Python 3 standard library only. |
| `probe-channel.toml` | The channel of the Java probe, for the channel map of the client. |
| `captures/neoforge-26.3.0.64-beta/probe-trace.jsonl` | The Java trace, recorded on 2026-10-15. |
| `captures/ironpumpkin/probe-trace.jsonl` | The Rust trace of the same day. |
| `captures/ironpumpkin/diff.txt` | The output of `diff.py` for the two traces. |

## Trace format

`probe-trace.jsonl` is written to the working directory of the server. Each line is one JSON
object with three keys, in this order:

```json
{"ordinal":7,"name":"ModConfigEvent.Loading","fields":{"enabled":true,"file_name":"parityprobe-synced.toml","type":"SYNCED"}}
```

- `ordinal`: the position of the event in the trace, from 0 without gaps.
- `name`: the NeoForge class name. A nested class keeps its parent: `ServerTickEvent.Pre`.
- `fields`: the stable fields of the event, sorted by name. Values are strings, booleans or
  integers. A trace never has timestamps, ids, player names, paths or counts that change from run
  to run. `diff.py` compares the values by their JSON text, so `true` and `1` are different, and it
  rejects any other value type.

The tick events are counted and only the first `ServerTickEvent.Pre` and the first
`ServerTickEvent.Post` are traced. The Java probe logs the counts when the server stops.

## Events of slice 1

| Event | Bus | Fields | IronPumpkin today |
|:--|:--|:--|:--|
| `FMLConstructModEvent` | mod | none | `NativeMod::init` |
| `RegisterEvent` | mod | `registry`: only `minecraft:block`, `minecraft:item`, `minecraft:entity_type` | `RegisterEvent` (`registries::RegisterEvent`) |
| `FMLCommonSetupEvent` | mod | none | `NativeMod::common_setup` |
| `FMLLoadCompleteEvent` | mod | none | `NativeMod::load_complete` |
| `RegisterPayloadHandlersEvent` | mod | none; the probe registers `parityprobe:ping` here | not yet: the network task (2.11 in `tasks.md`) |
| `ModConfigEvent.Loading` | mod | `type`, `file_name`, the value `enabled` of the probe's synced config | `NativeMod::config_loading` |
| `ServerAboutToStartEvent` | game | none | `ServerAboutToStartEvent` |
| `ServerStartingEvent` | game | none | `ServerStartingEvent` |
| `ServerStartedEvent` | game | none | `ServerLoadEvent` with `LoadType::Startup` |
| `ServerTickEvent.Pre` | game | none | `ServerTickStartEvent` |
| `ServerTickEvent.Post` | game | none | `ServerTickEndEvent` |
| `RegisterConfigurationTasksEvent` | mod | `connection_type`, `ping_channel` (the client negotiated `parityprobe:ping`) | not yet: the network task (2.11 in `tasks.md`) |
| `ServerStoppingEvent` | game | none | `ServerStoppingEvent` |
| `ServerStoppedEvent` | game | none | `ServerStoppedEvent` |

The Rust probe uses today's API of `ironpumpkin-mods`: the `NativeMod` methods and the Pumpkin
events of the last column, the events registered with `ModInit::register_event`. Like the Java
probe, it registers one `ConfigType::Synced` config with `ModInit::register_config`: the file
`parityprobe-synced.toml` with the value `enabled = true`. It traces nothing for the "not yet"
rows. When a task adds an event, the Rust probe listens to it, and the captures and `diff.txt` are
recorded again.

## Result of slice 1

`captures/ironpumpkin/diff.txt` is the input for the event tasks:

- IronPumpkin fires 14 of the 16 events of the Java trace, in the order of NeoForge and with the
  same fields: `FMLConstructModEvent`, the three `RegisterEvent` lines (`minecraft:block`,
  `minecraft:entity_type`, `minecraft:item`), `FMLCommonSetupEvent`, `FMLLoadCompleteEvent`,
  `ModConfigEvent.Loading`, `ServerAboutToStartEvent`, `ServerStartingEvent`, `ServerStartedEvent`,
  `ServerTickEvent.Pre`, `ServerTickEvent.Post`, `ServerStoppingEvent` and `ServerStoppedEvent`.
  The diff has no `moved` and no `fields` line.
- The other 2 events are `missing`: `RegisterPayloadHandlersEvent` and
  `RegisterConfigurationTasksEvent` (the network task, 2.11 in `tasks.md`).

On NeoForge 26.3.0.64-beta, `ModConfigEvent.Loading` of a `SYNCED` config fires at startup,
after `RegisterPayloadHandlersEvent` and before `ServerAboutToStartEvent`, and
`RegisterConfigurationTasksEvent` fires once per connection, after the channel negotiation.
IronPumpkin loads the synced configs after `load_complete` and before `ServerAboutToStartEvent`,
so the loading event has the same place relative to the events that both servers fire.

Two recordings of the IronPumpkin trace, each from an empty server directory, are byte-identical.

## diff.py

```bash
python3 -I tools/parity-probe/diff.py REFERENCE CANDIDATE
python3 -I tools/parity-probe/diff.py --self-test
```

The script aligns the event names of the two traces (longest matching blocks, as `difflib`
does) and compares the fields of each aligned pair. It prints one line per difference:

- `missing`: an event of the reference that the candidate does not have.
- `extra`: an event of the candidate that the reference does not have.
- `moved`: an event that both traces have with the same fields, at another place in the order.
- `fields`: an aligned pair with different fields. One line per pair, with each differing field.

It exits with 0 without differences, 1 with differences, and 2 on an unreadable, non-UTF-8 or
invalid trace. The self-test checks each kind of difference, `true` against `1`, malformed traces
(wrong ordinal type, field value types, a trace that is not UTF-8), changes one field in a copy of
the Java trace and expects exactly one `fields` difference, and compares the diff of the committed
traces with `captures/ironpumpkin/diff.txt`.

## Record the traces

Run every command from the repository root. Keep downloads and the server install in a scratch
directory outside the repository, each in its own empty directory.

### Java probe

Java 25 is the toolchain. The build needs the hosts listed in `examples/modpack/client-mod/README.md`.

```bash
cd tools/parity-probe/java
GRADLE_USER_HOME=<scratch>/gradle-home JAVA_HOME=<jdk 25> ./gradlew build
```

The jar is `build/libs/parity-probe-1.0.0.jar`.

### NeoForge server

The procedure of `tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/README.md`,
with the probe in `mods/`:

```bash
curl -fLo <scratch>/download/neoforge-26.3.0.64-beta-installer.jar \
    https://maven.neoforged.net/releases/net/neoforged/neoforge/26.3.0.64-beta/neoforge-26.3.0.64-beta-installer.jar
java -jar <scratch>/download/neoforge-26.3.0.64-beta-installer.jar --install-server <scratch>/server
mkdir <scratch>/server/mods
cp tools/parity-probe/java/build/libs/parity-probe-1.0.0.jar <scratch>/server/mods/
echo eula=true > <scratch>/server/eula.txt
printf 'online-mode=false\nserver-port=25599\nwhite-list=false\n' > <scratch>/server/server.properties
cat tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml tools/parity-probe/probe-channel.toml \
    > <scratch>/channels.toml
```

The installer and the server download from `maven.neoforged.net`, `libraries.minecraft.net` and
the Mojang servers. A NeoForge 26.3 server enables the whitelist by default, which kicks the
client at login: keep `white-list=false`.

Start the server in `<scratch>/server` with `./run.sh nogui`. After the `Done (` line, run the
client once, then type `stop` on the server console:

```bash
cargo run -p pumpkin-neoforge-client -- record --port 25599 --username ParityProbe \
    --channels <scratch>/channels.toml --out <scratch>/neoforge-client.jsonl
```

The client ends with `configuration phase ended: finished`. Copy `<scratch>/server/probe-trace.jsonl`
to `captures/neoforge-26.3.0.64-beta/`.

### IronPumpkin

Write `pumpkin.toml` in a scratch directory, start the probe server there, run the same client
command with the port of `pumpkin.toml`, and type `stop` on the console:

```toml
detect_neoforge_clients = true

[commands]
use_console = true
use_tty = false

[networking.java]
address = "127.0.0.1:25598"
online_mode = false
```

```bash
cd <scratch>/ironpumpkin
cargo run --manifest-path <repo>/Cargo.toml -p parity-probe-rust
```

The log shows `loaded 1 native mod: parityprobe (client_required = false)`. Copy
`probe-trace.jsonl` to `captures/ironpumpkin/`, then write the new diff:

```bash
python3 -I tools/parity-probe/diff.py \
    tools/parity-probe/captures/neoforge-26.3.0.64-beta/probe-trace.jsonl \
    tools/parity-probe/captures/ironpumpkin/probe-trace.jsonl \
    > tools/parity-probe/captures/ironpumpkin/diff.txt
```

## Slice 2: the play state of the headless client

Slice 2 (task 2.21 in `tasks.md`) compares player, level and entity events. The headless client
has a play state: `play` mode, described in `tools/pumpkin-neoforge-client/README.md`. It enters
play, confirms the first teleport, sends the player-loaded packet, answers keep-alive, ping and
chunk batches, and runs a script with a ping and pong barrier and an extra wait after each
action, so both servers get the same actions in the same order. It records the play packets of each step in the `--out` format.

The play state enables these slice 2 scenarios:

| Scenario | Script action | Events |
|:--|:--|:--|
| Login | `join` | `PlayerLoggedInEvent` |
| Chat | `chat <text>` | `ServerChatEvent` |
| Command | `command <text>` | `CommandEvent` |
| Block break | `break <x y z>` | `BreakBlockEvent` |
| Block place | `hold <item>`, then `place <x y z> <face>` | `PlayerInteractEvent.RightClickBlock`, `EntityPlaceEvent` |
| Item use | `use_item` | `PlayerInteractEvent.RightClickItem` |
| Logout | `leave` | `PlayerLoggedOutEvent` |

Both servers run in creative mode without spawn protection: `break` breaks at once and `hold` takes
the item from the creative slot packet. `respawn` sends the respawn request, but no action of the
script kills the player yet. `scripts/slice2.toml` of the client runs join, chat, break, hold, place
and leave; its recordings on both servers are `captures/neoforge-26.3.0.64-beta/play-slice2.jsonl.gz`
and `captures/ironpumpkin/play-slice2.jsonl.gz` in the client directory. With `seed = "parity"` on
IronPumpkin and `level-seed=parity` on NeoForge, the spawn blocks still differ (0 118 0 and 8 120 5),
so the probes trace positions relative to the spawn, with fields such as the block and the cancel
result.
