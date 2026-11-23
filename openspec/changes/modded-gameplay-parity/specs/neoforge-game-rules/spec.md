# Spec Delta

## Purpose

Lets a ported mod register game rules next to the vanilla rules, with a typed read, the `/gamerule` command, a save, and the client values.

## ADDED Requirements

### Requirement: Register a mod game rule
A mod SHALL register a boolean or an integer game rule with an id, a category, a default value and, for an integer, a range. The registration SHALL return a typed handle. A mod SHALL read and set its rule through the handle. Status: planned (wave 2).

#### Scenario: A boolean rule
- **WHEN** a mod registers `examplemod:machines_explode` with the default false
- **THEN** a read through the handle returns false

#### Scenario: An integer range
- **WHEN** a mod registers an integer rule with the range 1 to 10 and sets 20 through the handle
- **THEN** the call fails with a range error and the value stays

#### Scenario: Registration after the freeze
- **WHEN** a mod registers a game rule after the freeze
- **THEN** the call fails with the frozen-registry error

### Requirement: The gamerule command
The `/gamerule` command SHALL list, read and set a mod rule with its qualified id. It SHALL accept the short form when no other rule has the same short name. It SHALL validate a value as for a vanilla rule. Status: planned (wave 2).

#### Scenario: Set a mod rule
- **WHEN** an operator runs `/gamerule examplemod:machines_explode true`
- **THEN** the handle of the mod returns true

#### Scenario: Query a mod rule
- **WHEN** an operator runs `/gamerule examplemod:machines_explode`
- **THEN** the server reports the current value

### Requirement: Save and load
The server SHALL save mod rules under the key `modid:rule` with the vanilla rules. On load, a key with no registered rule SHALL load. The next save SHALL write that key back unchanged. Status: planned (wave 2).

#### Scenario: Persist a value
- **WHEN** a rule has the value true, the server stops, and the server starts again
- **THEN** the rule has the value true

#### Scenario: A removed mod
- **WHEN** a world saved with `examplemod:machines_explode` loads in a modpack without `examplemod`
- **THEN** the world loads and the next save holds the key and its value

### Requirement: Client values
The game rule values packet SHALL include the mod rules for a `Real` connection. It SHALL leave them out for a `Display` connection. Status: planned (wave 2).

#### Scenario: A Real connection
- **WHEN** a `Real` connection asks for the game rule values
- **THEN** the answer holds `examplemod:machines_explode` with its value

#### Scenario: A Display connection
- **WHEN** a `Display` connection asks for the game rule values
- **THEN** the answer holds the vanilla rules only

### Requirement: Change event
A change of a mod rule SHALL fire the native `GameRuleChangedEvent` with the mod rule id and the new value. The v0.2 host SHALL NOT send a mod rule change to a Wasm plugin. Status: planned (wave 2).

#### Scenario: A native handler
- **WHEN** an operator changes a mod rule
- **THEN** a native handler of `GameRuleChangedEvent` receives the mod rule id and the new value

#### Scenario: A Wasm plugin
- **WHEN** an operator changes a mod rule
- **THEN** a v0.2 plugin receives no event

### Requirement: Scope rule
A mod that registers a rule through `GameRules.register` SHALL count as demand for the game rule row. Status: planned (wave 1).

#### Scenario: A helper registration
- **WHEN** a mod of the reference modpack calls `GameRules.register`
- **THEN** the game rule row stays `planned`

### Requirement: Vanilla rules keep their interface
The vanilla rule set, its generated type, the Wasm interface for vanilla rules and the read cost of a vanilla rule SHALL stay the same. Status: planned (wave 2).

#### Scenario: A v0.2 plugin
- **WHEN** a v0.2 plugin reads `mob_griefing` in a modpack with mod rules
- **THEN** it receives the vanilla value
