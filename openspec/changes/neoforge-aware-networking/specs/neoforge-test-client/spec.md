# Spec Delta

## Purpose

Defines the headless client that drives and asserts the vanilla and NeoForge configuration handshakes, so protocol work is checked against real servers instead of memory.

## ADDED Requirements

### Requirement: Vanilla and NeoForge handshakes
The headless client SHALL log in offline and run the configuration phase as a vanilla client, or as a NeoForge 26.3 client when given a channel map, until it reaches play. Status: implemented (#12).

#### Scenario: NeoForge server
- **WHEN** the client runs in NeoForge mode against a NeoForge 26.3.0.52-beta dedicated server
- **THEN** it finishes the configuration and every received payload that has a codec decodes without error

#### Scenario: IronPumpkin server
- **WHEN** the client runs in vanilla mode against a booted IronPumpkin with `online_mode = false`
- **THEN** it finishes the configuration and receives `minecraft:brand`

### Requirement: Record and assert
The client SHALL record every configuration packet to stdout and JSONL, and in assert mode SHALL compare the received payload channels with an expected list. Exit code 2 SHALL mean the run could not complete. Status: implemented (#12).

#### Scenario: Mismatch
- **WHEN** assert mode receives a payload sequence that differs from the expected list
- **THEN** the client exits with a non-zero code

#### Scenario: Unreachable server
- **WHEN** the server address does not answer within `--timeout`
- **THEN** the client exits with code 2 after the timeout

### Requirement: Live test is opt-in
The crate's live server test SHALL pass without doing anything unless `PUMPKIN_NEOFORGE_CLIENT_SERVER` is set. Status: implemented (#12).

#### Scenario: Variable unset
- **WHEN** the test suite runs without `PUMPKIN_NEOFORGE_CLIENT_SERVER`
- **THEN** the live test passes without opening a connection
