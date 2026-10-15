# IronPumpkin captures

Recordings of the headless client against IronPumpkin, for a comparison with the NeoForge
26.3.0.64-beta captures in `../neoforge-26.3.0.64-beta/`. The format is the one of `--out`,
gzipped.

| File | Client | Result |
|:--|:--|:--|
| `play-slice2.jsonl.gz` | `channels/neoforge-26.3.toml`, `play` mode with `scripts/slice2.toml` | runs join, chat, break, hold, place and leave |

`play-slice2.jsonl.gz` was recorded on 2026-10-15 with `cargo run -p pumpkin` at commit `a2753a0ec`,
on a new world, with this `pumpkin.toml`: `seed = "parity"`, `detect_neoforge_clients = true`,
`default_gamemode = "Creative"`, `spawn_protection = 0`, and `online_mode = false` under
`[networking.java]`. The player entered play at block 0 118 0. The NeoForge recording of the same
script is `../neoforge-26.3.0.64-beta/play-slice2.jsonl.gz`.

```bash
cargo run -p pumpkin-neoforge-client -- play --port 25598 --username IronSlice2 \
    --channels tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml \
    --script tools/pumpkin-neoforge-client/scripts/slice2.toml --out play-slice2.jsonl
gzip -n -9 play-slice2.jsonl
```

Read the steps with `zcat play-slice2.jsonl.gz | jq -c 'select(.state == "play") | {step, direction, packet}'`.
