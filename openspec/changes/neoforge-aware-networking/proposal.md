# Proposal

## Why

A NeoForge client treats a server as vanilla when the first configuration payload it receives is `minecraft:brand`, which is what Pumpkin sends. IronPumpkin must recognise NeoForge clients and expose the configuration phase to plugins before any handshake, registry sync or NeoForge-shaped API can exist. Every later phase depends on this work.

Phase goal: IronPumpkin recognises NeoForge clients and exposes the configuration phase to plugins, with no gameplay change.

## What Changes

- Fork hygiene: upstream-only GitHub workflows are gated on the Pumpkin-MC repo; the README presents IronPumpkin.
- Status JSON: `isModded` behind the `advertise_modded` option; the secure chat key uses the vanilla name `enforcesSecureChat`.
- Codecs for the NeoForge and `c:` configuration payloads in pumpkin-protocol, with bounded lengths.
- An opt-in NeoForge probe (`detect_neoforge_clients`) at the start of the configuration phase. The server records the connection type and the channels the client declared.
- A plugin event for every configuration-phase custom payload, with a way to answer, for native and Wasm (WIT v0.2) plugins. The WIT change is additive.
- A headless Rust client (`tools/pumpkin-neoforge-client`) that speaks the vanilla and the NeoForge configuration handshake and records or asserts the payload sequence.
- Fixes of upstream Pumpkin bugs found on the way: particleStatus in client_information, the login loop after a kick, the brand decoding, and the parked bugs listed in tasks.md.

Out of scope: registry sync, custom content, any gameplay change.

## Capabilities

### New Capabilities

- `server-status`: the status JSON fields that modded and vanilla clients read.
- `neoforge-payloads`: the wire format of the NeoForge and `c:` configuration payloads.
- `neoforge-client-detection`: the configuration-phase probe, the connection type and the declared channels.
- `configuration-payload-events`: plugin access to configuration-phase custom payloads.
- `connection-lifecycle`: login and configuration behaviour shared by every Java client (client information, kick handling, brand).
- `neoforge-test-client`: the contract of the headless test client.

### Modified Capabilities

None. No specs existed before this change.

## Impact

- Crates: pumpkin-protocol (codecs, status JSON, client information), pumpkin-config (two options), pumpkin-core (`net/java/pending.rs`, `net/java/neoforge.rs`, `net/java/login/`, plugin events), pumpkin-plugin-wit v0.2 `event.wit`, pumpkin-wasm-host-v0_2, `tools/pumpkin-neoforge-client`, `.github/workflows`.
- The particleStatus fix adds a field to a published WIT record in v0.1 and v0.2 `java-packets.wit`. That is breaking for plugins built against the old record.
- With both options off, the status JSON and the configuration byte stream are identical to Pumpkin.

## Verification

- pumpkin-protocol tests decode bytes derived from the Java StreamCodecs and recorded from a NeoForge 26.3 dedicated server.
- A NeoForge 26.3 client without content mods joins and the log shows its channels; a vanilla client still joins. The real-client part is an owner play test.
