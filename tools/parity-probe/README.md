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
| `captures/ironpumpkin/probe-trace.jsonl` | The Rust trace, recorded on 2026-10-16. |
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
| `RegisterPayloadHandlersEvent` | mod | none; the probe registers `parityprobe:ping` here | `network::RegisterPayloadHandlersEvent` |
| `ModConfigEvent.Loading` | mod | `type`, `file_name`, the value `enabled` of the probe's synced config | `NativeMod::config_loading` |
| `ServerAboutToStartEvent` | game | none | `ServerAboutToStartEvent` |
| `ServerStartingEvent` | game | none | `ServerStartingEvent` |
| `ServerStartedEvent` | game | none | `ServerLoadEvent` with `LoadType::Startup` |
| `ServerTickEvent.Pre` | game | none | `ServerTickStartEvent` |
| `ServerTickEvent.Post` | game | none | `ServerTickEndEvent` |
| `RegisterConfigurationTasksEvent` | mod | `connection_type`, `ping_channel` (the client negotiated `parityprobe:ping`) | `network::RegisterConfigurationTasksEvent` |
| `ServerStoppingEvent` | game | none | `ServerStoppingEvent` |
| `ServerStoppedEvent` | game | none | `ServerStoppedEvent` |

The Rust probe uses today's API of `ironpumpkin-mods`: the `NativeMod` methods and the Pumpkin
events of the last column, the events registered with `ModInit::register_event`. Like the Java
probe, it registers one `ConfigType::Synced` config with `ModInit::register_config`: the file
`parityprobe-synced.toml` with the value `enabled = true`. It also registers `parityprobe:ping`
as an optional configuration channel in both directions, with
`PayloadRegistrar::configuration_bidirectional`. When a task adds an event, the Rust probe
listens to it, and the captures and `diff.txt` are recorded again.

## Result of slice 1

`captures/ironpumpkin/diff.txt` reports no differences. IronPumpkin fires the 16 events of the Java
trace in the order of NeoForge and with the same fields:

- `FMLConstructModEvent` and the three `RegisterEvent` lines (`minecraft:block`,
  `minecraft:entity_type`, `minecraft:item`).
- `FMLCommonSetupEvent`, `FMLLoadCompleteEvent` and `RegisterPayloadHandlersEvent`.
- `ModConfigEvent.Loading`, `ServerAboutToStartEvent`, `ServerStartingEvent` and
  `ServerStartedEvent`.
- `ServerTickEvent.Pre`, `ServerTickEvent.Post` and `RegisterConfigurationTasksEvent`.
- `ServerStoppingEvent` and `ServerStoppedEvent`.

On both servers, `RegisterPayloadHandlersEvent` fires at startup after `FMLLoadCompleteEvent` and
before `ModConfigEvent.Loading` of the `SYNCED` config, and `RegisterConfigurationTasksEvent` fires
once per connection, after the channel negotiation, with `connection_type` `NEOFORGE` and the
negotiated `parityprobe:ping`.

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
`tools/pumpkin-neoforge-client/scripts/slice2.toml`. The script has six steps: join, chat, break,
hold, place and leave. After each step the client sends a ping barrier and waits 1000 ms (the
"Play state" section of the [client README](../pumpkin-neoforge-client/README.md)).

Both servers run in creative mode with the seed `parity` and without spawn protection. So `break`
breaks at once, and `hold` takes the item from the creative slot packet.

| Path | Contents |
|:--|:--|
| `captures/neoforge-26.3.0.64-beta/probe-trace-slice2.jsonl` | The Java trace of a `play` run, recorded on 2026-10-15. |
| `captures/ironpumpkin/probe-trace-slice2.jsonl` | The Rust trace, recorded on 2026-10-17 on the merged wave 4 tree. |
| `captures/ironpumpkin/diff-slice2.txt` | The output of `diff.py --slice2` for the two traces. |

### Steps

The trace format is the format of slice 1. Each play event has one more field, `step`, the index
of the script step. The indexes are 0 `join`, 1 `chat`, 2 `break`, 3 `hold`, 4 `place` and 5
`leave`.

The server does not know the script, so each probe counts the play `ping_request` packets of the
barriers. The step of an event is the number of pings that arrived at least 500 ms before it. A
plain count does not work. NeoForge reads the ping on the network thread. At that time the main
thread has not run the action that the client sent before the ping. So the events of an action
would get the next step.

An event fires a few ticks after its action, well before the 500 ms of its ping end. The next
action comes 1000 ms (`wait_ms`) after the pong. Keep `wait_ms` of the script above 500 ms plus
the slowest step. The Java probe counts the ping with a mixin on
`ServerGamePacketListenerImpl.handlePingRequest`, because NeoForge has no event for it. The Rust
probe counts it with `PacketReceivedEvent`.

The probes trace the play events from `PlayerLoggedInEvent` to `ServerStoppingEvent`. The events of
the configuration phase fire before this window and are not traced. These are
`PlayerNegotiationEvent` and `PlayerEvent.LoadFromFile`, which NeoForge 26.3 fires in
`PrepareSpawnTask`. So the trace of slice 1 has none of them.

The tick events are traced once per step. The trace holds the first `ServerTickEvent.Pre` and
`.Post` and the first `PlayerTickEvent.Pre` and `.Post` of each step. The step is taken at
`ServerTickEvent.Pre`, so the four events of one tick have the same step.

