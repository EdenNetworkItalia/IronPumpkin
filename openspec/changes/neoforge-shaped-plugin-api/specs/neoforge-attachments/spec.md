# Spec Delta

## Purpose

Lets mods store their own data on entities, block entities, chunks and levels through NeoForge attachment types, with saving, copy on death and client sync handled by the host.

## ADDED Requirements

### Requirement: Attachment types are registry entries
An attachment type SHALL be registered in `neoforge:attachment_types` through `registration`, and holder calls SHALL name it by id; an unknown id SHALL fail with `unknown-type`. Status: planned (task 2.14).

#### Scenario: Unknown type
- **WHEN** a mod calls `get-data` with an attachment type id nobody registered
- **THEN** the call fails with `attachment-error::unknown-type`

### Requirement: Default on first access
`get-data` SHALL return the stored value or create it from `default-value`, or from the default constructor that the mod registered for the type, and store it. Status: planned (task 2.14).

#### Scenario: First read
- **WHEN** a mod reads a string attachment with default `""` from a new player
- **THEN** it gets `""` and `has-data` returns true afterwards

### Requirement: Value type is fixed by the default
`set-data` SHALL fail with `wrong-value-type` when the value is another `attachment-value` case than the default value of the type. Status: planned (task 2.14).

#### Scenario: Wrong case
- **WHEN** a mod sets an int value on a string attachment
- **THEN** the call fails with `attachment-error::wrong-value-type`

### Requirement: Saving
A type with `serialize` SHALL be saved with its holder under the `neoforge:attachments` NBT key and restored on load; a type without it SHALL not survive a save. Status: planned (task 2.14).

#### Scenario: Round trip
- **WHEN** a player with a serialized attachment logs out and back in
- **THEN** `get-existing-data` returns the same value

### Requirement: Copy on death
A type with `copy-on-death` SHALL keep its value on the respawned player, through the copy handler of the type when it has one. Status: planned (task 2.14).

#### Scenario: Respawn
- **WHEN** a player with a copy-on-death attachment dies and respawns
- **THEN** the new player has the value

### Requirement: Sync to clients
A type with `attachment-sync` other than `none` SHALL be sent to the matching players with `neoforge:sync_attachments` when it changes and on `sync-data`; `sync-data` on a type without sync SHALL fail with `not-synced`. Status: planned (task 2.14).

#### Scenario: Tracking player
- **WHEN** a synced value of an entity changes
- **THEN** every NeoForge client tracking that entity receives the new value

### Requirement: Backing phase
Until M5 (Modded gameplay parity) adds attachment storage, every call SHALL fail with `not-available`. Status: planned (task 2.14).

#### Scenario: Before M5 (Modded gameplay parity)
- **WHEN** a mod calls `get-data` on a host without attachment storage
- **THEN** the call fails with `attachment-error::not-available`
