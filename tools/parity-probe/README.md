# Differential parity harness

NeoForge itself is the oracle for the NeoForge semantics of IronPumpkin. The same probe exists
twice: a Java mod on a real NeoForge 26.3.0.64-beta dedicated server, and a Rust native mod on
IronPumpkin. Both listen to the same events and write a normalized trace. A scripted client
performs the same actions on both servers, and `diff.py` reports the differences.

Slice 1 covers the events that fire before the play phase: lifecycle, registration, config,
payload registration, the configuration phase of one client and the server start and stop. The
driver is the headless client `tools/pumpkin-neoforge-client` in `record` mode, which ends at
`finish_configuration`. Slice 2 runs the `play` mode of the same client: see
[Slice 2](#slice-2-play-events).

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

## Slice 2: play events

Slice 2 compares the play events. The driver is the `play` mode of the headless client with
`tools/pumpkin-neoforge-client/scripts/slice2.toml`: join, chat, break, hold, place and leave, with
a ping barrier and a wait of 1000 ms after each step (the "Play state" section of the
[client README](../pumpkin-neoforge-client/README.md)). Both servers run in creative mode without
spawn protection and with the seed `parity`, so `break` breaks at once and `hold` takes the item
from the creative slot packet.

| Path | Contents |
|:--|:--|
| `captures/neoforge-26.3.0.64-beta/probe-trace-slice2.jsonl` | The Java trace of a `play` run, recorded on 2026-10-15. |
| `captures/ironpumpkin/probe-trace-slice2.jsonl` | The Rust trace of the same day. |
| `captures/ironpumpkin/diff-slice2.txt` | The output of `diff.py --slice2` for the two traces. |

### Steps

The trace format is the format of slice 1. Each play event has one more field, `step`: the index
of the script step, 0 `join`, 1 `chat`, 2 `break`, 3 `hold`, 4 `place` and 5 `leave`.

The server does not know the script, so each probe counts the play `ping_request` packets of the
barriers. The step of an event is the number of pings that arrived at least 500 ms before it. A
plain count does not work: NeoForge reads the ping on the network thread, before the main thread
runs the action that the client sent just before it, so the events of an action would get the
next step. An event fires a few ticks after its action, well before the 500 ms of its ping end,
and the next action comes 1000 ms (`wait_ms`) after the pong. Keep `wait_ms` of the script above
500 ms plus the slowest step. The Java probe counts the ping with a mixin on
`ServerGamePacketListenerImpl.handlePingRequest`, because NeoForge has no event for it; the Rust
probe counts it with `PacketReceivedEvent`.

The probes trace the play events from `PlayerLoggedInEvent` to `ServerStoppingEvent`. The events of
the configuration phase (`PlayerNegotiationEvent`, and `PlayerEvent.LoadFromFile`, which NeoForge
26.3 fires in `PrepareSpawnTask`) fire before this window and are not traced, so a `record` run of
slice 1 gives the same trace as before. The tick events are traced once per step: the first
`ServerTickEvent.Pre` and `.Post` and the first `PlayerTickEvent.Pre` and `.Post` of each step, with
the step taken at `ServerTickEvent.Pre`, so the four events of one tick have the same step.

Positions are relative to the block of the player's feet at the login, the origin of the `~`
coordinates of the script: the two servers spawn the player at different blocks, and NeoForge at a
random block around the world spawn. For the same reason no field names a block of the terrain.
Two NeoForge runs gave `grass_block` and `dirt` for the same broken block. Only the placed block,
which comes from the script, is a field.

### Events of slice 2

| Event | Fields | IronPumpkin today |
|:--|:--|:--|
| `PlayerEvent.PlayerLoggedInEvent` | none | `PlayerJoinEvent` |
| `PlayerEvent.PlayerLoggedOutEvent` | none | `PlayerLeaveEvent` |
| `PlayerEvent.SaveToFile` | none | `PlayerEventSaveToFile` |
| `PlayerEvent.LoadFromFile` | none | `PlayerEventLoadFromFile` |
| `PlayerEvent.NameFormat`, `PlayerEvent.TabListNameFormat` | none | `PlayerEventNameFormat`, `PlayerEventTabListNameFormat` |
| `PlayerEvent.PlayerChangeGameModeEvent` | `current`, `new` | `PlayerGamemodeChangeEvent` |
| `ClientInformationUpdatedEvent` | none | `ClientInformationUpdatedEvent` |
| `ServerChatEvent` | `raw_text` | `PlayerChatEvent` |
| `PlayerInteractEvent.LeftClickBlock` | `action`, `face`, `hand`, `pos` | `PlayerInteractEvent` with `LeftClickBlock`: `pos` only |
| `PlayerInteractEvent.RightClickBlock` | `face`, `hand`, `item`, `pos` | `PlayerInteractEvent` with `RightClickBlock`: `pos` only |
| `UseItemOnBlockEvent` | `face`, `hand`, `item`, `phase`, `pos` | the same `PlayerInteractEvent`: `pos` only |
| `PlayerInteractEvent.RightClickItem` | `hand`, `item` | `PlayerInteractEvent` with `RightClickAir`: no field |
| `PlayerSwitchHotbarSlotEvent.Pre`, `.Post` | `new_slot`, `old_slot` | `PlayerItemHeldEvent`, `PlayerSwitchHotbarSlotEventPost` |
| `PlayerEvent.BreakSpeed`, `PlayerEvent.HarvestCheck` | none | `PlayerEventBreakSpeed`, `PlayerEventHarvestCheck` |
| `BreakBlockEvent` | `pos` | not traced: the level and block events task (2.13 in `tasks.md`) |
| `BlockEvent.EntityPlaceEvent` | `block`, `pos` | not traced: the same task |
| `ServerTickEvent.Pre`, `.Post` | none, once per step | `ServerTickStartEvent`, `ServerTickEndEvent` |
| `PlayerTickEvent.Pre`, `.Post` | none, once per step | none: the row is planned |

The Pumpkin `PlayerInteractEvent` has no hand, face, item, action or use phase, so the Rust probe
writes only the position for the four interact events. Not traced: `PlayerEvent.StartTracking` and
`StopTracking` (the entities around the spawn differ between the servers), the entity and living
events such as `EntityJoinLevelEvent` (other families), and `ItemEntityPickupEvent` (a creative
break drops nothing).

### Record slice 2

Prepare both servers as for slice 1 (see [Record the traces](#record-the-traces)), with these
additions. NeoForge `server.properties`:

```properties
gamemode=creative
spawn-protection=0
view-distance=4
simulation-distance=4
level-seed=parity
```

IronPumpkin `pumpkin.toml`, at the top level:

```toml
seed = "parity"
default_gamemode = "Creative"
spawn_protection = 0
```

Start each server in a new directory, run the client once in `play` mode, then type `stop` on the
console:

```bash
cargo run -p pumpkin-neoforge-client -- play --port <port> --username ParityProbe \
    --channels <scratch>/channels.toml \
    --script tools/pumpkin-neoforge-client/scripts/slice2.toml --out <scratch>/play.jsonl
```

The client ends with `play ended: left`. Copy `probe-trace.jsonl` of each server to
`probe-trace-slice2.jsonl` in its `captures/` directory, then write the new diff:

```bash
python3 -I tools/parity-probe/diff.py --slice2 \
    tools/parity-probe/captures/neoforge-26.3.0.64-beta/probe-trace-slice2.jsonl \
    tools/parity-probe/captures/ironpumpkin/probe-trace-slice2.jsonl \
    > tools/parity-probe/captures/ironpumpkin/diff-slice2.txt
```

With `--slice2`, `diff.py` compares only the events that have the field `step`, step by step, and
writes `step N` in each line. An event that both traces have with the same fields, but in
different steps, is one `step` difference. The exit codes are those of slice 1. The self-test
checks each kind on small traces, rejects a `step` that is not a non-negative integer, changes one
field of the committed Java trace, and compares the diff of the committed slice 2 traces with
`diff-slice2.txt`.

### Result of slice 2

`captures/ironpumpkin/diff-slice2.txt` has 20 differences (missing 16, fields 3, extra 1). Two
recordings per server, each from an empty server directory, are byte-identical.

| Event | NeoForge | IronPumpkin | Verdict | Cause |
|:--|:--|:--|:--|:--|
| `PlayerEvent.PlayerLoggedInEvent` | step 0 | step 0 | same | |
| `ServerChatEvent` | step 1, `raw_text` | step 1, `raw_text` | same | |
| `PlayerEvent.PlayerLoggedOutEvent`, `PlayerEvent.SaveToFile` | step 5 | step 5 | same | |
| `ServerTickEvent.Pre`, `.Post` | each step | each step | same | |
| `PlayerInteractEvent.LeftClickBlock` | step 2: `start`, `up`, `main_hand`, `1 -1 0` | step 2: `1 -1 0` | fields | Pumpkin's `PlayerInteractEvent` has no action, face or hand (gap in the design.md row). It fires at the dig start, as NeoForge, not on the arm swing: test `left_click_block_fires_at_the_dig_start`. |
| `BreakBlockEvent` | step 2: `1 -1 0` | not traced | missing | The level and block events task (2.13). |
| `PlayerSwitchHotbarSlotEvent.Pre` | none: the slot does not change | none | same | Both fire it only when the slot changes: test `switch_hotbar_slot_event_pre_fires_only_on_a_change`. |
| `PlayerEvent.NameFormat` | none in the window: NeoForge caches the name from before the login | step 3 | extra | Documented: Pumpkin does not cache the name and fires on every read (design.md row). |
| `PlayerInteractEvent.RightClickBlock` | step 4: `up`, `main_hand`, `minecraft:stone`, `1 -2 0` | step 4: `1 -2 0` | fields | No hand, face or item in Pumpkin's event (gap in the design.md row). |
| `UseItemOnBlockEvent` | step 4: three events, phases `item_before_block`, `block`, `item_after_block` | step 4: one event, `1 -2 0` | fields, missing 2 | One event per use without the phase, hand, face or item (gap in the design.md row). |
| `BlockEvent.EntityPlaceEvent` | step 4: `minecraft:stone`, `1 -1 0` | not traced | missing | The level and block events task (2.13). |
| `PlayerTickEvent.Pre`, `.Post` | each step | none | missing (12 lines) | Planned row: Pumpkin has no player tick event. |
| `ClientInformationUpdatedEvent` | none | none | same | The client sends no client information on the join, as the vanilla 26.3 client (client README, "Play state"). |

No difference is a vanilla gameplay difference of Pumpkin, so slice 2 opens no `upstream` issue.
