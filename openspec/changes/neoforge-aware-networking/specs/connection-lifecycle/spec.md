# Spec Delta

## Purpose

Defines login and configuration behaviour that every Java client relies on, where Pumpkin differed from vanilla.

## ADDED Requirements

### Requirement: Client information carries the particle status
Both client_information packets (configuration and play) SHALL read and write the trailing particleStatus VarInt from protocol 1.21.2, and an out-of-range value SHALL wrap like vanilla instead of failing the packet. Status: implemented (#15).

#### Scenario: Vanilla bytes
- **WHEN** the server decodes `05656e5f75730c01005a00010002`, written by the vanilla 26.3 `ClientInformation.write()`
- **THEN** the particle status is MINIMAL and no byte is left over

#### Scenario: Out-of-range value
- **WHEN** a client sends particle status 4
- **THEN** the server stores DECREASED (4 modulo 3) and the packet does not fail

### Requirement: No packet handled after a kick
After the server kicks a client in the login or configuration phase, it SHALL NOT handle any further packet from that client, including packets already buffered. Status: implemented (#14).

#### Scenario: Buffered pong after a malformed payload
- **WHEN** a client sends a malformed `neoforge:register` and a pong 0 in the same TCP write
- **THEN** the client is kicked and the configuration sequence does not start

### Requirement: Brand decoded as a string
The server SHALL decode the `minecraft:brand` payload as a VarInt length-prefixed UTF-8 string, as vanilla `readUtf` does. Status: implemented (#28).

#### Scenario: Vanilla brand
- **WHEN** a client sends the brand payload `07 76 61 6e 69 6c 6c 61`
- **THEN** the stored brand is `vanilla` with no length prefix

#### Scenario: Malformed brand
- **WHEN** a client sends a brand payload with bytes after the string, or a string of more than 32767 characters
- **THEN** the server kicks the client like any other malformed packet
