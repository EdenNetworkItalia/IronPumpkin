# Porting block shapes from NeoForge

In NeoForge, a block gives its shapes through the getters of `BlockBehaviour`: `getShape` for the outline, `getCollisionShape`, `getInteractionShape`, `getBlockSupportShape`, `getOcclusionShape` and `getVisualShape`. Each getter takes the state, the level, the position and, for the outline and the collision shape, a `CollisionContext`. A mod builds the `VoxelShape` with `Block.box`, `Block.column` and `Shapes.join`, and reads other blocks with `state.getShape(level, pos)`, `state.isFaceSturdy(level, pos, face, type)` and `level.noCollision(entity, box)`.

In IronPumpkin, a mod block declares its outline and its collision shape per state on the `BlockBuilder`. The server evaluates them once per state when the registry freezes, and uses them in entity collision, ray traces and support checks. The `shapes` module of `ironpumpkin-mods` reads the shapes and tests collisions.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Shape type** | `VoxelShape`, a discrete voxel grid | `shapes::VoxelShape`, a union of boxes in block-local coordinates (`0..1` for one block). The boolean operations give the same set of points. |
| **Build** | `Shapes.empty()`, `Shapes.block()`, `Shapes.box`, `Block.box`, `Block.column` | `VoxelShape::empty`, `block`, `cuboid`, `pixels`, `column` |
| **Combine** | `Shapes.or`, `Shapes.join(a, b, BooleanOp.ONLY_FIRST)`, `move`, `Shapes.rotateHorizontal` | `or`, `join(&b, BooleanOp::OnlyFirst)`, `move_by`, `rotate_horizontal` |
| **Outline** | override `getShape(state, level, pos, context)` | `BlockBuilder::shape(\|state\| ..)` |
| **Collision shape** | override `getCollisionShape`, or `noCollission()` on the properties | `BlockBuilder::collision_shape(\|state\| ..)`. Return `VoxelShape::empty()` for no collision. |
| **No shape declared** | `new Block(properties)` is a full cube; a subclass such as `StairBlock` has its shapes | A full cube; a block with `.block_type(..)` has the shapes of its type |
| **Read a shape** | `state.getShape(level, pos, context)`, `getCollisionShape`, `getInteractionShape`, `getBlockSupportShape`, `getOcclusionShape` | `shapes::get_block_shape(&world, &pos, kind, entity)` and `shapes::get_state_shape(state, kind)`, with `ShapeKind::Outline`, `Collision`, `Interaction`, `BlockSupport`, `Occlusion`, `Visual` |
| **Support test** | `state.isFaceSturdy(level, pos, face, SupportType.CENTER)` | `shapes::is_face_sturdy(&world, &pos, face, SupportType::Center)` |
| **Free space** | `level.noCollision(entity, box)` | `shapes::no_collision(&world, entity, box)` |
| **Shape per position or entity** | `dynamicShape()`, a `CollisionContext` check in the getter | Not supported. Shapes depend on the state only. |

---

## Code Comparison: a block with a shape per state

### 1. NeoForge implementation (Java)

```java
public class TrayBlock extends Block {
    public static final EnumProperty<Direction> FACING = HorizontalDirectionalBlock.FACING;
    public static final BooleanProperty OPEN = BooleanProperty.create("open");
    private static final Map<Direction, VoxelShape> SHAPES = Shapes.rotateHorizontal(
            Shapes.or(Block.box(0, 0, 0, 16, 8, 16), Block.box(0, 8, 0, 16, 16, 8)));

    @Override
    protected VoxelShape getShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return SHAPES.get(state.getValue(FACING));
    }

    @Override
    protected VoxelShape getCollisionShape(BlockState state, BlockGetter level, BlockPos pos, CollisionContext context) {
        return state.getValue(OPEN) ? Shapes.empty() : SHAPES.get(state.getValue(FACING));
    }
}
```

### 2. IronPumpkin implementation (Rust)

```rust
use ironpumpkin_mods::{
    ModInit,
    content::{BlockBuilder, RegistryError},
    pumpkin_data::{Block, BlockDirection},
    shapes::VoxelShape,
};

fn tray_shape(facing: BlockDirection) -> VoxelShape {
    VoxelShape::pixels(0.0, 0.0, 0.0, 16.0, 8.0, 16.0)
        .or(VoxelShape::pixels(0.0, 8.0, 0.0, 16.0, 16.0, 8.0))
        .rotate_horizontal(facing)
}

fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(
        BlockBuilder::new("hello-mod:tray", Block::OAK_SLAB.default_state.id)
            .enum_property("facing", &["north", "east", "south", "west"], "north")
            .bool_property("open", false)
            .shape(|state| tray_shape(state.get_direction("facing").unwrap_or(BlockDirection::North)))
            .collision_shape(|state| {
                if state.get_bool("open") == Some(true) {
                    return VoxelShape::empty();
                }
                tray_shape(state.get_direction("facing").unwrap_or(BlockDirection::North))
            }),
    )
}
```

The shape function gets a `StateProperties`: `get`, `get_bool`, `get_int` and `get_direction` read the values of the state by property name. The function runs once for each state, at the freeze, so it must not read the world.

---

## What the declared shapes change

A custom state copies the physical data of its display state or of its block type, except the shapes. With declared shapes, the server also recomputes the flags that vanilla derives from the shapes:

- the six full faces and the center of the top and bottom faces, which `is_side_solid`, `is_center_solid` and the placement of torches, rails and other blocks read;
- the full cube flag and the redstone conductor flag, which follow `Block.isShapeFullBlock` of the collision shape;
- the legacy solid flag (`BlockStateBase.calculateSolid`);
- the solid render flag, from the outline when the display state can occlude.

The light, the hardness, the occlusion of the display state and the sounds stay as the display state or the type gives them.

## The shape getters of vanilla blocks

Pumpkin extracts the outline and the collision shape of every vanilla state. The other getters give the vanilla defaults of `BlockBehaviour`:

| Getter | Default | Vanilla blocks that override it and answer with the default |
| :--- | :--- | :--- |
| `getInteractionShape` | empty | the cauldrons, the composter, the hopper, scaffolding |
| `getBlockSupportShape` | the collision shape | the leaves, the snow layer, the fence gates, the shulker boxes, soul sand, mud, the hanging signs, the chorus flower |
| `getOcclusionShape` | the outline when the state can occlude, else empty | the fences, the fence gates, the lectern, the sculk shrieker |
| `getVisualShape` | the collision shape | the fences, the iron bars and glass panes, mud, powder snow, the snow layer, soul sand, glass and the other transparent blocks |

`is_face_sturdy` reads the extracted flags for a full face and for the center of the top and bottom faces, and computes the center of a side face and the rigid support from the block support shape.

## Client side

A NeoForge client runs the Java half of the mod and computes the shapes itself. Declare the same shapes in the Rust port, else the server and the client disagree about where a player stands.

## Do and don't

- Do declare the shapes of every block that is not a full cube. A block without declared shapes and without a type is a full cube, whatever its display state.
- Do give `collision_shape` only when it differs from the outline: by default the collision shape is the outline.
- Do not read the world or a global state in a shape function: it runs before the first world loads.
- Do not port a shape that changes with the position or the entity, such as scaffolding. Use the shape of the most common case, or a source patch (see [Mixins, reflection and access transformers](./mixins-and-patches.md)).
