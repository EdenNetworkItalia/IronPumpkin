# Spec Delta

## Purpose

Gives native and Wasm plugins access to the custom payloads of the configuration phase, before a player exists, so they can observe them and answer.

## ADDED Requirements

### Requirement: Plugins observe configuration payloads
The server SHALL fire one configuration payload event per custom payload a client sends in the configuration phase, with the channel id, the bytes, the connection identity, the connection type and the declared channels. Status: implemented (#10).

#### Scenario: Brand payload
- **WHEN** a plugin listens for the event and a client sends `minecraft:brand`
- **THEN** the handler receives the channel `minecraft:brand`, the payload bytes, the player name and uuid, the protocol version and the connection type

### Requirement: Event after the built-in handling
The event SHALL fire after the server processed the brand and the NeoForge channel declarations, SHALL NOT be cancellable, and SHALL NOT fire when no handler is registered or the connection is closed. Status: implemented (#10).

#### Scenario: Declared channels are visible
- **WHEN** a client sends `neoforge:register` with detection on
- **THEN** the handler sees the connection type NeoForge and the channels that payload declared

### Requirement: Plugins answer with payloads
A handler SHALL be able to queue payloads on the event. The server SHALL send them to the client in queue order after all handlers ran, stop when the connection closes, and drop a payload whose channel is not a valid identifier with a warning. Status: implemented (#10).

#### Scenario: Reply reaches the client
- **WHEN** a handler queues a payload on `ip5probe:reply`
- **THEN** the client receives that payload in the configuration phase

#### Scenario: Invalid channel
- **WHEN** a handler queues a payload on a channel that is not a valid identifier
- **THEN** the payload is not sent, a warning is logged and the client stays connected

### Requirement: Wasm plugins receive the event
WIT v0.2 SHALL carry the event as an additive change. A Wasm handler SHALL be able to append responses, and its changes to the identity and channel fields SHALL be ignored. Status: implemented (#10).

#### Scenario: Wasm guest appends a response
- **WHEN** a Wasm handler appends a response and changes the channel field
- **THEN** the server sends the appended response and keeps the original channel
