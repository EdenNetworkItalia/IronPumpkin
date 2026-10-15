# Porting mixins, reflection and access transformers from NeoForge

In NeoForge, a mixin changes the bytecode of a vanilla class when the game loads it. An access transformer opens a private member. Reflection reaches a member at run time. A Rust server has no bytecode to change and no run-time reflection.

IronPumpkin replaces all three with a source patch. A source patch is a unified diff that the mod crate ships. The pack build applies it to the IronPumpkin source at the pinned commit and compiles the result. Use a patch only when no NeoForge event and no `ironpumpkin-mods` function does the same job. Every accepted patch is a candidate for a new event in a later version, so the API grows from real patches.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Change a method** | `@Inject`, `@Redirect`, `@ModifyVariable` or `@Overwrite` in a mixin | A handler of the NeoForge event, if the inventory has one for the point. Else a source patch. |
| **Open a private member** | Access transformer, `@Accessor`, `@Invoker`, reflection | A source patch that makes the member `pub` or adds an accessor |
| **State added to a vanilla class** | A `@Unique` field | An attachment of the entity, block entity, chunk or level. Planned (task 2.14). |
| **Condition on a mixin** | A mixin plugin | A Cargo feature or the mod config |
| **Declaration** | `[[mixins]]` and `[[accessTransformers]]` in `neoforge.mods.toml` | `patches/<name>.patch` and `patches/<name>.md` in the mod crate |
| **When it applies** | When the game loads the class | At build time, before the compile |
| **Version binding** | Mappings and method descriptors | One IronPumpkin commit, named in the `Cargo.toml` of the mod |
| **Conflict** | Mixin priority and merge | The build fails |
| **Client mixins** | A `client` list in the mixin config | None. A dedicated server has no client code. |
| **Mixin into another mod** | A mixin that targets a class of another mod | An event or an API of the target mod, or a change that both authors agree on |

---

## Which tool to use

Go down the list. Stop at the first line that fits.

1. A NeoForge event of the inventory fires at the point that the mixin changes. Handle the event. The event pages of this guide are planned (tasks 2.3, 2.12, 2.13 and 2.15). `mapping-table.md` lists the Pumpkin event for each NeoForge event.
2. An `ironpumpkin-mods` function, or a Pumpkin function that the crate re-exports, does the work. Call it.
3. The mixin only adds state to a vanilla class. Use an attachment. Planned (task 2.14).
4. None of the above. Write a source patch.

A patch cannot do four things. It cannot change a client class, because the server has none. It cannot apply a mixin conditionally, because there is no mixin plugin. It cannot change a generated file. It cannot reach into another mod.

---

## Code Comparison: opening a private member

### 1. NeoForge implementation (Java)

The code below is an illustration. It opens a private field of a vanilla class in two ways, with an access transformer and with an accessor mixin.

File `META-INF/accesstransformer.cfg`:

```text
public net.minecraft.world.level.chunk.ChunkAccess structureStarts
```

```java
@Mixin(ChunkAccess.class)
public interface ChunkAccessAccessor {
    @Accessor("structureStarts")
    Map<Structure, StructureStart> mymod$structureStarts();
}
```

### 2. IronPumpkin implementation (patch)

A mod author wrote this patch for the WITS port, which needs the structure starts of a chunk. The patch adds a read accessor to a private field of Pumpkin. It changes no behaviour of the server.

File `patches/structure-starts-accessor.patch`:

```diff
diff --git a/crates/pumpkin-world/src/generation/structure/placement.rs b/crates/pumpkin-world/src/generation/structure/placement.rs
index aa8d0b5..22dad45 100644
--- a/crates/pumpkin-world/src/generation/structure/placement.rs
+++ b/crates/pumpkin-world/src/generation/structure/placement.rs
@@ -86,6 +86,22 @@ impl GlobalStructureCache {
         computed
     }
 
+    /// Every structure start computed so far, with its structure.
+    #[must_use]
+    pub fn structure_starts(&self) -> Vec<(StructureKeys, StructurePosition)> {
+        self.structure_starts.get().map_or_else(Vec::new, |cache| {
+            cache
+                .iter()
+                .filter_map(|entry| {
+                    entry
+                        .value()
+                        .as_ref()
+                        .map(|start| (entry.key().0, start.clone()))
+                })
+                .collect()
+        })
+    }
+
     /// Calculates the 128 ring positions matching vanilla Minecraft's
     /// `ChunkGeneratorStructureState.generateRingPositions` exactly.
     #[must_use]
```

The patch needs a justification file next to it, and the commit in the `Cargo.toml` of the mod:

File `patches/structure-starts-accessor.md`:

```markdown
# structure-starts-accessor.patch

What it does: adds `GlobalStructureCache::structure_starts()` to
`crates/pumpkin-world/src/generation/structure/placement.rs`. The method returns a copy of every
structure start that the world generator has computed so far, with its structure key.

Why a NeoForge event or API is not enough: WITS reads the structure starts of a chunk with
vanilla `StructureManager.startsForStructure`, a plain vanilla call with no event around it.
IronPumpkin has no structure manager. It keeps the structure starts only in the private
`structure_starts` field of `GlobalStructureCache`. The patch is a read accessor on that field:
the compile-time equivalent of an access transformer. It changes no behaviour of the server.

Commit binding: the patch is written for IronPumpkin commit
`b83ce68e593d7e8e13ee7dbf3ccdf29271d493d3`, as `[package.metadata.ironpumpkin] commit` in
`Cargo.toml` declares. Regenerate it for any other commit.

Candidate API: a structure manager on the world (`startsForStructure`, `getStructureAt`) in the
NeoForge-shaped API would remove this patch.
```

