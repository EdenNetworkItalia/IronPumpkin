# Spec Delta

## Purpose

Lets a ported mod register custom payload channels in `ironpumpkin-mods`, handle them per channel, send them to players, all players or a dimension, and add configuration tasks, on top of the M3 (NeoForge handshake and registry sync) channel negotiation (`NegotiatedState`, `NetworkRegistry::register`, `c:version`, `c:register` and `neoforge:split`). The API is the module `ironpumpkin_mods::network`.

## ADDED Requirements

### Requirement: Channel registration
A mod SHALL register its channels in a handler of the Pumpkin event `RegisterPayloadHandlersEvent`, which fires once after `load_complete` and before the synced configs load. A `PayloadRegistrar` method SHALL register one channel with its direction, phase, version, optional flag and thread, or fail with `Duplicate` for a known channel and `RegistrationClosed` later. Status: supported (task 2.11, #116, test `registration_refuses_what_neoforge_refuses_and_closes_after_the_step`).

#### Scenario: Play to server
- **WHEN** a mod calls `play_to_server("examplemod:set_level", handler)` on a registrar for version "1"
- **THEN** the channel takes part in the negotiation with version "1"

#### Scenario: Common channel
- **WHEN** a mod calls `common_bidirectional`, `common_to_server` or `common_to_client`
- **THEN** the channel takes part in the negotiation of the configuration and play phases, and a `configuration_*` channel only in the configuration phase (test `common_channels_register_in_both_phases_and_configuration_channels_in_one`)

### Requirement: Negotiation decides availability
The host SHALL offer every registered channel in the `neoforge:network` negotiation, SHALL disconnect a NeoForge client that lacks a required channel, and SHALL treat a channel the client did not negotiate as unavailable. Status: supported (task 2.11, #116, tests `a_neoforge_client_without_a_required_mod_channel_fails_the_negotiation`, `send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it`).

#### Scenario: Missing required channel
- **WHEN** a NeoForge client without a required mod channel connects
- **THEN** the server sends `neoforge:modded_network_setup_failed` and disconnects it

### Requirement: Receiving payloads
A payload on a registered serverbound channel SHALL reach the handler of its channel once, with the raw bytes and a `PayloadContext`, on the world tick of the sender unless the channel uses `HandlerThread::Network`, which runs on the connection task. A configuration payload SHALL run on the connection task. Status: supported (task 2.11, #116, tests `a_play_payload_reaches_the_handler_of_its_channel_once`, `a_network_thread_handler_runs_on_the_read_loop_and_a_main_handler_waits_for_the_tick`).

#### Scenario: Handler runs
- **WHEN** a client sends a payload on `examplemod:set_level`
- **THEN** the mod's handler runs once with those bytes and `context.player()` returns the sender

### Requirement: Unnegotiated channels
A payload on a required mod channel that the client did not negotiate SHALL disconnect the client with `multiplayer.disconnect.incompatible`, as `NetworkRegistry.handleModdedPayload` does. An optional channel SHALL stay readable without the negotiation. Status: supported (task 2.11, #116, test `a_payload_on_a_required_channel_the_client_did_not_negotiate_disconnects_it`).

#### Scenario: Payload on a required channel without negotiation
- **WHEN** a vanilla client sends a payload on a required mod channel
- **THEN** the handler does not run and the server disconnects the client with "No Channel for" and the channel

#### Scenario: Payload on an optional channel without negotiation
- **WHEN** a vanilla client sends a payload on an optional mod channel
- **THEN** the handler runs and the client stays connected

#### Scenario: Payload before the negotiation
- **WHEN** a client sends a payload on a mod channel before its `neoforge:register` reply or the probe pong
- **THEN** the handler does not run and the server disconnects the client with "No Payload Setup" (test `a_mod_payload_before_the_negotiation_disconnects_with_no_payload_setup`)

#### Scenario: Payload on a client-bound channel
- **WHEN** a client sends a payload on a client-bound mod channel, also before the negotiation
- **THEN** the server drops it: no handler, no plugin event and no disconnect (test `a_payload_on_a_client_bound_mod_channel_is_dropped`)

### Requirement: Sending payloads
`send_to_player`, `send_to_all_players` and `send_to_players_in_dimension` SHALL send a payload to the players they name, and `send_to_player` SHALL fail with `NetworkError::ChannelNotNegotiated` for a client that did not negotiate the channel; the broadcasts SHALL skip such players. Status: supported (task 2.11, #116, test `send_to_player_fails_for_a_vanilla_client_and_broadcasts_skip_it`).

#### Scenario: Vanilla client
- **WHEN** a mod calls `send_to_player` for a vanilla client on a mod channel
- **THEN** the call fails with `NetworkError::ChannelNotNegotiated`

### Requirement: Payload context
`PayloadContext` SHALL reply on the same connection, disconnect with a reason, report the flow and protocol, and finish a configuration task by type. Status: supported (task 2.11, #116, tests `a_reply_reaches_the_client_of_the_sender`, `a_handler_disconnects_the_sender`, `a_play_payload_reaches_the_handler_of_its_channel_once`, `a_finish_request_while_no_mod_task_waits_disconnects_the_client`).

#### Scenario: Reply
- **WHEN** a handler calls `reply` with a clientbound channel
- **THEN** the sender's client receives that payload

### Requirement: Configuration tasks
Tasks that a mod registers in a handler of the Pumpkin event `RegisterConfigurationTasksEvent` SHALL run in registration order after the tasks of NeoForge and before `JoinWorldTask`, and configuration SHALL continue only after each task finishes. `ConfigurationTask` SHALL have a dynamic variant for a mod task (`ConfigurationTask::Mod`). Status: supported (task 2.11, #116, tests `mod_tasks_run_after_the_neoforge_tasks_in_registration_order`, `neoforge_network_with_the_headless_client`).

#### Scenario: Licence handshake
- **WHEN** a mod registers a task that sends a challenge and finishes on the client's answer
- **THEN** the player enters play only after the answer arrives

#### Scenario: Two mod tasks
- **WHEN** two mods each register a configuration task
- **THEN** the tasks run one after the other in registration order, and configuration continues after the second finishes

### Requirement: Mod channels are known to the reply check
A finish request from the handler of a mod channel SHALL finish the running mod task of its type, and a request when the current task has another type SHALL disconnect the client with `disconnect.genericReason` and the `IllegalStateException` of `finishCurrentTask`, as `Connection.exceptionCaught` reports it. Status: supported (task 2.11, #116, tests `a_finish_request_while_no_mod_task_waits_disconnects_the_client`, `neoforge_network_with_the_headless_client`).

#### Scenario: Reply to a mod task
- **WHEN** the client answers the challenge of a running mod task on the mod channel
- **THEN** the server finishes the task and does not kick the player

#### Scenario: Unexpected reply
- **WHEN** a client sends a configuration reply on a mod channel while no task of that mod waits for it
- **THEN** the server disconnects the client with "Internal Exception: java.lang.IllegalStateException: Unexpected request for task finish"

### Requirement: Raw configuration payloads stay in v0.2
Configuration payloads before or outside the negotiation SHALL keep reaching the v0.2 `config-custom-payload-event`; this interface SHALL NOT define a second event for them. A payload on a mod channel SHALL reach only the handler of its channel. Status: supported (task 2.11, #116, test `a_configuration_payload_on_an_unregistered_channel_reaches_the_plugin_event`).

#### Scenario: Unregistered channel
- **WHEN** a client sends a configuration payload on a channel no mod registered here
- **THEN** listeners of the v0.2 `config-custom-payload-event` receive it
