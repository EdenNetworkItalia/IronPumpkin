# Consolidated review of `modded-gameplay-parity`

Goldfish review of the EGM plugin, review mode, 2026-11-23. Every reviewer ran on Opus with effort high, in a fresh context. The stage reports are local files of this folder (`*_report.md`); git keeps only this file.

| Stage | Reviewer | Blocker | Major | Nit |
|:--|:--|--:|--:|--:|
| 5 | zoom-out | 0 | 11 | 10 |
| 5 | inventive catalyst | 0 | 6 | 5 |
| 6 | comprehension | 0 | 9 | 12 |
| 7 | design critic | 0 | 18 | 5 |
| 7 | structural critic | 1 | 7 | 13 |
| 7 | style critic | 0 | 6 | ~30 |
| 8 | intermediate zoom-out | 0 | 5 | 4 |
| 9 | implementation readiness | 3 | 22 | 8 |
| 9 | codebase details | 1 | 10 | ~12 |
| 9 | style (tasks and specs) | 0 | 9 | 14 |
| 9 | re-audit after fixes | 0 | 8 | 7 |

## Findings by theme, deduplicated

| Theme | Priority | Stage ids | Resolution |
|:--|:--|:--|:--|
| Scope rule for zero-demand rows | Major | Z1, D15 | Fixed. Owner decision (AskUserQuestion, 2026-11-23): implement a row with demand >= 1 mod or an `examples/modpack` need; helper registration counts. Recorded in the decision log of #29. |
| Policy for vanilla clients per registry | Major | I1, D5, D6, Z8-4, C13 | Fixed. One policy per registry spec; the encoder list includes the tags packet, the statistics packet and the command tree (`CommandTreeCleaner`); option-less display particles. |
| Client-optional mods and the sync rows | Blocker | D10, Z8-1, CB-B1 | Fixed. NeoForge semantics: a NeoForge client needs every mod with synced content; the rejected alternative is recorded. |
| Sync row rule and invariant | Major | Z12, D6, D7, D8, N-M1 | Fixed. A row only for a synced registry whose ids a packet carries (tags packet counts); one id function for rows and encoders; NeoForge built-in names from a capture; `command_argument_type` gets a row; POI ids stay out of the tags packet. |
| Two registry shapes | Major | Z2, I7, R-M4, CB-M7, N-M4 | Fixed. Id table in `pumpkin-data` for synced registries; `DecoderRegistry<T>` in `pumpkin-util` for server-only ones; the `Custom` arm trait in `pumpkin-util` with a context trait from `pumpkin-core`. |
| Hook mechanism | Major | Z5, I4, C3, C4, D1, D2, M1 (structural) | Fixed. Per-hook slot tables from builder registration; vanilla answers in functions that resolve M2 typed blocks; no upstream trait methods. |
| Block entity types | Major | Z3, Z14, D3, D4, D12, CB-M2, CB-M3, N-M2 | Fixed. No `ticks` flag; full-name helper owned by #192 (new upstream bug: the chunk packet leaked saved NBT); creation and valid sets with the M4 first-wins rule; the event sets creation for a mod block without one. |
| Menus and data slots | Major | I8, D11, CB-M1 | Fixed; I8 contested with evidence: NeoForge sends the advanced payload for every value (`ServerPlayer.java:324-329`). The `i32` path and a new `MenuError` variant are named. |
| Recipes | Major | I2, Z9, Z15, Z16, D13, R-M7 | Fixed. One snapshot swapped by `set_recipes`; serializers register a stream encoder; vanilla types cannot be listed; rows decided at the freeze; a bench with a master baseline. |
| Game rules | Major | Z10, M12, Z8-5, CB-M10 | Fixed. Native event carries the mod rule id; no new WIT record (no plugin demand). |
| Mod-defined registries and holder sets | Major | D9, Z8-2, CB-M6, CB-M8, CB-M9 | Fixed. Server-only `CustomRegistry<T>`; `sync(true)` fails at the freeze; callbacks attach to the mod tables; custom holder sets go on the wire as direct lists. |
| Fluids | Major | I3, Z4, Z17, I9, R-B3, R-M14, R-M15, R-M22 | Fixed. One bucket helper and one cauldron helper; `FluidType` limited to engine reads; the census removed; one trigger for step two; the water assumptions named; link checks with tests. |
| World generation | Major | I5, I6, Z6, D14, D17, Z8-3, R-M17, R-M18, R-M19 | Fixed. JSON is the public contract; the reader extends the runtime parsers with a differential test (derives on the feature structs); `load_all` moves before the world loop; carvers, biomes and structures in the last design pass. |
| Chunk tickets | Major | R-M13 | Fixed. `forceTicks` chunks count for natural spawning through `count_natural_spawn_chunks`. |
| Alternatives rule | Blocker | B1 (structural), D18 | Fixed. Every decision has a "Rejected alternatives" block with the trade-off. |
| Inventory editing and location | Blocker | R-B1 | Fixed in M4: the inventory header and its section say hand-written; M4 task 2.52 moves the inventory to `docs/neoforge-inventory/` before the archive. |
| Example mod per wave | Blocker | R-B2 | Fixed. One ported mod in task 6.39, chosen by a fixed rule. |
| Per-task API contracts and files | Major | R-M1 | Contested: the "Wave planning" section fixes them in each task's issue, as M4 did. |
| Upstream file table | Major | Z7, M3 (structural), D16, CB-M4, CB-M5 | Fixed. One row per decision; sites corrected against NeoForge 26.3 (no sheep or snow golem ability sites; villager pickup for plantables). |
| Saves of absent mods | Major | N-M3 | Fixed. Migration section; POI and stats keep unknown names. |
| Risks: security, scalability, failure modes | Major | M6 (structural) | Fixed. |
| Terms, counts and wording | Major and Nit | C1 to C21, S1 to S6, T1 to T9 | Fixed. Terms section, row map, one word per concept, task form with sub-bullets, no history and no planning text in the specs. |

## Resolution

- Every Blocker is fixed. Every Major is fixed, except R-M1, contested with the "Wave planning" section, and I8, contested with the NeoForge reference.
- The owner made one decision during the review: the scope rule. The session made the other choices under the project rule of NeoForge semantics with the Pumpkin shape, and each one carries its rejected alternatives in design.md.
- Bugs found outside the change and filed: #189, #190, #191, #192 (upstream) and the stale inventory text of M4 (fixed in `b039a2958` and `fe443d660`).
- `openspec validate modded-gameplay-parity --strict` and `openspec validate --all` pass. Every requirement heading of the specs is cited by a task.
