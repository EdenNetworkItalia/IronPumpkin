//! A limit on nested item stacks. A bundle, a container or another template component holds
//! stacks that hold more stacks, and each decoder of a stack recurses into its components. A
//! crafted value with many levels would otherwise exhaust the thread stack.

use std::cell::Cell;
use std::marker::PhantomData;

/// The maximum number of item stacks inside each other in one decoded value. Vanilla
/// survival play nests about 16 bundles at most, because of the bundle weight. One level of the
/// network decoder takes less than 8 KiB of stack in a release build and about 25 KiB in a
/// debug build, so 32 levels fit a 1 MiB stack in both.
pub const MAX_ITEM_NESTING: u32 = 32;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Whether a level failed to enter since the outermost level of the current value began.
    static DROPPED: Cell<bool> = const { Cell::new(false) };
}

/// One level of item stack nesting. The level ends when the guard drops.
///
/// The decoders are synchronous, so a thread-local counter sees every level of one value. The
/// guard is not `Send`, so it cannot live across an `await`.
pub struct NestingGuard(PhantomData<*const ()>);

impl NestingGuard {
    /// Enters one level, or returns `None` when the value is nested deeper than
    /// [`MAX_ITEM_NESTING`].
    #[must_use]
    pub fn enter() -> Option<Self> {
        DEPTH.with(|depth| {
            let current = depth.get();
            if current == 0 {
                DROPPED.set(false);
            }
            if current >= MAX_ITEM_NESTING {
                DROPPED.set(true);
                return None;
            }
            depth.set(current + 1);
            Some(Self(PhantomData))
        })
    }

    /// Whether this level is the outermost level of the value being decoded.
    #[must_use]
    pub fn is_outermost(&self) -> bool {
        DEPTH.with(Cell::get) == 1
    }
}

/// Whether a level failed to enter since the outermost level of the current value began. The
/// call clears the flag.
#[must_use]
pub fn take_dropped_levels() -> bool {
    DROPPED.replace(false)
}

impl Drop for NestingGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_counts_levels_and_releases_them() {
        let guards: Vec<_> = (0..MAX_ITEM_NESTING)
            .map(|_| NestingGuard::enter().expect("level within the limit"))
            .collect();
        assert!(NestingGuard::enter().is_none());
        drop(guards);
        assert!(NestingGuard::enter().is_some());
    }

    /// A saved bundle that holds `levels` bundles inside each other.
    fn nested_bundle_nbt(levels: u32) -> pumpkin_nbt::compound::NbtCompound {
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_nbt::tag::NbtTag;
        let mut stack = NbtCompound::new();
        stack.put_string("id", "minecraft:bundle".to_owned());
        for _ in 0..levels {
            let mut components = NbtCompound::new();
            components.put_list("minecraft:bundle_contents", vec![NbtTag::Compound(stack)]);
            let mut outer = NbtCompound::new();
            outer.put_string("id", "minecraft:bundle".to_owned());
            outer.put_compound("components", components);
            stack = outer;
        }
        stack
    }

    fn bundle_depth(stack: &crate::item_stack::ItemStack) -> u32 {
        use crate::data_component_impl::BundleContentsImpl;
        1 + stack
            .get_data_component::<BundleContentsImpl>()
            .and_then(|bundle| bundle.items.first())
            .map_or(0, bundle_depth)
    }

    /// Counts the warning events while it is the default subscriber.
    struct Warnings(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl tracing::Subscriber for Warnings {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            if *event.metadata().level() == tracing::Level::WARN {
                self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn warnings_while(f: impl FnOnce()) -> usize {
        let count = std::sync::Arc::default();
        tracing::subscriber::with_default(Warnings(std::sync::Arc::clone(&count)), f);
        count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// One saved value that loses levels past the limit logs one warning, however many of its
    /// stacks lose levels.
    #[test]
    fn a_saved_stack_past_the_nesting_limit_logs_one_warning() {
        use crate::item_stack::ItemStack;
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_nbt::tag::NbtTag;
        let within = nested_bundle_nbt(MAX_ITEM_NESTING - 1);
        assert_eq!(
            warnings_while(|| drop(ItemStack::read_item_stack(&within))),
            0
        );
        let too_deep = NbtTag::Compound(nested_bundle_nbt(MAX_ITEM_NESTING + 8));
        let mut components = NbtCompound::new();
        components.put_list("minecraft:bundle_contents", vec![too_deep; 3]);
        let mut bundle = NbtCompound::new();
        bundle.put_string("id", "minecraft:bundle".to_owned());
        bundle.put_compound("components", components);
        assert_eq!(
            warnings_while(|| drop(ItemStack::read_item_stack(&bundle))),
            1
        );
        assert_eq!(
            warnings_while(|| drop(ItemStack::read_item_stack(&within))),
            0
        );
    }

    /// A bundle drops an entry that does not load, so the levels past the limit are lost.
    #[test]
    fn saved_stacks_load_at_most_the_nesting_limit() {
        use crate::item_stack::ItemStack;
        let within = ItemStack::read_item_stack(&nested_bundle_nbt(MAX_ITEM_NESTING - 1));
        assert_eq!(within.as_ref().map(bundle_depth), Some(MAX_ITEM_NESTING));
        let too_deep = ItemStack::read_item_stack(&nested_bundle_nbt(MAX_ITEM_NESTING + 8));
        assert_eq!(too_deep.as_ref().map(bundle_depth), Some(MAX_ITEM_NESTING));
    }
}
