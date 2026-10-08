# Spec Delta

## Purpose

Lets a ported mod register typed custom payload channels, handle them, send them to players and add configuration tasks, on top of the M3 (NeoForge handshake and registry sync) NeoForge channel negotiation.

## ADDED Requirements

### Requirement: Channel registration
During `RegisterPayloadHandlersEvent`, `payload-registrar` methods SHALL register one channel each with its direction, phase, version, optional flag and handler thread, and SHALL fail with `duplicate` for a channel id that exists and with `registration-closed` after the dispatch. Status: planned (task 2.11).

#### Scenario: Play to server
- **WHEN** a mod calls `play-to-server("examplemod:set_level", 3)` on a registrar for version "1"
- **THEN** the channel takes part in the negotiation with version "1"

### Requirement: Negotiation decides availability
The host SHALL offer every registered channel in the `neoforge:network` negotiation, SHALL disconnect a NeoForge client that lacks a required channel, and SHALL treat a channel the client did not negotiate as unavailable. Status: planned (task 2.11).

#### Scenario: Missing required channel
- **WHEN** a NeoForge client without a required mod channel connects
- **THEN** the server sends `neoforge:modded_network_setup_failed` and disconnects it

### Requirement: Receiving payloads
A payload on a registered serverbound channel SHALL reach the handler that the mod registered for the channel, with the raw bytes and a `payload-context`, on the tick thread unless the channel uses `handler-thread::network`. Status: planned (task 2.11).

#### Scenario: Handler runs
- **WHEN** a client sends a payload on `examplemod:set_level`
- **THEN** the mod's handler runs once with those bytes and `context.player()` returns the sender

### Requirement: Sending payloads
The `send-to-*` functions SHALL send a payload to the players they name and SHALL fail with `channel-not-negotiated` for a single target whose client did not negotiate the channel; broadcasts SHALL skip such players. Status: planned (task 2.11).

#### Scenario: Vanilla client
- **WHEN** a mod calls `send-to-player` for a vanilla client on a mod channel
- **THEN** the call fails with `network-error::channel-not-negotiated`

### Requirement: Payload context
`payload-context` SHALL reply on the same connection, disconnect with a reason, report the flow and protocol, and finish a configuration task by type. Status: planned (task 2.11).

#### Scenario: Reply
- **WHEN** a handler calls `reply` with a clientbound channel
- **THEN** the sender's client receives that payload

### Requirement: Configuration tasks
Tasks registered through `RegisterConfigurationTasksEvent` SHALL run in order during configuration through the run method of each task, and configuration SHALL continue only after each task finishes through `finish-current-task`. Status: planned (task 2.11).

#### Scenario: Licence handshake
- **WHEN** a mod registers a task that sends a challenge and finishes on the client's answer
- **THEN** the player enters play only after the answer arrives

### Requirement: Raw configuration payloads stay in v0.2
Configuration payloads before or outside the negotiation SHALL keep reaching the v0.2 `config-custom-payload-event`; this interface SHALL NOT define a second event for them. Status: planned (task 2.11).

#### Scenario: Unregistered channel
- **WHEN** a client sends a configuration payload on a channel no mod registered here
- **THEN** listeners of the v0.2 `config-custom-payload-event` receive it
