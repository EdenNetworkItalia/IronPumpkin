# pumpkin-neoforge-client

A headless Minecraft Java client for the configuration phase. It connects to a server, logs in
in offline mode and answers the configuration phase like the vanilla client. With a channel map it
also answers like a NeoForge 26.3 client. It records every clientbound configuration packet and
decodes each custom payload with the codecs in `pumpkin-protocol/src/java/neoforge`.

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

Options for both modes:

| Option | Default | Meaning |
|:--|:--|:--|
| `--host`, `--port` | `127.0.0.1`, `25565` | Server address. |
| `--username` | random `ProbeNNNNNN` | Offline player name. |
| `--channels <file>` | none | Channel map. Without it the client is a vanilla client. |
| `--brand <name>` | `neoforge` with a channel map, else `vanilla` | Value of `minecraft:brand`. |
| `--timeout <s>` | `60` | Time limit for login and configuration. |
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
