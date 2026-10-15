# pumpkin-neoforge-client

A headless Minecraft Java client for the configuration phase and a scripted play state. It
connects to a server, logs in in offline mode and answers the configuration phase like the vanilla
client. With a channel map it also answers like a NeoForge 26.3 client. It records every
clientbound configuration packet and decodes each custom payload with the codecs in
`pumpkin-protocol/src/java/neoforge`. The `play` mode then enters the play state and runs a script
of player actions (see [Play state](#play-state)).

The server must run with online mode off. The client handles compression, and encryption without
the session server join, because Pumpkin asks for encryption in offline mode too.

## Usage

```bash
cargo run -p pumpkin-neoforge-client -- record --port 25565 --out recording.jsonl
cargo run -p pumpkin-neoforge-client -- record --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml --out modded.jsonl
cargo run -p pumpkin-neoforge-client -- assert --port 25565 \
    --expected-file tools/pumpkin-neoforge-client/expected/pumpkin-vanilla.txt
cargo run -p pumpkin-neoforge-client -- assert --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml \
    --expected-file tools/pumpkin-neoforge-client/expected/pumpkin-neoforge.txt \
    --registries tools/pumpkin-neoforge-client/expected/pumpkin-neoforge-registries.toml
cargo run -p pumpkin-neoforge-client -- assert --port 25565 --expect minecraft:brand
cargo run -p pumpkin-neoforge-client -- assert --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/required-unknown.toml \
    --expect-disconnect multiplayer.disconnect.incompatible
cargo run -p pumpkin-neoforge-client -- compare \
    --reference tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/run-b-neoforge.jsonl.gz \
    modded.jsonl
```

Options for the `record`, `assert` and `play` modes:

| Option | Default | Meaning |
|:--|:--|:--|
| `--host`, `--port` | `127.0.0.1`, `25565` | Server address. |
| `--username` | random `ProbeNNNNNN` | Offline player name. |
| `--channels <file>` | none | Channel map. Without it the client is a vanilla client. |
| `--brand <name>` | `neoforge` with a channel map, else `vanilla` | Value of `minecraft:brand`. |
| `--timeout <s>` | `60` | Time limit for login and configuration, and in play for the first player position and for each pong. |
| `--out <file>` | none | JSONL file with one line per recorded packet. |

`record` stops at `finish_configuration` or at a disconnect, and then exits with 0.

`assert` compares the channels of the clientbound custom payloads, in order, with the expected
list from `--expect` or `--expected-file`. It exits with 1 when the lists differ, a payload with a
codec fails `decode_exact`, or the configuration does not finish. The expected file has one
channel per line. `#` starts a comment.

`assert --registries <file>` also checks the `neoforge:frozen_registry` payloads against a
registry expectation file. The run fails when the registry names differ from the file in order,
or when an expected registry differs in its id count, its first or last name, or a named entry.

`assert --expect-disconnect <translation key>` expects a kick instead of a finished configuration.
The run passes only when the server ends the configuration with a `disconnect` whose reason is a
translate component with that key. It exits with 1 on any other end: `finish_configuration`, a
disconnect with another key or a plain text reason, a login disconnect, or a disconnect by the
client's own NeoForge checks. With `--expect-disconnect`, the expected channel list is optional;
without a list, the client does not check the channel sequence. The log shows the key, the
arguments and the English text of the reason.

`compare --reference <capture> <recording>` reads two recordings in the `--out` format, gzipped
when the name ends in `.gz`, and exits with 1 when they differ in one of these:

- The channels of the clientbound custom payloads from `neoforge:frozen_registry_sync_start` to
  `finish_configuration`, in order. In the reference, the `frozen_registry` payloads of registries
  other than block, item and entity type, the registries that IronPumpkin syncs, are left out. The
  recording is not filtered, so a registry that IronPumpkin sends in addition to those three is a
  difference.
- The clientbound bodies of `c:version`, `c:register`, `neoforge:config_file`,
  `neoforge:extensible_enum_data` and `neoforge:feature_flags`, byte for byte, and the decoded
  `neoforge:known_registry_data_maps`. `NeoForge` writes the registries of that payload in the
  hash order of their keys, which changes with each server start, so the comparison sorts them.
  A `neoforge:known_registry_data_maps` body that does not decode, in the reference or in the
  recording, is a difference.

The record and assert modes exit with 2 when the run cannot complete: a connect failure, the
timeout, an early end of the stream, or a decode error in a packet or payload that the client must
act on, such as a disconnect reason that is not exactly one text component.

Each recorded packet goes to stdout as one line and, with `--out`, to the JSONL file. A JSONL line
has the direction, the state, the packet name, the channel, the body length, the decode result, a
summary, and the body bytes in hex. The client also records the custom payloads it sends.

## Play state

`play` runs the login and the configuration phase like `record`, acknowledges
`finish_configuration`, and then runs a script in the play state:

```bash
cargo run -p pumpkin-neoforge-client -- play --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml \
    --script tools/pumpkin-neoforge-client/scripts/slice2.toml --out play.jsonl
cargo run -p pumpkin-neoforge-client -- play --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml \
    --script tools/pumpkin-neoforge-client/scripts/boot.toml \
    --expected-file tools/pumpkin-neoforge-client/expected/pumpkin-play.txt
```

In the play state the client answers these packets:

- `login`: it sends nothing, as the vanilla 26.3 client. `ClientPacketListener.handleLogin` sends
  no client information. In play, only `ClientPacketListener.broadcastClientInformation` sends it,
  through `Options.broadcastOptions`, which every `Options.save` calls. The field
  `remoteClientInformation` starts empty for each play connection, so the first save in play sends
  the information even without a change: an option change, a tutorial step, or the first join to
  any multiplayer server (`joinedFirstServer` in `Minecraft.tick`). The client acts as a vanilla
  client that joined a server before and changes no option, so a join fires no
  `ClientInformationUpdatedEvent` on the server.
- `player_position`: it applies the position and the relative flags, then sends
  `accept_teleportation` and `move_player_pos_rot`. After the first position it sends
  `player_loaded`. The block of the player's feet at that position is the origin of the `~`
  coordinates of the script.
- `keep_alive` and `ping`: it answers with `keep_alive` and `pong`.
- `pong_response`: it ends the barrier of a step. See [Script format](#script-format).
- `chunk_batch_finished`: it answers with `chunk_batch_received` and a rate of 64 chunks per tick.
- `disconnect`: the run ends with the reason.
- `custom_payload` on `neoforge:split`: it joins the slices as in the configuration phase.
- `start_configuration`: the run fails, because the client has no second configuration phase.

The client records all other packets and does nothing with them.

### Script format

A script is TOML, or JSON when the file name ends in `.json`. `steps` is the list of actions, in
order. The first step must be `join` and the last step must be `leave`.

Each action ends with a barrier. The client sends a `ping_request` and reads until the
`pong_response` with the same payload, so the server has read the action. It then keeps reading
for `wait_ms` milliseconds (default 1000) for the packets that trail the pong, and answers the
packets above. A server that sends no pong within `--timeout` fails the run.

The pong shows that the server read the action. It does not show that the server finished the
action. On IronPumpkin, the `disguised_chat` echo of a `chat` step and the `block_changed_ack` of
a `break` step arrive after the pong (measured in one run of `scripts/boot.toml`), so `wait_ms`
still decides whether a step sees them. Set `wait_ms` for the slowest effect that the script
needs.

```toml
wait_ms = 1000
steps = [
    "join",
    "chat hello from the parity script",
    "break ~1 ~-1 ~0",
    "hold minecraft:stone",
    "place ~1 ~-2 ~0 up",
    "leave",
]
```

| Action | Packets | Meaning |
|:--|:--|:--|
| `join` | | Log in, configure, and wait for the first player position. |
| `chat <text>` | `chat` | An unsigned chat message. |
| `command <text>` | `chat_command` | A command, with or without the leading `/`. |
| `move <dx> <dy> <dz>` | `move_player_pos` | Move by this offset in one packet, with the on-ground flag set. |
| `break <x> <y> <z> [ticks]` | `player_action` | Start to destroy the block on its top face. With `ticks`, stop to destroy it after that many ticks. |
| `place <x> <y> <z> <face>` | `use_item_on` | Use the main hand on a face of the block: `down`, `up`, `north`, `south`, `west` or `east`. A block item places a block on that side. |
| `hold <item>` | `set_creative_mode_slot`, `set_carried_item` | Put one item in the first hotbar slot and select that slot. |
| `use_item` | `use_item` | Use the item in the main hand. |
| `respawn` | `client_command` | Ask to respawn after a death. |
| `wait <ms>` | | Read for this long. It sends no ping and has no `wait_ms`. |
| `leave` | | Close the connection, as the vanilla client does on a disconnect. |

A coordinate is absolute, or relative to the origin with a `~` prefix: `~`, `~1`, `~-2`. The
actions that predict a block change count a sequence number from 1, like
`BlockStatePredictionHandler`.

`scripts/slice2.toml` is the script of slice 2 of the parity harness. `scripts/boot.toml` is the
script of the boot test.

### Recording and expectations

Each entry of a play run has the field `step`, the index of the script step that was running. The
login and configuration packets belong to step 0. The play packets have the state `play`, in both
directions. `level_chunk_with_light`, `light_update`, `chunks_biomes` and the
`neoforge:auxiliary_light_data` payload are recorded with their length but without the body bytes.
After each step the log lists the clientbound play packets of the step, with a count for repeated
packets. Each step except `wait` and `leave` records one serverbound `ping_request` and one
clientbound `pong_response` from its barrier. The recordings under `captures/` come from a client
without the barrier and have neither packet. A new recording has both on each server.

`play --expected-file <file>` checks the run. The file has one line per step, `action: packets`:
the action name of the step and the clientbound play packets that the step must receive, in any
order. `#` starts a comment. The run fails with exit 1 when the script does not reach `leave`, the
step actions differ from the file, a step does not receive a packet of its line, or a payload with
a codec does not decode. Without the option, `play` exits with 0 whenever the run completes. It
exits with 2 for the reasons of `record`, when no player position or no pong arrives within
`--timeout`, and on `start_configuration`.

### Limits

- No rendering and no world: the client discards chunk and light data and does not know the
  blocks. A `~` coordinate is relative to the first position, whatever block is there.
- No physics beyond the position sync: the position changes only by a teleport of the server and
  by `move`. The server checks a move like any other, so a long move or a move into a block fails.
- No inventory model: `hold` works only in creative mode, where the server takes the item from
  `set_creative_mode_slot`. `place` and `use_item` use the main hand.
- `break` without ticks breaks a block only in creative mode. In survival mode, the stop must come
  after the destroy time of the block, which the script gives in ticks.
- Chat is unsigned, so the server must not enforce secure chat. Online mode off turns it off on
  vanilla and NeoForge.
- No second configuration phase, no automatic respawn, no entity or container interaction.

## Channel map

The map lists the channels the client claims in its `neoforge:register` reply, per protocol. Files
that end in `.json` are JSON. All other files are TOML.

```toml
[[configuration]]
id = "mymod:sync"
version = "1"
flow = "clientbound" # or "serverbound"; leave it out for a bidirectional channel
optional = true

[[play]]
id = "mymod:data"
version = "1"
optional = true
```

`channels/neoforge-26.3.toml` has the channels of a NeoForge 26.3 client without mods, plus the
fake mod channel `probe:fake`. `channels/required-unknown.toml` has one required channel that no
server has, so the negotiation fails.

## Registry expectations

A registry expectation file is TOML with one `[[registry]]` table per `frozen_registry` payload,
in the order the server sends them. The ids of a registry must be `0` to `ids - 1` without gaps.

```toml
[[registry]]
name = "minecraft:block"
ids = 1287                                # the id count
first = "minecraft:air"                   # the name at id 0
last = "test-mod:test_block"              # the name at id ids - 1
at = { 1285 = "minecraft:firefly_bush" }  # optional: more names by id
```

## Expected lists

- `expected/pumpkin-vanilla.txt`: a vanilla client against IronPumpkin with
  `detect_neoforge_clients = false`.
- `expected/pumpkin-neoforge.txt`: `channels/neoforge-26.3.toml` against IronPumpkin with
  `detect_neoforge_clients = true`.
- `expected/pumpkin-neoforge-test-mod.txt`: the same against IronPumpkin with the native mod
  `ironpumpkin-test-mod`, which adds its synced config. The boot test of that crate reads it.
- `expected/pumpkin-neoforge-registries.toml`: the block, item and entity type registries of
  IronPumpkin without native mods. Their counts and first and last names equal run (b) of the
  capture: block 1286 ids from `minecraft:air` to `minecraft:firefly_bush`, item 1658 ids from
  `minecraft:air` to `minecraft:ominous_bottle`, entity type 161 ids from `minecraft:acacia_boat`
  to `minecraft:fishing_bobber`.
- `expected/pumpkin-neoforge-test-mod-registries.toml`: the same registries with the content of
  `ironpumpkin-test-mod` at the first free ids, 1286, 1658 and 161. The boot test reads it.
- `expected/pumpkin-play.txt`: the play expectation of `scripts/boot.toml` against IronPumpkin in
  creative mode without spawn protection. The play boot test of `ironpumpkin-test-mod`
  (`tests/neoforge_play.rs`) reads it, and checks the join, the chat message and the leave in the
  server log.

## NeoForge behaviour

With a channel map the client does what `ClientConfigurationPacketListenerImpl`,
`ClientCommonPacketListenerImpl` and `ClientNetworkRegistry` do in NeoForge 26.3:

- It answers `minecraft:register` before the negotiation with its listening channels. It keeps
  the channels of every `minecraft:register` and `minecraft:unregister` from the server as ad hoc
  channels.
- It answers the `neoforge:register` probe with the channel map.
- After `neoforge:network` it registers the negotiated configuration channels.
- It answers `c:version` and `c:register`.
- It disconnects on a modded payload that the real client rejects: a payload before the
  negotiation, or a payload on a channel that is neither negotiated nor an optional clientbound
  channel of the map. A channel that the map does not have, or has as serverbound, is discarded.
- It echoes `neoforge:frozen_registry_sync_completed`, and disconnects when a registry from
  `neoforge:frozen_registry_sync_start` did not arrive.
- It always acknowledges `neoforge:feature_flags` and `neoforge:extensible_enum_data`, without a
  compare against local flags or enums. Its `neoforge:known_registry_data_maps` reply is an empty
  map. This passes only because the built-in NeoForge data maps are not mandatory.
- It sends a modded reply only on a channel that the server negotiated or registered, like
  `NetworkRegistry.checkPacket`. Otherwise it disconnects.
- It reassembles `neoforge:split` slices and handles the whole packet.
- Without a probe, `minecraft:brand`, `update_enabled_features` or `finish_configuration` starts
  the fallback for a server without NeoForge.
- Before it acknowledges `finish_configuration`, it sends `minecraft:unregister` and
  `minecraft:register` for the play phase.

As a vanilla client it sends `minecraft:brand` and the client information after the login. It
answers ping, keep-alive, cookie requests and the code of conduct. To `select_known_packs` it
replies with the offered packs in the `minecraft` namespace.

## Test against a live server

`tests/live_server.rs` has two cases against a server without native mods:

- The vanilla configuration phase against `expected/pumpkin-vanilla.txt`. With
  `detect_neoforge_clients = true`, the three payloads of the `NeoForge` probe and the
  `minecraft:register` of the vanilla connection come first, as in run (a) of the capture.
- The `NeoForge` configuration phase against `expected/pumpkin-neoforge.txt` and
  `expected/pumpkin-neoforge-registries.toml`, and the `compare` checks against run (b) of the
  capture. This case needs `detect_neoforge_clients = true`.

The tests do nothing unless `PUMPKIN_NEOFORGE_CLIENT_SERVER` is set:

```bash
# In a scratch directory, with detect_neoforge_clients = true at the top level and
# online_mode = false under [networking.java] in pumpkin.toml:
cargo run --manifest-path <repo>/Cargo.toml -p pumpkin
# In the repository:
PUMPKIN_NEOFORGE_CLIENT_SERVER=127.0.0.1:25565 cargo test -p pumpkin-neoforge-client --test live_server
```
