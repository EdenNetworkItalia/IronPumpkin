# Tasks

Each item mirrors one GitHub issue on EdenNetworkItalia/IronPumpkin. Close the issue and check the item in the same transition.

## 1. Fork hygiene

- [x] 1.1 Gate upstream-only workflows on the Pumpkin-MC repo (#6); verify: YAML parse of all workflows, empty diff on rust.yml, typos.yml and nix.yml
- [x] 1.2 Rewrite the README for IronPumpkin (#7); verify: README credits Pumpkin-MC/Pumpkin and lists the roadmap unchecked

## 2. Status JSON

- [x] 2.1 Advertise isModded behind `advertise_modded` (#11); verify: the two connection_cache status tests and a status ping with the option off and on
- [x] 2.2 Send the vanilla key enforcesSecureChat (#13); verify: `status_uses_the_vanilla_enforces_secure_chat_key` fails without the fix

## 3. NeoForge payload codecs

- [x] 3.1 Codecs for the NeoForge and `c:` configuration payloads in pumpkin-protocol (#8); verify: `cargo test -p pumpkin-protocol` decode, write and hostile-length tests

## 4. NeoForge client detection

- [x] 4.1 Opt-in probe, connection type and declared channels (#9); verify: raw-socket boot test, option off byte-identical to Pumpkin, option on logs the connection type

## 5. Configuration payload events

- [x] 5.1 Plugin event and response queue for configuration payloads, native and WIT v0.2 (#10); verify: host unit tests and a boot test where a native plugin logs `minecraft:brand` and its reply reaches the client

## 6. Headless test client

- [x] 6.1 `tools/pumpkin-neoforge-client` with record and assert modes (#12); verify: modded run against a NeoForge 26.3 server decodes every payload, vanilla assert against IronPumpkin passes

## 7. Login and configuration fixes

- [x] 7.1 Read particleStatus in both client_information packets (#15); verify: decode and encode tests against vanilla `ClientInformation.write()` bytes
- [x] 7.2 Stop the login sequence loop after a kick (#14); verify: a regression test or boot check with a buffered pong after a malformed `neoforge:register`
- [x] 7.3 Decode the configuration brand as a length-prefixed string (#28); verify: a decode test from captured bytes fails without the fix
- [ ] 7.4 Decode translate components with plain string arguments from NBT (#25, parked with `later`); verify: a regression test from the captured NeoForge disconnect reason
- [ ] 7.5 Kick the old session on duplicate_login and drop closed configuration sessions at once (#26, parked with `later`); verify: boot test with two logins of the same name
- [x] 7.6 Offline-mode UUIDs equal Java `nameUUIDFromBytes` (#27); verify: a test vector computed by Java

## 8. Channel negotiation

- [ ] 8.1 Negotiate NeoForge channels with `neoforge:network` after the probe (#31); verify: negotiator unit tests, headless client assert against `expected/pumpkin-neoforge.txt`, the `channels/required-unknown.toml` run gets `neoforge:modded_network_setup_failed` and a disconnect, vanilla assert with the option off

## 9. Phase verification

- [ ] 9.1 Owner play test with real clients (#9): a NeoForge 26.3 client without content mods joins with `detect_neoforge_clients = true` and the log shows its channels; a vanilla client still joins

## Workflow follow-up

- The owner opens the upstream PRs drafted in #13 and #15 on Pumpkin-MC/Pumpkin, and later those of the other upstream bugs.
- Archive this change with `openspec archive neoforge-aware-networking` when phase 1 lands.
