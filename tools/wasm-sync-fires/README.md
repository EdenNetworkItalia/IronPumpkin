# wasm-sync-fires

`sync_fires.py` lists the synchronous (`func`) imports of the v0.2 Wasm host whose host code
reaches `PluginManager::fire_blocking` or `fire_blocking_with`, or a read of an item stack that
fires an event through the item hooks (`get_enchantment_level`, `get_all_enchantments`,
`get_attribute_modifiers`, `has_prevent_armor_change`). Such an import runs while the guest
call holds the root permit of the Wasm plugin stores. If a Wasm plugin handles the event, the
handler waits for that permit and the call hangs. An import that returns nothing defers the work
with `DeferHost::defer` (`crates/pumpkin-wasm-host-v0_2/src/lib.rs`). The deferred queue
(`crates/pumpkin-wasm-host-common/src/deferred.rs`) runs the work after the guest call, in call
order. The scan does not follow the code inside a `defer` call. An import that returns a value
from an event runs its work in `guest_call::native_handlers_only`
(`crates/pumpkin-core/src/plugin/guest_call.rs`), so that its fires skip the Wasm handlers. The
Java and Bedrock send paths defer the send of a packet, a raw packet and a kick inside a guest
call when a Wasm plugin handles the sent event of the send, so the scan treats these send
functions (the `SENDS` set of the script) as safe. The scan cannot see the native-only runtime
check, so `sync_fires.tsv` records those imports.

The scan also lists the async imports (`impl Host*WithStore<PluginHostState>`) that reach a
blocking fire outside `run_blocking` and `in_active_context`. wasmtime polls the future of an
async import outside the re-entry context of the guest call, so such a fire hangs too. The scan
blanks the arguments of `run_blocking` and `in_active_context` and every `async` block, so an
async block that does not run through them is a miss. These rows start with `async `.

`sync_fires.tsv` records each import that the scan finds and its disposition. The check fails when
the scan finds an import that the list does not have, or when the list has an import that the scan
no longer finds.

## Command

```sh
python3 -I -B tools/wasm-sync-fires/sync_fires.py            # one line for each chain of an import
python3 -I -B tools/wasm-sync-fires/sync_fires.py --check    # exit 1 when the scan and the list differ
python3 -I -B tools/wasm-sync-fires/test_sync_fires.py       # unit tests
```

Run the check after a change to a synchronous v0.2 import or to a fire site of the server. For a
new import in the output, defer its work, or add a row to `sync_fires.tsv` with the reason. The
script uses the standard library only.

## What the scan finds and misses

The scan builds a call graph from function names, without types:

- It reads the functions of `pumpkin-core`, `pumpkin-inventory`, `pumpkin-world`,
  `pumpkin-wasm-host-common` and `pumpkin-wasm-host-v0_2`. A synchronous function cannot call an
  async function without `block_on`, so the scan ignores async functions.
- The code inside `defer`, `spawn`, `spawn_blocking` and `async` blocks runs after the import
  returns. The scan ignores it.
- A name with more than four definitions is too ambiguous to follow, and the scan ignores it.
  Examples are `set_velocity` (a player and three projectiles) and `remove_stack` (each
  inventory). The scan also ignores the names of the standard library and of common traits
  (`get`, `clone`, `tick`, `update`). Thus an import that reaches a fire only through such a name
  is not found: `player.set-velocity` and the two statistic increments are examples.
- A closure that the import stores and that the server calls later looks like a call. The
  `false positive` rows of the list record these.
- A call through a trait object (`dyn BlockBehaviour`, `dyn Inventory`) joins every function of
  that name, if the name has four definitions or fewer.

Thus the scan does not prove that an import cannot hang. It catches a new direct path to a fire.