Positions are relative to the block of the player's feet at the login. That block is the origin of
the `~` coordinates of the script. The two servers spawn the player at different blocks, and
NeoForge uses a random block around the world spawn. For the same reason no field names a block
of the terrain.
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
| `PlayerInteractEvent.LeftClickBlock` | `action`, `face`, `hand`, `pos` | `PlayerInteractEvent` with `LeftClickBlock` |
| `PlayerInteractEvent.RightClickBlock` | `face`, `hand`, `item`, `pos` | `PlayerInteractEvent` with `RightClickBlock` |
| `UseItemOnBlockEvent` | `face`, `hand`, `item`, `phase`, `pos` | `UseItemOnBlockEvent` |
| `PlayerInteractEvent.RightClickItem` | `hand`, `item` | `PlayerInteractEvent` with `RightClickAir` |
| `PlayerSwitchHotbarSlotEvent.Pre`, `.Post` | `new_slot`, `old_slot` | `PlayerItemHeldEvent`, `PlayerSwitchHotbarSlotEventPost` |
| `PlayerEvent.BreakSpeed`, `PlayerEvent.HarvestCheck` | none | `PlayerEventBreakSpeed`, `PlayerEventHarvestCheck` |
| `BreakBlockEvent` | `pos` | `BlockBreakEvent` with a player: the probe skips a break without a player, as `getPlayer` is never null |
| `BlockEvent.EntityPlaceEvent` | `block`, `pos` | `BlockPlaceEvent` |
| `ServerTickEvent.Pre`, `.Post` | none, once per step | `ServerTickStartEvent`, `ServerTickEndEvent` |
| `PlayerTickEvent.Pre`, `.Post` | none, once per step | `PlayerTickEventPre`, `PlayerTickEventPost` |

These events are not traced:

- `PlayerEvent.StartTracking` and `StopTracking`: the entities around the spawn differ between the
  servers.
- The entity and living events, such as `EntityJoinLevelEvent`: they belong to other families.
- `ItemEntityPickupEvent`: a creative break drops nothing.

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
different steps, is one `step` difference. The exit codes are those of slice 1. The self-test:

- checks each kind on small traces;
- rejects a `step` that is not a non-negative integer;
- changes one field of the committed Java trace;
- compares the diff of the committed slice 2 traces with `diff-slice2.txt`.

### Result of slice 2

`captures/ironpumpkin/diff-slice2.txt` has 1 difference (extra 1). Two
recordings per server, each from an empty server directory, are byte-identical.

| Event | NeoForge | IronPumpkin | Verdict | Cause |
|:--|:--|:--|:--|:--|
| `PlayerEvent.PlayerLoggedInEvent` | step 0 | step 0 | same | |
| `ServerChatEvent` | step 1, `raw_text` | step 1, `raw_text` | same | |
| `PlayerEvent.PlayerLoggedOutEvent`, `PlayerEvent.SaveToFile` | step 5 | step 5 | same | |
| `ServerTickEvent.Pre`, `.Post` | each step | each step | same | |
| `PlayerInteractEvent.LeftClickBlock` | step 2: `start`, `up`, `main_hand`, `1 -1 0` | step 2: `start`, `up`, `main_hand`, `1 -1 0` | same | It fires for each dig action, as NeoForge, not on the arm swing, and a cancel stops the action and re-syncs the block: tests `left_click_block_fires_at_the_dig_start` and `finished_digging_breaks_only_the_started_dig`. |
| `BreakBlockEvent` | step 2: `1 -1 0` | step 2: `1 -1 0` | same | The dig also breaks the plant on the block (`1 0 0`), and Pumpkin fires `BlockBreakEvent` without a player for it, where NeoForge posts nothing (design.md row). |
| `PlayerSwitchHotbarSlotEvent.Pre` | none: the slot does not change | none | same | Both fire it only when the slot changes: test `switch_hotbar_slot_event_pre_fires_only_on_a_change`. |
| `PlayerEvent.NameFormat` | none in the window: NeoForge caches the name from before the login | step 3 | extra | Documented: Pumpkin does not cache the name and fires on every read (design.md row). |
| `PlayerInteractEvent.RightClickBlock` | step 4: `up`, `main_hand`, `minecraft:stone`, `1 -2 0` | step 4: `up`, `main_hand`, `minecraft:stone`, `1 -2 0` | same | Test `right_click_block_and_use_item_on_block_fire_each_phase`; the `use_block` and `use_item` denials and the cancellation result: tests `use_block_and_use_item_denials_follow_common_hooks` and `a_success_cancellation_result_swings_the_arm_without_the_action`. |
| `UseItemOnBlockEvent` | step 4: three events, phases `item_before_block`, `block`, `item_after_block` | step 4: the same three events | same | One event per phase that runs, as NeoForge: test `right_click_block_and_use_item_on_block_fire_each_phase`. |
| `BlockEvent.EntityPlaceEvent` | step 4: `minecraft:stone`, `1 -1 0` | step 4: `minecraft:stone`, `1 -1 0` | same | Pumpkin fires it before the block changes; NeoForge fires it after and restores the block on a cancel. |
| `PlayerTickEvent.Pre`, `.Post` | each step | each step | same | |
| `ClientInformationUpdatedEvent` | none | none | same | The client sends no client information on the join, as the vanilla 26.3 client (client README, "Play state"). |

No difference is a vanilla gameplay difference of Pumpkin, so slice 2 opens no `upstream` issue.
