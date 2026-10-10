package it.edennetworkitalia.hellomod;

import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.animal.pig.Pig;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.RedstoneLampBlock;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.event.BuildCreativeModeTabContentsEvent;
import net.neoforged.neoforge.event.entity.EntityAttributeCreationEvent;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredItem;
import net.neoforged.neoforge.registries.DeferredRegister;

/**
 * Registers the content of the IronPumpkin native mod {@code hello-mod} under the same names, so
 * the registry snapshot of the server matches this mod: one block, one item and one entity type.
 */
@Mod(HelloMod.MOD_ID)
public final class HelloMod {
    public static final String MOD_ID = "hello_mod";
    // FML rejects `-` in a mod id, but registry namespaces allow it: the server names use it.
    public static final String NAMESPACE = "hello-mod";

    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(NAMESPACE);
    private static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(NAMESPACE);
    private static final DeferredRegister.Entities ENTITY_TYPES = DeferredRegister.createEntities(NAMESPACE);

    // A redstone lamp copy: same `lit` property and default state as the server block.
    public static final DeferredBlock<RedstoneLampBlock> GREETER_LAMP = BLOCKS.registerBlock(
            "greeter_lamp", RedstoneLampBlock::new, () -> BlockBehaviour.Properties.ofFullCopy(Blocks.REDSTONE_LAMP));
    public static final DeferredItem<BlockItem> GREETER_LAMP_ITEM = ITEMS.registerSimpleBlockItem(GREETER_LAMP);
    // The server shows this entity as a pig but sends base entity data only, so the client Pig uses
    // its own defaults and attributes.
    public static final DeferredHolder<EntityType<?>, EntityType<Pig>> GREETER = ENTITY_TYPES.registerEntityType(
            "greeter", Pig::new, MobCategory.CREATURE, builder -> builder.sized(0.9F, 0.9F).passengerAttachments(0.86875F).clientTrackingRange(10));

    public HelloMod(IEventBus modBus) {
        BLOCKS.register(modBus);
        ITEMS.register(modBus);
        ENTITY_TYPES.register(modBus);
        modBus.addListener(HelloMod::createAttributes);
        modBus.addListener(HelloMod::addToCreativeTab);
    }

    private static void createAttributes(EntityAttributeCreationEvent event) {
        event.put(GREETER.get(), Pig.createAttributes().build());
    }

    private static void addToCreativeTab(BuildCreativeModeTabContentsEvent event) {
        if (event.getTabKey() == CreativeModeTabs.REDSTONE_BLOCKS) {
            event.accept(GREETER_LAMP_ITEM);
        }
    }
}
