//! The cost of the enchantment level and attribute modifier reads of an item stack once a hook is
//! set (issue #180): with no live handler, a read allocates nothing.

mod support;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

use pumpkin_core::plugin::{
    BoxFuture, EventHandler, EventPriority, PluginManager, enchantment::GetEnchantmentLevelEvent,
    inventory::ItemAttributeModifierEvent,
};
use pumpkin_core::server::Server;
use pumpkin_data::{Enchantment, item::Item, item_stack::ItemStack};

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// Counts the allocations of the thread that calls it.
struct Counting;

// SAFETY: every call goes to the system allocator with the same arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller keeps the contract of `GlobalAlloc::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller keeps the contract of `GlobalAlloc::dealloc`.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations<R>(read: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCATIONS.with(Cell::get);
    let result = read();
    (result, ALLOCATIONS.with(Cell::get) - before)
}

struct Nothing;

impl<E: pumpkin_core::plugin::Payload + Send + Sync + 'static> EventHandler<E> for Nothing {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

/// A handler on a manager that no server owns sets the hooks of the process. The server of the
/// support module has no handler of the two events, so the reads find no live handler.
#[test]
fn a_read_with_a_hook_set_and_no_live_handler_allocates_nothing() {
    let _serial = support::serial();
    let _server = support::server();
    let elsewhere = PluginManager::new(Vec::new());
    elsewhere.register_with::<GetEnchantmentLevelEvent, _>(
        Arc::new(Nothing),
        EventPriority::Normal,
        true,
        false,
    );
    elsewhere.register_with::<ItemAttributeModifierEvent, _>(
        Arc::new(Nothing),
        EventPriority::Normal,
        true,
        false,
    );
    let mut sword = ItemStack::new(1, &Item::GOLDEN_SWORD);
    sword.add_enchantment(&Enchantment::SHARPNESS, 3);

    let (level, level_allocations) =
        allocations(|| sword.get_enchantment_level(&Enchantment::SHARPNESS));
    let (all, all_allocations) = allocations(|| {
        sword
            .get_all_enchantments()
            .map(|all| all.enchantment.len())
    });
    let (modifiers, modifier_allocations) = allocations(|| {
        sword
            .get_attribute_modifiers()
            .map(|m| m.attribute_modifiers.len())
    });

    assert_eq!(level, 3);
    assert_eq!(all, Some(1));
    assert!(modifiers.is_some_and(|count| count > 0));
    assert_eq!(
        (level_allocations, all_allocations, modifier_allocations),
        (0, 0, 0)
    );
}
