# Spec Delta

## Purpose

Defines how IronPumpkin reads and writes the NeoForge and `c:` custom payloads of the configuration phase, matching the NeoForge 26.3.x StreamCodecs byte for byte.

## ADDED Requirements

### Requirement: Payload codecs match NeoForge
The server SHALL read and write `neoforge:register`, `neoforge:network`, `neoforge:modded_network_setup_failed`, `c:version`, `c:register`, `neoforge:frozen_registry_sync_start`, `neoforge:frozen_registry`, `neoforge:frozen_registry_sync_completed`, `neoforge:feature_flags`, `neoforge:config_file` and `neoforge:split` with the field order and encodings of the NeoForge 26.3.x StreamCodecs. Status: implemented (#8).

#### Scenario: Decode bytes from the Java codec
- **WHEN** a payload is decoded from bytes assembled from the Java StreamCodec definition
- **THEN** every field has the expected value and writing the value back produces the same bytes

#### Scenario: Decode bytes from a NeoForge server
- **WHEN** the payloads recorded from a NeoForge 26.3.0.52-beta dedicated server are decoded
- **THEN** every payload that has a codec decodes without error and without leftover bytes

### Requirement: Bounded lengths
Every count and byte-array length read from the network SHALL be rejected when it is negative or larger than the remaining input, before any allocation of that size. Status: implemented (#8).

#### Scenario: Hostile length
- **WHEN** a payload declares a byte array or collection longer than the bytes that follow
- **THEN** decoding fails with an error and does not allocate the declared size

### Requirement: No trailing bytes
A configuration payload SHALL be rejected when bytes remain after its last field, as the vanilla packet decoder does. Status: implemented (#8).

#### Scenario: Extra byte
- **WHEN** a `neoforge:register` payload carries one byte after its last field
- **THEN** decoding fails with a "bytes left over" error
