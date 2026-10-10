# NeoForge 26.3.0.64-beta captures

Recordings of the configuration phase against a real NeoForge 26.3.0.64-beta dedicated server
(Minecraft 26.3), made on 2026-10-10. The server ran from the official installer with no mods,
`online-mode=false` and `server-port=25599`. These files are the reference for the NeoForge wire
format: the codec tests in `crates/pumpkin-protocol/src/java/neoforge/` use payload bodies from
run (b).

| File | Client | Result |
|:--|:--|:--|
| `run-a-vanilla.jsonl.gz` | vanilla, no channel map | finishes the configuration |
| `run-b-neoforge.jsonl.gz` | `channels/neoforge-26.3.toml` | finishes the configuration |
| `run-c-required.jsonl.gz` | `run-c-channels.toml`: the NeoForge channels plus the required channel `mymod:sync` | disconnected with `multiplayer.disconnect.incompatible` |

Each file is gzipped JSONL in the format of the `--out` option, with one line per packet. The
`decode` and `summary` fields come from the client at the time of the recording, so a channel
that had no codec then shows `no_codec`. The `data` field holds the body bytes.

Record commands, run from the repository root:

```bash
cargo run -p pumpkin-neoforge-client -- record --port 25599 --username VanillaA \
    --out run-a-vanilla.jsonl
cargo run -p pumpkin-neoforge-client -- record --port 25599 --username NeoB \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml --out run-b-neoforge.jsonl
cargo run -p pumpkin-neoforge-client -- record --port 25599 --username NeoC \
    --channels tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/run-c-channels.toml \
    --out run-c-required.jsonl
gzip -n -9 run-a-vanilla.jsonl run-b-neoforge.jsonl run-c-required.jsonl
```

Read a capture with `zcat run-b-neoforge.jsonl.gz | jq -c '{seq, direction, packet, channel}'`.
