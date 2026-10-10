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
    --expected-file tools/pumpkin-neoforge-client/expected/pumpkin-neoforge.txt
cargo run -p pumpkin-neoforge-client -- assert --port 25565 --expect minecraft:brand
cargo run -p pumpkin-neoforge-client -- assert --port 25565 \
    --channels tools/pumpkin-neoforge-client/channels/required-unknown.toml \
    --expect-disconnect multiplayer.disconnect.incompatible
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

`assert --expect-disconnect <translation key>` expects a kick instead of a finished configuration.
The run passes only when the server ends the configuration with a `disconnect` whose reason is a
translate component with that key. It exits with 1 on any other end: `finish_configuration`, a
disconnect with another key or a plain text reason, a login disconnect, or a disconnect by the
client's own NeoForge checks. With `--expect-disconnect`, the expected channel list is optional;
without a list, the client does not check the channel sequence. The log shows the key, the
arguments and the English text of the reason.

Both modes exit with 2 when the run cannot complete: a connect failure, the timeout, an early end
of the stream, or a decode error in a packet or payload that the client must act on, such as a
disconnect reason that is not exactly one text component.

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

## Expected lists

- `expected/pumpkin-vanilla.txt`: a vanilla client against IronPumpkin with
  `detect_neoforge_clients = false`.
- `expected/pumpkin-neoforge.txt`: `channels/neoforge-26.3.toml` against IronPumpkin with
  `detect_neoforge_clients = true`.

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

`tests/live_server.rs` runs the vanilla configuration phase and checks it against
`expected/pumpkin-vanilla.txt`. The test does nothing unless `PUMPKIN_NEOFORGE_CLIENT_SERVER` is
set:

```bash
# In a scratch directory, with online_mode = false under [networking.java] in pumpkin.toml:
cargo run --manifest-path <repo>/Cargo.toml -p pumpkin
# In the repository:
PUMPKIN_NEOFORGE_CLIENT_SERVER=127.0.0.1:25565 cargo test -p pumpkin-neoforge-client --test live_server
```
