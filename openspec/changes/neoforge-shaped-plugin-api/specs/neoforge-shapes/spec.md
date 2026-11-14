# Spec Delta

## Purpose

Lets a ported mod read block shapes and collisions of the level, and declare the shapes of its own blocks, with the behaviour of `VoxelShape`, `CollisionContext` and the shape getters of `BlockState`, as the `shapes` module of `ironpumpkin-mods`.

## ADDED Requirements

### Requirement: Read block shapes
`ironpumpkin_mods::shapes::get_block_shape` SHALL return the outline, collision, interaction, block support, occlusion or visual shape of the block at a position, with the collision context of the given entity or the empty context, and `get_state_shape` SHALL return the shape of a state on its own, without the position offset. Both SHALL answer for vanilla and mod blocks. Status: supported (task 2.10, #155).

#### Scenario: A vanilla slab
- **WHEN** a mod asks the collision shape of a bottom stone slab
- **THEN** the result is one box from 0 to 1 on x and z and from 0 to 0.5 on y

### Requirement: Collision tests
`shapes::is_face_sturdy` SHALL answer as `BlockState#isFaceSturdy` does for the support type, and `shapes::no_collision` SHALL answer as `CollisionGetter#noCollision` does for the box and the entity. Status: supported (task 2.10, #155).

#### Scenario: Free space for a spawn
- **WHEN** a mod tests `no_collision` for a box of one block inside a stone wall
- **THEN** the answer is false

### Requirement: Declared shapes of mod blocks
A mod block SHALL use the shapes that `BlockBuilder::shape` and `collision_shape` declare, per state, in Pumpkin collision, ray traces and the shape getters, and SHALL be a full cube when it declares none. Status: supported (task 2.10, #155).

#### Scenario: A half-height mod block
- **WHEN** a mod registers a block whose declared shape is one box from 0 to 0.5 in height and a player walks onto it
- **THEN** the player stands half a block above the block position, and `get_state_shape` of its state returns that box
