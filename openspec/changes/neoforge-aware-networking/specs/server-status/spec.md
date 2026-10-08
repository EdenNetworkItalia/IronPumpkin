# Spec Delta

## Purpose

Defines the status (server list ping) JSON fields that vanilla and NeoForge clients read, so that both show the server correctly.

## ADDED Requirements

### Requirement: Modded flag in the status response
The status JSON SHALL contain `"isModded": true` when the `advertise_modded` option is on, and SHALL be identical to Pumpkin's status JSON when it is off. Status: implemented (#11).

#### Scenario: Option on
- **WHEN** `advertise_modded = true` and a client sends a status request
- **THEN** the status JSON contains the key `isModded` with the value `true`

#### Scenario: Option off
- **WHEN** `advertise_modded = false` (the default) and a client sends a status request
- **THEN** the status JSON has no `isModded` key and is byte-identical to Pumpkin's

### Requirement: Vanilla secure chat key
The status JSON SHALL carry the secure chat flag under the vanilla key `enforcesSecureChat`. Status: implemented (#13); the in-game check waits for the owner.

#### Scenario: Key name
- **WHEN** a client sends a status request
- **THEN** the status JSON contains `enforcesSecureChat` and does not contain `enforceSecureChat`

#### Scenario: Real client
- **WHEN** a vanilla 26.3 client joins a server with default settings
- **THEN** the client shows no secure-chat warning toast
