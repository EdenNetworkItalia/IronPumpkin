# Spec Delta

## Purpose

Lets a ported mod register custom payload channels in `ironpumpkin-mods`, handle them per channel, send them to players, all players or a dimension, and add configuration tasks, on top of the M3 (NeoForge handshake and registry sync) channel negotiation (`NegotiatedState`, `NetworkRegistry::register`, `c:version`, `c:register` and `neoforge:split`).

## ADDED Requirements

### Requirement: Channel registration
A mod SHALL register its payload channels during its registration step (the `NativeMod` method that stands for `RegisterPayloadHandlersEvent`). The `payload-registrar` methods SHALL register one channel each with its direction, phase, version, optional flag and handler thread, and SHALL fail with `duplicate` for a channel id that exists and with `registration-closed` after that step. Status: planned (task 2.11).

#### Scenario: Play to server
- **WHEN** a mod calls `play-to-server("examplemod:set_level", 3)` on a registrar for version "1"
- **THEN** the channel takes part in the negotiation with version "1"

### Requirement: Negotiation decides availability
The host SHALL offer every registered channel in the `neoforge:network` negotiation, SHALL disconnect a NeoForge client that lacks a required channel, and SHALL treat a channel the client did not negotiate as unavailable. Status: planned (task 2.11).

#### Scenario: Missing required channel
- **WHEN** a NeoForge client without a required mod channel connects
- **THEN** the server sends `neoforge:modded_network_setup_failed` and disconnects it

### Requirement: Receiving payloads
A payload on a registered serverbound channel SHALL reach the handler that the mod registered for the channel (per-channel dispatch), with the raw bytes and a `payload-context`, on the tick thread unless the channel uses `handler-thread::network`. The tick thread is the world tick of the sender, and the network thread is the connection task. Status: planned (task 2.11).

#### Scenario: Handler runs
- **WHEN** a client sends a payload on `examplemod:set_level`
- **THEN** the mod's handler runs once with those bytes and `context.player()` returns the sender

### Requirement: Sending payloads
The `send-to-*` functions (to one player, to all players and to a dimension) SHALL send a payload to the players they name and SHALL fail with `channel-not-negotiated` for a single target whose client did not negotiate the channel; broadcasts SHALL skip such players. Status: planned (task 2.11).

#### Scenario: Vanilla client
- **WHEN** a mod calls `send-to-player` for a vanilla client on a mod channel
- **THEN** the call fails with `network-error::channel-not-negotiated`

### Requirement: Payload context
`payload-context` SHALL reply on the same connection, disconnect with a reason, report the flow and protocol, and finish a configuration task by type. Status: planned (task 2.11).

#### Scenario: Reply
- **WHEN** a handler calls `reply` with a clientbound channel
- **THEN** the sender's client receives that payload

### Requirement: Configuration tasks
Tasks that a mod registers in its configuration task step (the `NativeMod` method that stands for `RegisterConfigurationTasksEvent`) SHALL run in order during configuration through the run method of each task, and configuration SHALL continue only after each task finishes through `finish-current-task`. `ConfigurationTask` SHALL have a dynamic variant for a mod task, because it is a `Copy` enum with `const ALL` today. Status: planned (task 2.11).

#### Scenario: Licence handshake
- **WHEN** a mod registers a task that sends a challenge and finishes on the client's answer
- **THEN** the player enters play only after the answer arrives

#### Scenario: Two mod tasks
- **WHEN** two mods each register a configuration task
- **THEN** the tasks run one after the other in registration order, and configuration continues after the second finishes

### Requirement: Mod channels are known to the reply check
The kick for an unexpected configuration reply SHALL know the mod channels: a reply on a mod channel SHALL finish the running mod task that waits for it, and a reply when the current task does not wait for it SHALL disconnect the client. Status: planned (task 2.11).

#### Scenario: Reply to a mod task
- **WHEN** the client answers the challenge of a running mod task on the mod channel
- **THEN** the server finishes the task and does not kick the player

#### Scenario: Unexpected reply
- **WHEN** a client sends a configuration reply on a mod channel while no task of that mod waits for it
- **THEN** the server disconnects the client

### Requirement: Raw configuration payloads stay in v0.2
Configuration payloads before or outside the negotiation SHALL keep reaching the v0.2 `config-custom-payload-event`; this interface SHALL NOT define a second event for them. Status: planned (task 2.11).

#### Scenario: Unregistered channel
- **WHEN** a client sends a configuration payload on a channel no mod registered here
- **THEN** listeners of the v0.2 `config-custom-payload-event` receive it