File `Cargo.toml`:

```toml
[package.metadata.ironpumpkin]
commit = "b83ce68e593d7e8e13ee7dbf3ccdf29271d493d3"
```

The files are in the `ironpumpkin-pack-test` repository, in commit `958190c64`, under `mods/wits/patches/`.

### 3. When an API replaces the patch

The "Candidate API" line of the justification file came true. IronPumpkin now has `World::structure_starts_at`, so the current WITS port has no patch and no `[package.metadata.ironpumpkin]` table. This is the intended life of a patch: it proves a need, and the API absorbs it.

```rust
use ironpumpkin_mods::{math::position::BlockPos, world::World};

/// The names of the structures whose start box contains `pos`.
/// The lookup can load chunks, so call it from a server task.
async fn structure_names_at(world: &World, pos: BlockPos) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = world
        .structure_starts_at(&pos)
        .await
        .into_iter()
        .map(|start| start.structure.to_name())
        .collect();
    names.sort_unstable();
    names
}
```

---

## The rules of a patch

**Files**

```text
mods/my-mod/
  Cargo.toml                       has [package.metadata.ironpumpkin] commit
  src/lib.rs
  patches/
    fall-damage.patch              made with git diff against the root of the IronPumpkin tree
    fall-damage.md                 the justification
```

**Binding to one commit**

- The mod names the IronPumpkin commit in `[package.metadata.ironpumpkin] commit`. The value is a full 40-character hash.
- `modpack.toml` pins the commit of the pack. The build fails when the two commits differ.
- `allow-drift = true` under `[ironpumpkin]` in `modpack.toml` lets the patch apply anyway, with a warning. A conflict still fails the build.
- After an IronPumpkin update, the mod author writes the patch again against the new commit.

**Application**

- The build checks out IronPumpkin at the pinned commit, with no local change.
- It applies the patches of the mods in the byte order of the keys of `modpack.toml`. The keys are the package names of the mod crates. Inside a mod, it applies them in file name order.
- It uses `git apply`. There is no fuzz, no whitespace leniency and no three-way merge.
- The `init` order of the mods is separate. It follows `NativeMod::id`.

**Conflicts**

Two mods that patch the same lines fail the build. The message names both mods and the file. The pack maintainer has three ways to resolve it:

- Order the mods. The later mod writes its patch against the tree with the patch of the earlier mod applied.
- Share one patch that both mods use.
- Propose an event that replaces both patches.

**What a patch may not touch**

- A file under `crates/pumpkin-data/src/generated`. `pumpkin-codegen` writes these files. A change to generated data goes into IronPumpkin itself.
- A `Cargo.lock`, in any letter case.

**Errors of the build**

Each case below fails the build with a message. The message names the mod and the patch.

| Case | The message also names |
| :--- | :--- |
| A patch written for another commit, and no `allow-drift` | The commit of the patch and the commit of the pack |
| A patch that does not apply | The file. When an earlier mod changed the same file, that mod too. |
| A patch that touches a generated file or a `Cargo.lock` | The file |
| A patch with no justification file | The missing `patches/<name>.md` |
| A mod with patches and no `[package.metadata.ironpumpkin] commit` | The missing key |

Two real messages of `cargo xtask check`:

```text
error: mod `greeter`: `patches/demo.patch` is written for IronPumpkin commit 5cb0ebbe773969d056316c705966f10f3d9e974e, the pack pins commit 68184bf421eed373f6f7dbfe25bcbfdbc6cc29a8; update the mod or set `allow-drift = true` in modpack.toml
error: mod `greeter`: `patches/demo.patch` has no justification file `patches/demo.md` next to it
```

**Licence**

A patch is a derivative work of the GPL-3.0 server. Distribute it under terms that are compatible with GPL-3.0. A distributed pack binary comes with the source of every linked mod and every applied patch.

---

## Do and don't

**Do**

- Look for a NeoForge event or an `ironpumpkin-mods` function first.
- Keep a patch small. A read accessor or a new `pub` is easier to port to the next commit than a change of logic.
- Write the justification file when you write the patch. The build fails without it.
- Say in the justification file which event or API would remove the patch.
- Name the commit with the full 40-character hash.
- Make the diff with `git diff` in the IronPumpkin tree. Paths start at the repository root.
- Keep the patch file with LF line endings. The `.gitattributes` of the template does this.
- Write the key names of `modpack.toml` with care. `hello-mod` sorts before `hello_mod`, because `-` comes before `_` in byte order, and upper case comes before lower case.

**Don't**

- Do not patch a generated file or a `Cargo.lock`.
- Do not use a patch for a client class. The server never runs it.
- Do not use a patch where an event fires at the point of the mixin.
- Do not expect a patch to survive an IronPumpkin update. A patch is bound to one commit.
- Do not make two mods patch the same lines without a plan. The build fails.
- Do not add a git dependency on a server crate to call a patched function. Use the crates that `ironpumpkin-mods` re-exports, for example `ironpumpkin_mods::pumpkin_world`. The build fails when a mod depends by git URL on a server crate that the pack binary does not link.
