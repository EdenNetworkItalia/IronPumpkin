# Spec Delta

## Purpose

Defines the ordered configuration tasks of a Java connection: which tasks run, in which order, which client reply each one waits for, and the guarantee that the vanilla configuration byte stream does not change.

## ADDED Requirements

### Requirement: Task order
The server SHALL send the brand, the server links and `update_enabled_features`, then run the configuration tasks in this order: registry sync, known packs, resource pack, common version and common register, sync config, data map negotiation, extensible enum check, feature flag check, finish. A task whose condition does not hold SHALL be skipped. The known packs task SHALL send only `select_known_packs`. Status: planned (T1).

#### Scenario: NeoForge client
- **WHEN** the headless client joins in NeoForge mode with `channels/neoforge-26.3.toml` and `detect_neoforge_clients = true`
- **THEN** after `minecraft:brand` the clientbound payloads are `neoforge:frozen_registry_sync_start`, the `neoforge:frozen_registry` payloads, `neoforge:frozen_registry_sync_completed`, then `select_known_packs`, the registry data and `update_tags`, then `c:version`, `c:register`, `neoforge:config_file`, `neoforge:known_registry_data_maps`, `neoforge:extensible_enum_data`, `neoforge:feature_flags` and `finish_configuration`

#### Scenario: Resource pack after known packs
- **WHEN** a server with a configured resource pack and `detect_neoforge_clients = false` configures a vanilla client
- **THEN** the client receives `update_enabled_features`, then `select_known_packs`, the registry data and `update_tags`, then the resource pack request

#### Scenario: Vanilla client with detection on
- **WHEN** a vanilla client joins with `detect_neoforge_clients = true` and no client-required mod is loaded
- **THEN** it receives no registry sync and no modded configuration payload, and the configuration finishes after `update_tags`

### Requirement: Task conditions
The registry sync task SHALL run only for a NeoForge connection that declared `neoforge:frozen_registry_sync_start`, `neoforge:frozen_registry` and `neoforge:frozen_registry_sync_completed`. The common version and common register tasks SHALL run only when the client declared the ad hoc channels `c:version` and `c:register`. The sync config task SHALL run only when the client declared `neoforge:config_file`. Status: planned (T1).

#### Scenario: Client without the c channels
- **WHEN** a NeoForge client's `minecraft:register` does not contain `c:version`
- **THEN** the server sends neither `c:version` nor `c:register`, and the next task starts

### Requirement: One reply per task
A task SHALL either finish when it has sent its payloads, or wait for exactly one named client reply. The queue SHALL start the next task only when the current task finishes. Status: planned (T1).

#### Scenario: Wait for the echo
- **WHEN** the registry sync task has sent `neoforge:frozen_registry_sync_completed` and the client has not echoed it
- **THEN** the server sends no `select_known_packs` until the echo arrives

### Requirement: Unexpected replies disconnect
A reply that a task waits for SHALL be a protocol error when it arrives while that task is not the current task. The server SHALL disconnect the client on that error and log the channel and the current task. Status: planned (T1).

#### Scenario: Early acknowledgement
- **WHEN** a client sends `neoforge:feature_flags_ack` while the registry sync task waits for its echo
- **THEN** the server disconnects the client

### Requirement: Probe unchanged
The phase 1 probe, the channel negotiation and the pong handling SHALL stay before the queue. The queue SHALL start after the brand and the server links, which follow the first pong 0. Status: planned (T1).

#### Scenario: Probe order
- **WHEN** detection is on and a client acknowledges the login
- **THEN** the first four configuration packets are still `minecraft:unregister`, `minecraft:register`, `neoforge:register` and ping 0

### Requirement: Vanilla byte stream unchanged
With `detect_neoforge_clients = false` and no resource pack configured, the configuration packets a client receives SHALL be byte-identical to the packets the server sends before the task queue exists. Status: planned (T1).

#### Scenario: Detection off
- **WHEN** the headless client joins in vanilla mode with `detect_neoforge_clients = false`
- **THEN** the assertion against `expected/pumpkin-vanilla.txt` passes and the recorded bodies equal a recording made on the commit before the queue
