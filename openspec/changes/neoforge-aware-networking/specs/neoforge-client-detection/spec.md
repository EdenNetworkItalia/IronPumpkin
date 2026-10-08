# Spec Delta

## Purpose

Defines how the server tells NeoForge clients from other clients at the start of the configuration phase, and what it records about the channels they declare.

## ADDED Requirements

### Requirement: Detection is opt-in
The NeoForge probe SHALL run only when the `detect_neoforge_clients` option is on. With the option off the configuration byte stream SHALL be identical to Pumpkin's. Status: implemented (#9).

#### Scenario: Option off
- **WHEN** `detect_neoforge_clients = false` (the default) and any client logs in
- **THEN** the configuration packets the client receives are byte-identical to Pumpkin's

### Requirement: Probe before the brand
With detection on, the server SHALL send `minecraft:unregister`, `minecraft:register` with the NeoForge builtin channels, an empty `neoforge:register` and a ping with id 0 before any other configuration packet, and SHALL start the vanilla configuration sequence on the first pong 0. Status: implemented (#9).

#### Scenario: Probe order
- **WHEN** detection is on and a client acknowledges the login
- **THEN** the first four configuration packets are the unregister, the register, the empty `neoforge:register` and ping 0, and the vanilla sequence follows the pong

### Requirement: Connection type and declared channels
The server SHALL record a client that answers `neoforge:register` as NeoForge with its channel map per protocol, record any other client as Other, and log the type and channel count at info. Status: implemented (#9); the real-client scenario waits for an owner play test.

#### Scenario: Simulated NeoForge client
- **WHEN** a client answers the probe with a `neoforge:register` that declares 3 channels
- **THEN** the log shows `connection type: NeoForge, 3`

#### Scenario: Vanilla client
- **WHEN** a client ignores the probe and answers the ping
- **THEN** the log shows `connection type: Other, 0` and the client reaches play

#### Scenario: Real NeoForge client
- **WHEN** a NeoForge 26.3 client without content mods joins with detection on
- **THEN** the log shows `connection type: NeoForge, N NeoForge channels declared` with N greater than 0

### Requirement: Bounded ad hoc channels
The server SHALL keep at most 1024 channels declared through `minecraft:register` per connection and SHALL disconnect a client that declares more. Status: implemented (#9).

#### Scenario: Over the cap
- **WHEN** a client's `minecraft:register` payloads declare channel number 1025
- **THEN** the packet fails and the client is disconnected

### Requirement: Malformed declarations disconnect
A malformed `neoforge:register`, `minecraft:register` or `minecraft:unregister` SHALL fail the packet and disconnect the client, like a decoder error. Status: implemented (#9).

#### Scenario: Trailing byte
- **WHEN** a client sends a `neoforge:register` with one byte after its last field
- **THEN** the client is disconnected with a "bytes left over" reason

### Requirement: Unknown channels are logged
A configuration-phase payload on a channel the server does not handle SHALL be logged at debug level with its channel id. Status: implemented (#9).

#### Scenario: Unknown channel
- **WHEN** a client sends a payload on `testmod:unknown` during configuration
- **THEN** the debug log contains `testmod:unknown` and the connection continues

### Requirement: Server channel registry
The server SHALL keep a registry of the modded channels it speaks, per protocol (configuration and play), with version, flow and optional flag. It SHALL start with the payloads NeoForge 26.3 registers in `NetworkInitialization` and `GenericPacketSplitter`, and plugins SHALL be able to add channels. Status: implemented (#31).

#### Scenario: Plugin channel
- **WHEN** a native plugin calls `context.server.network_registry.register` with a new channel id
- **THEN** the next connection negotiates that channel, and a `minecraft` namespace, a blank version or a duplicate id is refused with an error

### Requirement: Channel negotiation after the probe
When the client's `neoforge:register` reply to the probe arrives, the server SHALL negotiate its channels against the registry like `NetworkComponentNegotiator`. On success it SHALL send `neoforge:network` and then `minecraft:register` with the builtin and negotiated configuration channels. On failure it SHALL send `neoforge:modded_network_setup_failed` and disconnect. Status: implemented (#31); the real-client scenario waits for an owner play test.

#### Scenario: NeoForge client negotiates
- **WHEN** a client answers the probe with the channel map `tools/pumpkin-neoforge-client/channels/neoforge-26.3.toml`
- **THEN** the clientbound payloads are `minecraft:unregister`, `minecraft:register`, `neoforge:register`, `neoforge:network`, `minecraft:register`, `minecraft:brand`, the setup has 11 configuration and 9 play channels, and the configuration finishes

#### Scenario: Required channel missing on the server
- **WHEN** a client declares the required configuration channel `probe:required`, which the server does not have
- **THEN** the client receives `neoforge:modded_network_setup_failed` with `probe:required` and the reason `neoforge.network.negotiation.failure.missing.client.server`, then a disconnect, and no `minecraft:brand`

#### Scenario: Optional channel dropped
- **WHEN** a client declares the optional channel `probe:fake`, which the server does not have
- **THEN** `neoforge:network` and the second `minecraft:register` do not contain `probe:fake` and the configuration finishes

#### Scenario: Late reply
- **WHEN** a client sends `neoforge:register` after the probe pong or a second time
- **THEN** the server logs it and disconnects the client

#### Scenario: Real NeoForge client
- **WHEN** a NeoForge 26.3 client without content mods joins with detection on
- **THEN** it passes the configuration phase without a channel error and reaches the world

### Requirement: Channels for other clients
With detection on, a client that did not answer `neoforge:register` SHALL receive, after the pong and before the brand, a `minecraft:register` with the builtin channels plus the optional configuration channels that accept serverbound payloads, like `initializeOtherConnection`. Status: implemented (#31).

#### Scenario: Vanilla client
- **WHEN** a client ignores the probe and answers the ping
- **THEN** it receives one more `minecraft:register` before `minecraft:brand`, with the 7 builtin channels plus the 5 optional serverbound configuration channels, and the configuration finishes
