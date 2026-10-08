# FTB StoneBlock 4 usage of NeoForge and Minecraft classes

This document lists the NeoForge, Minecraft and Mojang API that the 414 mods of the FTB StoneBlock 4 modpack reference, aggregated across mods. It is the second benchmark for the contract of the native API `ironpumpkin-neo`: the contract is checked against what a large pack needs, not against one mod. The single-mod scan of #35 is the first benchmark. The tracking issue is #46.

Source: the Prism Launcher instance `FTB StoneBlock 4` (Minecraft 1.21.1, NeoForge 21.1.248), folder `.minecraft/mods`, 414 jars, 738 MB. The folder was only read. No jar was run, and no class was loaded: jars are read with `zipfile`, class files with a class file parser. The scan used bytecode only, as in #35. The sources of the mods were not read.

## Generator command

The scripts live outside the repository, in the session scratchpad `stoneblock-scan/` (`scan.py`, `cf.py`, `idx.py`, `jdk.py`, `agg.py`, `ana.py`, `ana2.py`, `cov.py`, `gapdesc.py`, `report.py`). The whole pipeline reruns with `run_all.sh`, listed here in full. The scan ran on 2026-10-07. The pipeline needs about 10 s on 32 cores, with cold caches.

```sh
# Full pipeline for openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md (issue #46).
S=/tmp/claude-1000/-home-cappyt-Documenti-Repos-Pumpkin/b8ee3b0b-8eb9-4d18-ab4a-445ad25958ca/scratchpad/stoneblock-scan
M="$HOME/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances/FTB StoneBlock 4/.minecraft/mods"
P=$HOME/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/libraries
L=/tmp/claude-1000/-home-cappyt-Documenti-Repos-Pumpkin/b8ee3b0b-8eb9-4d18-ab4a-445ad25958ca/scratchpad/neoforge-server/libraries
V=/tmp/claude-1000/vanilla/META-INF/versions/26.3/server-26.3.jar
# 1. scan every jar and its Jar-in-Jar jars (cached per jar in $S/cache)
python3 -I $S/scan.py scan "$M" $S/cache 0 28
# 2. class and member indexes of the 26.3 artifacts (unpatched, then NeoForge-patched) and of the 21.1.248 artifacts
python3 -I $S/idx.py $S/idx/v263.pkl vanilla=$V brig=$L/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar dfu=$L/com/mojang/datafixerupper/10.0.21/datafixerupper-10.0.21.jar log=$L/com/mojang/logging/1.7.12/logging-1.7.12.jar auth=$L/com/mojang/authlib/10.0.77/authlib-10.0.77.jar
python3 -I $S/idx.py $S/idx/n263.pkl patched=$L/net/neoforged/minecraft-server-patched/26.3.0.52-beta/minecraft-server-patched-26.3.0.52-beta.jar universal=$L/net/neoforged/neoforge/26.3.0.52-beta/neoforge-26.3.0.52-beta-universal.jar fml=$L/net/neoforged/fancymodloader/loader/12.0.8/loader-12.0.8.jar bus=$L/net/neoforged/bus/8.0.5/bus-8.0.5.jar mergetool=$L/net/neoforged/mergetool/2.0.7/mergetool-2.0.7-api.jar brig=$L/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar dfu=$L/com/mojang/datafixerupper/10.0.21/datafixerupper-10.0.21.jar log=$L/com/mojang/logging/1.7.12/logging-1.7.12.jar auth=$L/com/mojang/authlib/10.0.77/authlib-10.0.77.jar
python3 -I $S/idx.py $S/idx/n211.pkl universal=$P/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-universal.jar clientpatched=$P/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-client.jar fml=$P/net/neoforged/fancymodloader/loader/4.0.43/loader-4.0.43.jar bus=$P/net/neoforged/bus/8.0.5/bus-8.0.5.jar mergetool=$P/net/neoforged/mergetool/2.0.0/mergetool-2.0.0-api.jar
# 3. JDK supertypes of those classes, through javap
python3 -I $S/jdk.py
# 4. aggregation, 26.3 check, coverage against mapping-table.md, markdown
python3 -I $S/report.py /home/cappyt/Documenti/Repos/Pumpkin/openspec/changes/neoforge-shaped-plugin-api/modpack-usage.md

```

The coverage section reads `mapping-table.md` as it was at generation time: SHA-1 `206a846fe701`, 1299 rows. Section 11 therefore predates the rewrite of `mapping-table.md` for #49, which added the mods column, the gap verdicts and the WIT additions; the current coverage is in the Coverage table of `mapping-table.md`.

Section 9 comes from the scanner `tools/modpack-scan/mixin_scan.py` in this repository. It reads the same folder, and its README gives the command.

The class file parser was checked against `javap -p -v` on 351 random classes of 60 random jars: 7320 member references, 0 differences, and the `@SubscribeEvent` counts match. The mod counts of eight NeoForge classes were checked against a raw byte search over all class files of all jars and nested jars: seven match exactly, `IEventBus` differs by one jar (376 against 377), `IItemHandler` by the prefix match of `IItemHandlerModifiable`. The scan has no `javap` pass over the whole pack: `javap` on 118,000 classes adds nothing that the constant pool does not give, and the parser needs 2 s for all jars. The NeoForge 26.3.x source tree was not needed: the universal jar of 26.3.0.52-beta gives the signatures.

## 1. Summary

### 1.1 Totals

| Measure | Value |
|:--|:--|
| Mod jars in the folder | 414 (738 MB) |
| Nested Jar-in-Jar jars (`META-INF/jarjar`) | 106 (93 distinct by SHA-1). 51 of them have their own `mods.toml` |
| Mods with a `neoforge.mods.toml` or `mods.toml` | 413 of 414 jars carry their own. `kotlinforforge-5.12.0-all.jar` carries it in a nested jar. Loaders: javafml 407, kotlinforforge 5, lowcodefml 2 |
| Class files | 118462 (99299 in the outer jars, 19163 in nested jars; nested jars hold 18010 distinct class files) |
| Class files by side (heuristic, section 1.2) | server 90930, client 16201, datagen and game test 2895, mixed 8436 |
| Referenced NeoForge classes (`net.neoforged.*`) | 947. Server 644, client-only 183, dev-only 43, client+dev 77 |
| Referenced Minecraft classes (`net.minecraft.*`) | 4116. Server 2980, client-only 868, dev-only 203, client+dev 65. 898 are in `net.minecraft.client.*` |
| Referenced Mojang library classes (`com.mojang.*`) | 272. Server 165, client-only 98, dev-only 1, client+dev 8 |
| Distinct members referenced (owner, name, descriptor; NeoForge, Minecraft and Mojang owners) | 35836 (26534 methods, 9302 fields). Server-side 30718 |
| Call sites of NeoForge members | 134438 (`invoke*` instructions on `net.neoforged.*` owners) |
| Event handlers | 3666 `@SubscribeEvent` methods in 1617 classes plus 2565 `addListener` calls, for 305 distinct NeoForge or Minecraft event classes (section 5) |
| Registrations through `DeferredRegister` | 10860 register call sites resolved to a registry in 210 mods (section 6) |
| Mixins | 3508 `@Mixin` classes in 259 mods, 460 mixin config files (section 8) |
| Mixin members | 9183 in all mixin classes. 2674 server-side members with a vanilla target, in 157 mods, into 1931 target members of 519 classes (section 9) |
| Network payload types | 1120 classes implement `CustomPacketPayload` in 188 mods; 1215 `PayloadRegistrar` registration calls |
| Not at the same name in 26.3, server side | NeoForge 88 of 644, Minecraft 885 of 2980 (section 10) |

The server contract is the set of classes that server-side or mixed mod classes reference: 644 NeoForge, 2980 Minecraft and 165 Mojang classes. Client-only and dev-only classes are not part of a server-side plugin API.

### 1.2 How to read the tables

- **Mods (srv)** is the number of the 414 mods (outer jar, nested jars included) that have at least one server-side or mixed class referencing the API class. **Mods (all)** also counts client and dev classes. One class used by 1 mod of 414 is lower priority than one used by 200.
- **Refs** is the number of mod class files that reference the API class and are server-side or mixed. A class file is one `.class` entry, so nested and anonymous classes count on their own.
- **S/C/D/M** splits all refs by the side of the referencing mod class. A mod class is `D` (dev) when its package has a segment `datagen`, `gametest`, `test` or `tests`, or it references `net.minecraft.data`, `net.minecraft.gametest` or the NeoForge data classes. It is `C` (client) when the mod is client-only in its `mods.toml` (the `neoforge` and `minecraft` dependencies have `side = "CLIENT"`), or it carries `@OnlyIn(Dist.CLIENT)`, `@EventBusSubscriber(value = Dist.CLIENT)` or `@Mod(dist = Dist.CLIENT)`, or its package has a segment `client`. It is `M` (mixed) when it has no such marker but references client API (`net.minecraft.client`, `com.mojang.blaze3d`, `net.neoforged.neoforge.client`, LWJGL, JOML). `S` (server) is everything else. Mixed classes count as server-side in the **srv** columns, so the srv numbers are an upper bound: a common class with a client-only helper method is mixed. Method-level `@OnlyIn` is not tracked.
- Sensitivity: if mixed classes are not counted as server-side, the server NeoForge classes fall from 644 to 632 and the server Minecraft classes from 2980 to 2931 (client API classes are never server-side here: `net.minecraft.client`, `blaze3d` and `neoforge.client` classes are removed from the srv columns whoever references them).
- **26.3** is `same` when a class with this exact name exists in the 26.3 artifacts, `moved` when a class with the same simple name exists under another package (or the manual rename list of #35 applies), and `GONE` when neither exists. `client` means the class is in a client package that the 26.3 server artifacts do not hold, so it is not checked. Section 10 gives the method.
- **Members n/u** is the number of distinct members of the class that server-side or mixed classes reference (owner, name and erased descriptor), and the number of them that do not resolve in 26.3. `same` does not mean the members are unchanged.
- A **mod** is an outer jar. Nested jars count for the jar that carries them: a library that is shaded as Jar-in-Jar into 10 mods counts 10 times, once per carrying mod.

### 1.3 Totals per API family

NeoForge families (package group, as in `mapping-table.md`). The last column counts server classes that are `moved` or `GONE` in 26.3.

| Family | Classes | Server | Client-only | Dev-only | Refs (srv) | Not same in 26.3 |
|:--|--:|--:|--:|--:|--:|--:|
| neoforge.registries | 53 | 52 | 0 | 1 | 9566 | 5 |
| neoforge.common | 174 | 145 | 3 | 26 | 9038 | 20 |
| neoforge.capabilities | 15 | 15 | 0 | 0 | 4542 | 3 |
| neoforge.event | 220 | 219 | 1 | 0 | 4356 | 11 |
| neoforge.fluids | 39 | 39 | 0 | 0 | 4170 | 17 |
| bus | 13 | 13 | 0 | 0 | 3281 | 0 |
| neoforge.items | 19 | 19 | 0 | 0 | 3058 | 19 |
| neoforge.network | 15 | 15 | 0 | 0 | 2649 | 2 |
| distmarker | 2 | 2 | 0 | 0 | 2268 | 0 |
| fml.common | 6 | 4 | 2 | 0 | 1267 | 1 |
| fml.(root) | 17 | 17 | 0 | 0 | 1193 | 0 |
| fml.config | 6 | 6 | 0 | 0 | 821 | 0 |
| fml.event | 13 | 13 | 0 | 0 | 699 | 0 |
| fml.loading | 26 | 26 | 0 | 0 | 641 | 1 |
| neoforge.attachment | 6 | 6 | 0 | 0 | 556 | 0 |
| neoforge.energy | 4 | 4 | 0 | 0 | 545 | 4 |
| neoforgespi | 23 | 20 | 3 | 0 | 328 | 2 |
| neoforge.server | 13 | 13 | 0 | 0 | 244 | 1 |
| neoforge.entity | 2 | 2 | 0 | 0 | 112 | 0 |
| fml.util | 5 | 5 | 0 | 0 | 106 | 0 |
| coremod | 1 | 1 | 0 | 0 | 35 | 1 |
| fml.javafmlmod | 2 | 2 | 0 | 0 | 12 | 0 |
| neoforge.resource | 2 | 2 | 0 | 0 | 8 | 0 |
| neoforge.internal | 2 | 2 | 0 | 0 | 4 | 1 |
| fml.mclanguageprovider | 1 | 1 | 0 | 0 | 1 | 0 |
| fml.i18n | 1 | 1 | 0 | 0 | 1 | 0 |
| neoforge.client | 255 | 0 | 247 | 8 | 0 | 0 |
| neoforge.data | 5 | 0 | 0 | 5 | 0 | 0 |
| neoforge.gametest | 3 | 0 | 0 | 3 | 0 | 0 |
| fml.earlydisplay | 3 | 0 | 3 | 0 | 0 | 0 |
| jarjar | 1 | 0 | 1 | 0 | 0 | 0 |

Minecraft families by package, the 40 with most server refs. 17 more families have client-only or dev-only classes only, or fewer refs.

| Family | Classes | Server | Client-only | Dev-only | Refs (srv) | Not same in 26.3 |
|:--|--:|--:|--:|--:|--:|--:|
| net.minecraft.core | 86 | 81 | 2 | 3 | 60169 | 5 |
| net.minecraft.world.level.block | 357 | 347 | 3 | 7 | 55575 | 18 |
| net.minecraft.world.item | 269 | 258 | 1 | 10 | 49922 | 49 |
| net.minecraft.network | 194 | 184 | 10 | 0 | 41381 | 20 |
| net.minecraft.world.level | 59 | 59 | 0 | 0 | 25479 | 9 |
| net.minecraft.world.entity | 69 | 68 | 1 | 0 | 18349 | 7 |
| net.minecraft.resources | 12 | 12 | 0 | 0 | 17919 | 5 |
| net.minecraft.world.phys | 17 | 17 | 0 | 0 | 13804 | 0 |
| net.minecraft.world.entity.player | 9 | 6 | 3 | 0 | 13483 | 0 |
| net.minecraft.server | 141 | 139 | 2 | 0 | 11458 | 13 |
| net.minecraft.util | 86 | 79 | 4 | 3 | 8605 | 19 |
| net.minecraft.world | 26 | 25 | 0 | 1 | 8418 | 3 |
| net.minecraft.nbt | 30 | 30 | 0 | 0 | 8190 | 0 |
| net.minecraft.world.entity.ai | 186 | 186 | 0 | 0 | 6914 | 4 |
| net.minecraft.world.inventory | 55 | 55 | 0 | 0 | 6296 | 1 |
| net.minecraft | 398 | 396 | 2 | 0 | 6154 | 382 |
| net.minecraft.sounds | 5 | 4 | 1 | 0 | 5322 | 0 |
| net.minecraft.world.level.levelgen | 256 | 227 | 1 | 28 | 4232 | 47 |
| net.minecraft.tags | 20 | 18 | 0 | 2 | 3772 | 2 |
| net.minecraft.world.level.material | 10 | 9 | 1 | 0 | 3685 | 0 |
| net.minecraft.world.level.storage | 178 | 167 | 1 | 10 | 3601 | 30 |
| net.minecraft.commands | 73 | 73 | 0 | 0 | 2146 | 3 |
| net.minecraft.world.damagesource | 11 | 9 | 0 | 2 | 2016 | 0 |
| net.minecraft.world.effect | 7 | 7 | 0 | 0 | 1958 | 1 |
| net.minecraft.advancements | 145 | 127 | 0 | 18 | 1417 | 113 |
| net.minecraft.world.entity.item | 3 | 3 | 0 | 0 | 1098 | 0 |
| net.minecraft.world.level.chunk | 36 | 35 | 1 | 0 | 947 | 3 |
| net.minecraft.world.level.biome | 37 | 35 | 0 | 2 | 659 | 4 |
| net.minecraft.world.entity.projectile | 29 | 29 | 0 | 0 | 617 | 19 |
| net.minecraft.world.entity.monster | 52 | 52 | 0 | 0 | 616 | 25 |
| net.minecraft.stats | 8 | 8 | 0 | 0 | 611 | 0 |
| net.minecraft.world.entity.animal | 69 | 69 | 0 | 0 | 547 | 57 |
| net.minecraft.world.level.pathfinder | 14 | 14 | 0 | 0 | 482 | 0 |
| net.minecraft.world.entity.npc | 23 | 23 | 0 | 0 | 440 | 20 |
| net.minecraft.world.level.gameevent | 9 | 9 | 0 | 0 | 428 | 0 |
| net.minecraft.world.level.saveddata | 10 | 10 | 0 | 0 | 359 | 1 |
| net.minecraft.world.food | 6 | 6 | 0 | 0 | 247 | 1 |
| net.minecraft.world.flag | 5 | 5 | 0 | 0 | 212 | 0 |
| net.minecraft.world.level.entity | 14 | 14 | 0 | 0 | 145 | 0 |
| net.minecraft.world.level.dimension | 6 | 6 | 0 | 0 | 137 | 2 |

| Family | Classes | Server | Client-only | Dev-only | Refs (srv) | Not same in 26.3 |
|:--|--:|--:|--:|--:|--:|--:|
| serialization | 47 | 47 | 0 | 0 | 16182 | 0 |
| datafixers | 55 | 54 | 0 | 1 | 12660 | 0 |
| brigadier | 37 | 37 | 0 | 0 | 4448 | 0 |
| authlib | 15 | 15 | 0 | 0 | 413 | 4 |
| math | 6 | 6 | 0 | 0 | 345 | 0 |
| logging | 1 | 1 | 0 | 0 | 192 | 0 |
| util | 1 | 1 | 0 | 0 | 9 | 0 |
| text2speech | 4 | 4 | 0 | 0 | 8 | 4 |
| blaze3d | 75 | 0 | 75 | 0 | 0 | 0 |
| realmsclient | 31 | 0 | 31 | 0 | 0 | 0 |

### 1.4 Top 20 most referenced classes

| # | Class | Refs (srv) | Mods (srv) | S/C/D/M | 26.3 |
|:--|--:|--:|--:|--:|:--|
| 1 | `net.minecraft.world.item.ItemStack` | 15659 | 327 | 13702/2079/438/1957 | same |
| 2 | `net.minecraft.core.BlockPos` | 13843 | 296 | 12510/1344/222/1333 | same |
| 3 | `net.minecraft.world.level.Level` | 13802 | 310 | 12494/1036/162/1308 | same |
| 4 | `net.minecraft.resources.ResourceLocation` | 13144 | 367 | 10453/4000/1575/2691 | moved |
| 5 | `net.minecraft.world.level.block.state.BlockState` | 10604 | 280 | 9419/1254/235/1185 | same |
| 6 | `net.minecraft.world.entity.player.Player` | 10357 | 302 | 9348/731/64/1009 | same |
| 7 | `net.minecraft.network.chat.Component` | 9090 | 335 | 6450/3216/151/2640 | same |
| 8 | `net.minecraft.world.item.Item` | 8704 | 312 | 7665/980/997/1039 | same |
| 9 | `net.minecraft.core.Direction` | 7546 | 249 | 6646/1204/174/900 | same |
| 10 | `net.minecraft.world.level.block.Block` | 7384 | 275 | 6630/609/807/754 | same |
| 11 | `net.minecraft.network.chat.MutableComponent` | 6872 | 325 | 4922/1994/122/1950 | same |
| 12 | `net.minecraft.world.entity.Entity` | 6291 | 249 | 5560/1185/35/731 | same |
| 13 | `net.neoforged.neoforge.registries.DeferredHolder` | 5678 | 224 | 5178/450/535/500 | same |
| 14 | `net.minecraft.world.level.block.entity.BlockEntity` | 5643 | 235 | 4990/823/143/653 | same |
| 15 | `net.minecraft.core.Holder` | 5210 | 258 | 4745/398/443/465 | same |
| 16 | `net.minecraft.world.entity.LivingEntity` | 5006 | 219 | 4425/720/33/581 | same |
| 17 | `net.minecraft.network.codec.StreamCodec` | 4845 | 252 | 4560/172/48/285 | same |
| 18 | `net.minecraft.nbt.CompoundTag` | 4762 | 257 | 4402/142/71/360 | same |
| 19 | `net.minecraft.world.phys.Vec3` | 4752 | 219 | 3971/847/24/781 | same |
| 20 | `net.minecraft.resources.ResourceKey` | 4485 | 306 | 4117/252/596/368 | same |

### 1.5 Findings

- **Core surface.** 8 NeoForge classes are used by 200 or more mods (`IEventBus`, `Mod`, `ModContainer`, `DeferredHolder`, `NeoForge`, `DeferredRegister`, `SubscribeEvent`, `ModList`) and 26 more by 100 to 199 mods. 281 server-side NeoForge classes are used by 8 or more mods (section 3 table), and 363 by fewer (long tail).
- **Contract.** Of 1274 contract rows 1011 are needed by at least one mod, 669 by 10 or more and 348 by 50 or more (section 11.1). 405 of 414 mods need at least one gap or contract-only row. The gap rows of the NeoForge inventory alone hit 383 mods. If the rows that Rust std and the byte-level payload design cover (Java helpers, DataFixerUpper codecs, stream codecs) are left out, 397 mods still need a gap row.
- **Largest gaps** (section 11.2): Java helpers (312 mods); Mixins (291 mods); DFU codecs (278 mods); Stream codecs and byte buffers (263 mods); Registries the host rejects (247 mods); Access transformers (183 mods); Container menus (176 mods); Block shapes (170 mods).
- **Mixins.** 259 of 414 mods ship mixin classes (3508 mixin classes); 165 mods have at least one common or server mixin into vanilla server code. These mods cannot be ported by an API alone. 183 mods ship or declare an access transformer. At member level (section 9), 41% of the server-side members into vanilla are accessors, 66% of the other members act at the start or the end of a method, and 1% override a whole method. The demand is flat: the 200 most targeted members cover 11% of the mods.
- **26.3.** The 21.1 item, fluid and energy handler classes are GONE in 26.3 (the transfer rework): `IItemHandler` 139 mods, `IFluidHandler` 89, `IEnergyStorage` 75, `Capabilities$ItemHandler` 112. `EventBusSubscriber$Bus` is GONE and used by 69 mods. 36% of the server-side members that mods reference do not resolve in 26.3, and 381 of 414 mods reference at least one of them (section 10.2).
- **Side.** 17 mods are client-only in their `mods.toml`; 18 mods have no server-side class. `@OnlyIn` appears 2074 times in 111 mods (classes 642, methods 1397, fields 35). 987 handlers are `@EventBusSubscriber(value = Dist.CLIENT)` only.

## 2. Per-mod table

One row per jar. **Side (toml)** is the `side` of the `neoforge` and `minecraft` dependencies in `mods.toml` (`BOTH` is the default; `-` means the file declares none). **Side (classes)** is `server/common` when no class is client, `client` when no class is server-side, `both` otherwise, and `client (toml)` when the toml makes the mod client-only. **Mx** is the number of `@Mixin` classes. **Ev** is the number of distinct event classes the mod subscribes to. **Top NeoForge** and **Top Minecraft** are the three classes with most server-side refs inside the mod. A `+N` after the class count is the number of nested jars.

| # | Mod id | Jar | Side (toml) | Side (classes) | Classes | S/C/D/M | Mx | Ev | Top NeoForge | Top Minecraft |
|--:|:--|:--|:--|:--|--:|--:|--:|--:|:--|:--|
| 1 | aaron | aaron-1.21.1-1.20.0-build.28 | BOTH | both | 216 | 190/5/10/11 | 1 | 7 | DeferredHolder, DeferredRegister, ModConfigSpec | StreamCodec, ResourceLocation, BlockPos |
| 2 | accelerateddecay | accelerated-decay-neoforge-21.0.0 | BOTH | server/common | 4 | 4/0/0/0 | 0 | 0 | Mod | Level, ResourceKey, ServerPlayer |
| 3 | actuallyadditions | actuallyadditions-1.3.26+mc1.21.1 | BOTH | both | 548 | 425/1/45/77 | 3 | 32 | IEnergyStorage, FluidStack, ModConfigSpec | ItemStack, Level, BlockPos |
| 4 | advanced_ae | AdvancedAE-1.6.12-1.21.1 | BOTH | both | 422 +2 | 275/99/18/30 | 16 | 24 | Dist, FluidStack, DeferredHolder | ItemStack, Player, Item |
| 5 | aci | AdvancedCoreInfo-neoforge-1.21.1-1.1.0 | BOTH | server/common | 65 | 56/0/5/4 | 0 | 1 | - | ResourceLocation, Component, RegistryFriendlyByteBuf |
| 6 | ali | AdvancedLootInfo-neoforge-1.21.1-2.1.0 | BOTH | both | 325 | 254/24/8/39 | 12 | 10 | LootModifier, ItemAbility, IPayloadContext | ResourceLocation, ItemStack, Item |
| 7 | advancedperipherals | AdvancedPeripherals-1.21.1-0.7.62b | BOTH | both | 257 | 222/4/10/21 | 0 | 18 | ModConfigSpec, DeferredHolder, ModConfigSpec.BooleanValue | ItemStack, BlockPos, Level |
| 8 | advancementplaques | AdvancementPlaques-1.21.1-neoforge-1.6.8 | BOTH | both | 14 | 9/3/0/2 | 0 | 1 | - | AdvancementHolder, SoundEvent, ResourceLocation |
| 9 | ae2things | AE2-Things-1.4.2-beta | BOTH | both | 14 | 11/1/2/0 | 1 | 7 | DeferredItem, RegisterCommandsEvent, DeferredRegister.Items | Item, ItemStack, ItemLike |
| 10 | ae2addonlib | AE2AddonLib-1.0.3-1.21.1 | BOTH | both | 92 | 43/38/6/5 | 0 | 4 | Dist, IEventBus, FMLEnvironment | StreamCodec, Item, CustomPacketPayload |
| 11 | ae2ct | ae2ct-1.21.1-1.1.1 | BOTH | server/common | 21 | 14/0/0/7 | 2 | 0 | ModConfigSpec, ModConfigSpec.BooleanValue, FluidStack | Component, MutableComponent, ResourceLocation |
| 12 | ae2jeiintegration | ae2jeiintegration-1.2.1 | - | server/common | 52 | 39/0/0/13 | 0 | 0 | FluidStack, Mod, ModList | MutableComponent, Component, ItemStack |
| 13 | ae2netanalyser | AE2NetworkAnalyzer-1.21-2.1.5-neoforge | BOTH | both | 57 | 37/15/2/3 | 1 | 11 | Dist, OnlyIn, IEventBus | Player, ResourceLocation, FriendlyByteBuf |
| 14 | ae2wtlib | ae2wtlib-19.5.1 | - | both | 125 +1 | 107/1/0/17 | 5 | 13 | PacketDistributor, DeferredRegister, NeoForgeStreamCodecs | ItemStack, Player, DataComponentType |
| 15 | akashictome | AkashicTome-1.8-30 | BOTH | both | 17 | 12/3/0/2 | 0 | 5 | DeferredItem, ModConfigSpec, ModConfigSpec.ConfigValue | ItemStack, Item, Player |
| 16 | alltheleaks | alltheleaks-1.1.12+1.21.1-neoforge | BOTH | both | 322 +1 | 189/90/0/43 | 123 | 31 | IEventBus, NeoForge, LevelEvent | Level, ClientLevel, ItemStack |
| 17 | almostunified | almostunified-neoforge-1.21.1-1.4.2 | BOTH | server/common | 165 | 160/0/0/5 | 12 | 4 | ConditionalOps, NeoForgeRegistries, ServerTickEvent | ResourceLocation, TagKey, Item |
| 18 | amendments | amendments-1.21-2.1.10-neoforge | BOTH | both | 258 | 185/55/0/18 | 56 | 5 | SubscribeEvent, NeoForgeRegistries, NeoForgeRegistries.Keys | BlockState, BlockPos, Level |
| 19 | antiblocksrechiseled | antiblocksrechiseled-neo-0.10.8 | BOTH | server/common | 32 | 16/0/16/0 | 0 | 1 | IEventBus, DeferredBlock, DeferredRegister | Block, Item, BlockState |
| 20 | apotheosis | Apotheosis-1.21.1-8.8.0 | BOTH | both | 547 | 455/35/31/26 | 22 | 49 | AttributeTooltipContext, IItemHandler, AttachmentType | ItemStack, ResourceLocation, Component |
| 21 | apothic_compat | apothic_compat-2.0.2 | BOTH | server/common | 5 | 5/0/0/0 | 0 | 3 | RegisterCommandsEvent, NeoForge, Mod | ResourceLocation, ServerPlayer, CommandSourceStack |
| 22 | apothic_compats | apothic_compats-0.2.4.2 | BOTH | server/common | 94 | 29/0/65/0 | 3 | 5 | AttributeTooltipContext, ModList, EntityInvulnerabilityCheckEvent | TagKey, ItemStack, ResourceLocation |
| 23 | apothic_tooltip_cleanup | apothic_tooltip_cleanup-1.3.0 | BOTH | both | 21 | 13/2/0/6 | 0 | 2 | ModConfigSpec, ModConfigSpec.ConfigValue, ModConfigSpec.BooleanValue | Component, Screen, MutableComponent |
| 24 | apothic_attributes | ApothicAttributes-1.21.1-2.10.1 | BOTH | both | 124 | 101/18/2/3 | 8 | 31 | DeferredHolder, AttachmentType, Event | Holder, ResourceLocation, LivingEntity |
| 25 | apothic_enchanting | ApothicEnchanting-1.21.1-1.6.2 | BOTH | both | 153 | 129/4/9/11 | 20 | 28 | AttachmentType, PacketDistributor, Tags | ItemStack, Holder, BlockPos |
| 26 | apothic_spawners | ApothicSpawners-1.21.1-1.4.0 | BOTH | server/common | 33 | 29/0/0/4 | 2 | 12 | IEventBus, ObfuscationReflectionHelper, SubscribeEvent | ResourceLocation, Component, MutableComponent |
| 27 | appleskin | appleskin-neoforge-mc1.21-3.0.9 | CLIENT | client (toml) | 36 | 0/36/0/0 | 0 | 9 | - | - |
| 28 | appex | applied-experienced-1.21.1-1.3.2 | BOTH | both | 73 | 49/14/9/1 | 1 | 11 | BlockCapability, DeferredRegister, ModConfigSpec | ItemStack, Item, Component |
| 29 | appmek | Applied-Mekanistics-1.6.3 | BOTH | server/common | 49 | 40/0/6/3 | 2 | 8 | BlockCapability, IFluidHandler, IFluidHandler.FluidAction | ResourceLocation, MethodsReturnNonnullByDefault, Component |
| 30 | appliedcooking | appliedcooking-6.2.1 | BOTH | server/common | 29 | 29/0/0/0 | 0 | 3 | DeferredHolder, IEventBus, DeferredRegister | ItemStack, ItemLike, ResourceKey |
| 31 | applieddelight | applieddelight-1.1.0 | BOTH | both | 32 | 30/2/0/0 | 0 | 4 | DeferredHolder, IEventBus, DeferredRegister | ItemStack, Component, ResourceKey |
| 32 | ae2 | appliedenergistics2-19.2.17 | BOTH | both | 1824 | 1348/310/70/96 | 16 | 52 | Dist, OnlyIn, IItemHandler | ItemStack, Level, BlockPos |
| 33 | appflux | AppliedFlux-1.21-2.1.5-neoforge | BOTH | both | 73 | 70/3/0/0 | 8 | 9 | IEnergyStorage, BlockCapability, Capabilities | Direction, BlockPos, ItemStack |
| 34 | appliedsticks | appliedsticks-1.21.1-1.2.1 | BOTH | both | 11 | 9/2/0/0 | 0 | 5 | RegisterPayloadHandlersEvent, IPayloadHandler, ModList | ItemStack, BlockPos, ResourceKey |
| 35 | architectury | architectury-13.0.11-neoforge | BOTH | both | 382 | 280/83/0/19 | 20 | 112 | Dist, OnlyIn, IEventBus | ResourceLocation, Player, ResourceKey |
| 36 | ars_affinity | ars_affinity-1.21.1-1.1.1 | BOTH | both | 342 +2 | 216/74/30/22 | 14 | 42 | DeferredHolder, SubscribeEvent, ModConfigSpec | Player, Level, Component |
| 37 | ars_creo | ars_creo-1.21.1-5.4.0 | BOTH | both | 47 | 35/8/0/4 | 2 | 7 | DeferredHolder, ModConfigSpec, DeferredRegister | BlockPos, Level, BlockState |
| 38 | ars_elemental | ars_elemental-1.21.1-0.7.10.1 | BOTH | both | 456 +1 | 339/51/44/22 | 31 | 55 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | Level, LivingEntity, BlockPos |
| 39 | ars_hex | ars_hex-1.21.1-5.0.4b | BOTH | both | 106 +1 | 67/3/19/17 | 6 | 22 | DeferredHolder, ModConfigSpec, IEventBus | ItemStack, Level, LivingEntity |
| 40 | ars_nouveau | ars_nouveau-1.21.1-5.13.1 | BOTH | both | 6169 +3 | 5492/412/107/158 | 46 | 110 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | Level, BlockPos, ItemStack |
| 41 | ars_ocultas | ars_ocultas-1.21.1-2.6.1 | BOTH | server/common | 45 | 30/0/10/5 | 0 | 13 | IEventBus, DeferredBlock, ModConfigSpec | BlockPos, Level, Component |
| 42 | ars_unification | ars_unification-1.2.21 | BOTH | both | 49 | 39/8/2/0 | 5 | 3 | DeferredHolder, ModConfigSpec, ModConfigSpec.BooleanValue | ItemStack, Ingredient, RecipeType |
| 43 | arseng | arseng-2.1.1-beta | - | both | 40 | 37/3/0/0 | 3 | 7 | BlockCapability, DeferredRegister, DeferredHolder | BlockPos, ItemStack, Direction |
| 44 | artifacts | artifacts-neoforge-13.2.5 | BOTH | both | 334 | 258/42/17/17 | 42 | 27 | IEventBus, DeferredRegister, DeferredHolder | Holder, LivingEntity, ItemStack |
| 45 | athena | athena-neoforge-1.21.1-4.0.6 | BOTH | both | 57 | 2/51/0/4 | 3 | 1 | Dist, IEventBus, FMLLoader | ResourceLocation, ResourceManager, BlockStateModelLoader.LoadedJson |
| 46 | atlas_api | atlas_api-1.21.1-1.2.0 | BOTH | server/common | 16 | 3/0/0/13 | 0 | 5 | IUnbakedGeometry, IGeometryLoader, EmptyModel | ResourceLocation, BakedModel, Registry |
| 47 | attributefix | attributefix-neoforge-1.21.1-21.1.3 | BOTH | server/common | 5 | 5/0/0/0 | 1 | 1 | FMLLoadCompleteEvent, Mod, EventBusSubscriber | ResourceLocation, RangedAttribute, BuiltInRegistries |
| 48 | authme | authme-neoforge-9.0.1+1.21.1 | BOTH/CLIENT | server/common | 26 | 11/0/0/15 | 7 | 0 | Mod, IEventBus | Component, Button, Screen |
| 49 | avaritia | AvaritiaNeo-1.21-1.3.1 | BOTH | both | 184 | 140/1/0/43 | 4 | 28 | DeferredHolder, IEventBus, ModConfigSpec | ItemStack, ResourceLocation, Item |
| 50 | baguettelib | baguettelib-1.21.1-NeoForge-2.0.6 | BOTH | both | 57 | 51/2/0/4 | 8 | 5 | Event, IEventBus, NeoForge | Player, ItemStack, StreamCodec |
| 51 | balm | balm-neoforge-1.21.1-21.0.65 | BOTH | both | 605 +1 | 450/124/0/31 | 21 | 100 | DeferredRegister, DeferredHolder, IEventBus | ResourceLocation, ResourceKey, Holder |
| 52 | bambooeverything | bambooeverything-neoforge-21.1.2+mc1.21.1 | BOTH | server/common | 14 | 13/0/0/1 | 0 | 2 | IEventBus, FMLCommonSetupEvent, FMLClientSetupEvent | BlockBehaviour, SoundType, BlockBehaviour.Properties |
| 53 | bhc | baubley-heart-canisters-1.21.1-1.4.1 | BOTH | both | 47 | 31/6/4/6 | 0 | 9 | DeferredHolder, ModConfigSpec, ComponentItemHandler | ItemStack, Item, Player |
| 54 | beer | beer-1.21.1-1.4.4 | BOTH | server/common | 32 | 28/0/2/2 | 2 | 7 | SubscribeEvent, IPayloadContext, EventBusSubscriber | BlockPos, Level, StreamCodec |
| 55 | belts | belts-neoforge-0.2.2 | BOTH | both | 30 | 19/8/0/3 | 0 | 2 | IItemHandler, Mod, IItemHandlerModifiable | BlockPos, Direction, Block |
| 56 | betteradvancedtooltips | better-advanced-tooltips-2101.1.0-build.5 | CLIENT | client (toml) | 9 | 0/9/0/0 | 2 | 1 | - | - |
| 57 | bcc | better-compatability-checker-neoforge-21.1.8 | BOTH | both | 16 | 13/1/0/2 | 5 | 1 | ModConfigSpec, ModConfigSpec.BooleanValue, ModConfigSpec.ConfigValue | ServerStatus, ServerData, ServerStatus.Players |
| 58 | betteradvancements | BetterAdvancements-NeoForge-1.21.1-0.4.3.21 | CLIENT | client (toml) | 36 | 0/36/0/0 | 0 | 5 | - | - |
| 59 | betterblockz | betterblockz-0.2.10-1.21.1-neoforge | BOTH | both | 95 | 63/7/10/15 | 0 | 15 | DeferredHolder, DeferredBlock, DeferredRegister | Block, BlockState, Direction |
| 60 | bhmenu | BHMenu-NeoForge-1.21-2.4.4 | - | both | 71 | 22/2/1/46 | 2 | 4 | ModConfigSpec, ModConfigSpec.ConfigValue, ModConfigSpec.BooleanValue | Component, Minecraft, Font |
| 61 | bonsaitrees4 | bonsaitrees4-4.0.9 | BOTH | both | 228 | 146/15/7/60 | 1 | 18 | DeferredHolder, ItemStackHandler, DeferredBlock | ResourceLocation, ItemStack, GuiGraphics |
| 62 | bookshelf | bookshelf-neoforge-1.21.1-21.1.81 | BOTH | both | 173 | 153/5/1/14 | 35 | 7 | NeoForgeRegistries, IngredientType, RegisterPayloadHandlersEvent | ResourceLocation, ItemStack, ResourceKey |
| 63 | bowinfinityfix | bowinfinityfix-1.21-neo-3.1.1 | BOTH | server/common | 2 | 2/0/0/0 | 1 | 1 | LivingGetProjectileEvent, NeoForge, Mod | EnchantmentHelper, ServerLevel, Item |
| 64 | brandonscore | BrandonsCore-1.21.1-3.2.1.309 | BOTH | both | 314 | 201/82/5/26 | 5 | 33 | IEventBus, NeoForge, Dist | CompoundTag, HolderLookup.Provider, HolderLookup |
| 65 | buildinggadgets2 | buildinggadgets2-1.3.9 | BOTH | both | 166 | 97/50/8/11 | 0 | 15 | DeferredHolder, IPayloadContext, Dist | ItemStack, Player, BlockPos |
| 66 | busy_villagers | busy_villagers-neoforge-2101.1.1 | BOTH | server/common | 4 | 4/0/0/0 | 2 | 1 | ModConfigSpec, ModConfig.Type, Mod | Activity, Schedule, Villager |
| 67 | cable_facades | cable_facades-1.21.1-NeoForge-2.1.4 | BOTH | both | 70 +1 | 49/14/0/7 | 7 | 14 | IPayloadContext, DeferredHolder, DeferredRegister | ResourceLocation, BlockState, BlockPos |
| 68 | cabletiers | cabletiers-neoforge-1.21.1-0.6.14 | BOTH | server/common | 253 | 198/0/18/37 | 18 | 9 | ModelProperty, ModelData, ModConfigSpec | Component, MutableComponent, ResourceLocation |
| 69 | caelus | caelus-neoforge-7.0.1+1.21.1 | BOTH | server/common | 24 | 22/0/0/2 | 4 | 3 | IEventBus, IPayloadContext, NeoForge | LivingEntity, Holder, Player |
| 70 | carbonconfig | CarbonConfig-Neoforge-1.21.1-2.0.2.1 | BOTH | both | 845 +1 | 775/1/0/69 | 0 | 14 | ModContainer, ModList, Dist | Component, MutableComponent, GuiGraphics |
| 71 | computercraft | cc-tweaked-1.21.1-forge-1.120.2 | BOTH | both | 1403 +2 | 1282/119/2/0 | 6 | 41 | BlockCapability, FluidStack, RegisterCapabilitiesEvent | ItemStack, Level, BlockPos |
| 72 | chancecubes | ChanceCubes-1.21.1-5.0.2.517 | - | both | 348 | 326/7/8/7 | 0 | 22 | DeferredBlock, DeferredHolder, ModConfigSpec | BlockPos, Player, ServerLevel |
| 73 | chipped | chipped-neoforge-1.21.1-4.0.2 | BOTH | both | 62 | 49/10/1/2 | 2 | 3 | Mod, IEventBus, BlockEntityTypeAddBlocksEvent | BlockBehaviour, BlockBehaviour.Properties, TransparentBlock |
| 74 | chisel | chisel-1.21.1-NeoForge-1.4.1 | BOTH | both | 157 | 117/22/11/7 | 4 | 26 | DeferredHolder, DeferredRegister, DeferredBlock | ItemStack, Block, ResourceLocation |
| 75 | classicperipherals | classicperipherals-neoforge-1.21.1-0.6.3 | BOTH | both | 4590 +3 | 4553/1/33/3 | 5 | 9 | IEventBus, ModList, DeferredHolder | Level, BlockPos, ItemStack |
| 76 | classicpipes | classicpipes-neoforge-1.21.1-1.1.7 | BOTH | both | 159 | 129/25/0/5 | 0 | 7 | IFluidHandler, FluidStack, IItemHandler | BlockPos, BlockState, ItemStack |
| 77 | clavis | Clavis-NEOFORGE-0.2.13+1.21.1 | BOTH | both | 97 | 65/27/0/5 | 14 | 5 | Dist, OnlyIn, EventBusSubscriber | BlockPos, Player, ResourceLocation |
| 78 | cleanswing | cleanswing-1.10-1.21 | BOTH | server/common | 1 | 0/0/0/1 | 0 | 1 | PlayerInteractEvent.LeftClickBlock, ItemAbilities, Mod | InteractionHand, BlockTags, LivingEntity |
| 79 | cloth_config | cloth-config-15.0.140-neoforge | BOTH | both | 633 | 493/85/1/54 | 0 | 0 | Dist, OnlyIn, FMLPaths | Component, GuiGraphics, MutableComponent |
| 80 | cmpreviewfixer | cmpreviewfixer-21.1-1.1.0 | CLIENT | client (toml) | 4 | 0/4/0/0 | 2 | 0 | - | - |
| 81 | codechickenlib | CodeChickenLib-1.21.1-4.6.1.529 | BOTH | both | 806 +1 | 633/4/39/130 | 5 | 14 | IEventBus, FluidStack, ModelData | ResourceLocation, Direction, Minecraft |
| 82 | cognition | Cognition-v2.4.13-1.21.1 | BOTH | server/common | 173 | 138/0/0/35 | 0 | 14 | DeferredHolder, ModConfigSpec, ModConfigSpec.ConfigValue | ItemStack, BlockPos, Level |
| 83 | collapsible_groups | collapsible_groups-neoforge-1.21.1-1.4.4 | BOTH | both | 378 | 235/117/0/26 | 5 | 6 | FluidStack, ModList, ModConfigSpec | ResourceLocation, ItemStack, Component |
| 84 | collective | collective-1.21.1-8.39 | BOTH | server/common | 607 | 559/0/0/48 | 80 | 16 | IEventBus, NeoForge, ModLoadingContext | class_1937, Level, class_2338 |
| 85 | colorfulallays | colorfulallays-1.21.1-1.0.0 | BOTH | both | 13 | 8/3/2/0 | 2 | 7 | SubscribeEvent, EventBusSubscriber, IEventBus | DyeColor, EntityType, RandomSource |
| 86 | colorfulhearts | colorfulhearts-neoforge-1.21.1-10.5.9 | CLIENT | client (toml) | 53 | 0/53/0/0 | 3 | 9 | - | - |
| 87 | compactmachines | compactmachines-neoforge-7.0.81 | BOTH | both | 362 +8 | 288/23/0/51 | 4 | 33 | DeferredHolder, DeferredRegister, IAttachmentHolder | ResourceLocation, MinecraftServer, BlockPos |
| 88 | configureddefaults | ConfiguredDefaults-v21.1.3-1.21.1-NeoForge | BOTH | both | 6 | 5/1/0/0 | 0 | 0 | Dist, ModFileScanData, ModLoadingException | - |
| 89 | connectedglass | connectedglass-1.1.14-neoforge-mc1.21 | BOTH | server/common | 21 | 16/0/5/0 | 0 | 0 | Tags.Items, Tags.Blocks, Tags | DyeColor, Block, ResourceLocation |
| 90 | constructionstick | ConstructionSticks-1.21.1-1.5.0 | BOTH | both | 94 | 76/4/9/5 | 0 | 12 | DeferredHolder, ModConfigSpec, IPayloadContext | ItemStack, ResourceLocation, Item |
| 91 | controlling | Controlling-neoforge-1.21.1-19.0.5 | CLIENT | client (toml) | 42 | 0/42/0/0 | 5 | 2 | - | - |
| 92 | cookingforblockheads | cookingforblockheads-neoforge-1.21.1-21.1.24 | BOTH | both | 229 | 197/28/0/4 | 0 | 3 | IItemHandler, Capabilities, Capabilities.ItemHandler | ItemStack, Player, Level |
| 93 | coroutil | coroutil-neoforge-1.21.0-1.3.9 | BOTH | server/common | 44 | 39/0/0/5 | 0 | 4 | RegisterClientCommandsEvent, ClientTickEvent.Post, ClientTickEvent | Level, BlockPos, Vec3 |
| 94 | cosmeticarmorreworked | cosmeticarmorreworked-1.21.1-v1-neoforge | BOTH | both | 39 | 25/13/0/1 | 0 | 19 | Dist, ModConfigSpec, ModConfigSpec.BooleanValue | ResourceLocation, ItemStack, Player |
| 95 | craftingstation | craftingstationjei-1.21.1-NeoForge-2.1.1 | BOTH | both | 30 | 25/4/0/1 | 0 | 5 | IItemHandler, DeferredRegister, DeferredHolder | AbstractContainerMenu, Player, ItemStack |
| 96 | craftingtweaks | craftingtweaks-neoforge-1.21.1-21.1.11 | BOTH | both | 101 | 84/10/0/7 | 0 | 1 | InterModProcessEvent, InterModComms, InterModComms.IMCMessage | AbstractContainerMenu, Player, ResourceLocation |
| 97 | crash_assistant | CrashAssistant-neoforge-1.20.6-1.21.4-1.11.1 | BOTH | server/common | 1447 +2 | 1444/0/0/3 | 4 | 2 | JarInJarDependencyLocator, IModFileInfo, ModFileInfoParser | ChatFormatting, MutableComponent, Component |
| 98 | crashutilities | crashutilities-9.0.4 | BOTH | server/common | 92 | 73/0/0/19 | 0 | 11 | IPayloadContext, FakePlayer, SubscribeEvent | Component, CommandSourceStack, ResourceLocation |
| 99 | create | create-1.21.1-6.0.10 | BOTH | both | 3805 +3 | 2261/642/129/773 | 101 | 94 | Dist, OnlyIn, IItemHandler | BlockPos, BlockState, Level |
| 100 | create_central_kitchen | create-central-kitchen-2.6.0 | BOTH | both | 228 | 195/16/16/1 | 26 | 9 | SubscribeEvent, FMLConstructModEvent, DeferredHolder | Level, BlockPos, BlockState |
| 101 | create_enchantment_industry | create-enchantment-industry-2.5.3b | BOTH | both | 483 +1 | 385/30/23/45 | 24 | 19 | FluidStack, IEventBus, IFluidHandler | Level, ItemStack, BlockPos |
| 102 | create_shimmer | create-shimmer-1.3.1 | BOTH | both | 113 | 89/6/5/13 | 6 | 20 | DeferredHolder, IEventBus, FluidStack | ItemStack, Level, ResourceLocation |
| 103 | create_connected | create_connected-1.3.3-mc1.21.1 | BOTH | both | 415 +1 | 325/1/39/50 | 54 | 14 | DeferredHolder, FluidStack, ModelData | BlockPos, BlockState, Direction |
| 104 | create_hypertube | create_hypertube-0.6.0-NEOFORGE | BOTH | both | 141 | 95/24/2/20 | 13 | 24 | Dist, OnlyIn, DeferredHolder | BlockPos, Direction, BlockState |
| 105 | create_pattern_schematics | create_pattern_schematics-2.0.10 | BOTH | both | 58 | 44/3/0/11 | 19 | 7 | DeferredHolder, IItemHandler, IEventBus | ItemStack, BlockPos, Item |
| 106 | createaddition | createaddition-1.6.0 | BOTH | both | 259 +1 | 183/2/35/39 | 7 | 16 | ModConfigSpec, IEnergyStorage, ModConfigSpec.IntValue | BlockPos, BlockState, Level |
| 107 | create_dragons_plus | CreateDragonsPlus-1.11.8b | BOTH | both | 389 +1 | 318/25/21/25 | 36 | 17 | FluidStack, IEventBus, DeferredHolder | Level, ResourceLocation, ItemStack |
| 108 | createenchantablemachinery | createenchantablemachinery-3.6.0+mc1.21.1-ne | BOTH | both | 142 | 99/8/0/35 | 15 | 8 | ModConfigSpec, ModConfigSpec.ConfigValue, RegisterCapabilitiesEvent | BlockPos, BlockState, Level |
| 109 | createenderstorage | createenderstorage-1.1.1 | BOTH | both | 12 | 9/1/2/0 | 0 | 2 | IFluidHandler.FluidAction, FluidStack, IFluidHandler | BlockPos, BlockState, BlockEntity |
| 110 | createultimine | createultimine-1.21.1-neoforge-1.3.2 | BOTH | server/common | 6 | 6/0/0/0 | 0 | 0 | Mod, Tags.Items, Tags | ServerPlayer, CommandBuildContext, Commands |
| 111 | crystalix | crystalix-3.0.1 | BOTH | both | 72 | 48/8/14/2 | 0 | 11 | ModConfig, DeferredHolder, IPayloadContext | ItemStack, ResourceLocation, MutableComponent |
| 112 | ctm | CTM-1.21-1.2.1+3 | BOTH | both | 145 | 14/124/0/7 | 1 | 6 | ModConfig.Type, ModConfigSpec.BooleanValue, ModConfigEvent | TextureAtlasSprite, MethodsReturnNonnullByDefault, BlockState |
| 113 | cucumber | Cucumber-1.21.1-8.0.16 | BOTH | both | 138 | 114/16/0/8 | 4 | 6 | DeferredHolder, Event, DeferredRegister | ItemStack, Item, Player |
| 114 | cupboard | cupboard-1.21.1-4.1 | BOTH | server/common | 18 | 16/0/0/2 | 5 | 5 | Dist, FMLEnvironment, ModContainer | Minecraft, Screen, Util |
| 115 | curios | curios-neoforge-9.5.1+1.21.1 | BOTH | both | 152 | 117/27/1/7 | 13 | 32 | IEventBus, Dist, Event | ResourceLocation, ItemStack, LivingEntity |
| 116 | custommachineryars | custom_machinery_ars-1.2.5 | BOTH | both | 37 | 22/15/0/0 | 4 | 6 | DeferredHolder, BlockCapability, DeferredRegister | MutableComponent, Component, BlockPos |
| 117 | custommachinery | CustomMachinery-neoforge-1.21.1-0.10.69 | BOTH | both | 953 | 609/333/0/11 | 0 | 31 | IPayloadContext, FluidStack, PacketDistributor | ResourceLocation, Component, MutableComponent |
| 118 | custommachinerycreate | CustomMachineryCreate-1.21.1-1.2.7 | BOTH | both | 25 | 20/5/0/0 | 8 | 3 | DeferredRegister, IPayloadContext, EventBusSubscriber | BlockEntity, BlockPos, Component |
| 119 | custommachinerymekanism | CustomMachineryMekanism-1.21.1-1.4.16 | BOTH | both | 61 | 37/24/0/0 | 4 | 6 | BlockCapability, ICapabilityProvider, RegisterCapabilitiesEvent | Direction, MutableComponent, Component |
| 120 | deepdarkdimdungeons | deepdarkdimdungeons-1.21.1-1.0.1 | BOTH | server/common | 12 | 12/0/0/0 | 0 | 2 | DeferredRegister, ModConfigSpec, DeferredRegister.Blocks | BlockState, BlockPos, Block |
| 121 | dsp | default-server-properties-neoforge-21.0.1 | SERVER | server/common | 6 | 6/0/0/0 | 1 | 0 | FMLPaths, Mod | DedicatedServerProperties |
| 122 | derenderpatcher | derenderpatcher-2.0.0 | CLIENT | client (toml) | 16 | 0/16/0/0 | 5 | 2 | - | - |
| 123 | deus_ex_machina | deus_ex_machina-1.21.1-1.2.0 | BOTH | both | 56 | 53/2/0/1 | 1 | 15 | DeferredRegister, EventBusSubscriber, SubscribeEvent | ResourceLocation, Component, MutableComponent |
| 124 | dimdungeons | dimdungeons-210-neoforge-1.21.1 | BOTH | server/common | 60 | 52/0/5/3 | 0 | 21 | DeferredItem, DeferredBlock, Dist | ItemStack, BlockPos, Level |
| 125 | disconnect_packet_fix | disconnect-packet-fix-neoforge-2.0.1 | BOTH | server/common | 4 | 4/0/0/0 | 1 | 0 | Mod | - |
| 126 | draconicevolution | Draconic-Evolution-1.21.1-3.1.4.633 | BOTH | both | 682 | 385/234/17/46 | 5 | 46 | DeferredHolder, Dist, OnlyIn | Level, ItemStack, BlockPos |
| 127 | drippyloadingscreen | drippyloadingscreen_neoforge_3.1.5_MC_1.21.1 | BOTH | both | 66 | 37/18/0/11 | 18 | 0 | IEventBus, Mod, FMLConfig.ConfigValue | Component, Minecraft, GuiGraphics |
| 128 | dummmmmmy | dummmmmmy-neoforge-1.21-2.1.0 | BOTH | both | 45 | 29/13/0/3 | 6 | 3 | FinalizeSpawnEvent, NeoForge, Mod | ResourceLocation, Entity, ItemStack |
| 129 | durabilitytooltip | durabilitytooltip-1.2.0-neoforge-mc1.21 | BOTH | both | 5 | 4/1/0/0 | 0 | 1 | ModList, Mod | ChatFormatting, MutableComponent, Component |
| 130 | edivadlib | EdivadLib-1.21-3.0.0 | BOTH | server/common | 7 | 2/0/0/5 | 0 | 2 | FluidStack, FMLClientSetupEvent, NeoForge | MutableComponent, GuiGraphics, TextureAtlasSprite |
| 131 | elevatorid | elevatorid-neoforge-1.21.1-1.11.4 | BOTH | both | 26 | 12/10/0/4 | 0 | 7 | ModConfigSpec, ModConfigSpec.BooleanValue, ModConfigSpec.IntValue | BlockPos, Player, BlockState |
| 132 | emojiful | Emojiful-Neoforge-1.21-5.2.4-all | CLIENT | client (toml) | 179 +1 | 134/45/0/0 | 4 | 3 | - | - |
| 133 | enchdesc | enchdesc-neoforge-1.21.1-21.1.11 | BOTH | server/common | 7 | 6/0/0/1 | 2 | 0 | IEventBus, Mod | ItemStack, Component, Item |
| 134 | enderdrives | enderdrives-neoforge-1.21.1-1.5.23 | BOTH | both | 100 | 87/5/5/3 | 4 | 9 | ModConfigSpec, IPayloadContext, ModConfigSpec.IntValue | ItemStack, Item, Component |
| 135 | enderio | enderio-8.2.12-beta | BOTH | both | 1439 +3 | 1072/242/71/54 | 5 | 60 | DeferredHolder, FluidStack, ModConfigSpec | ItemStack, BlockPos, Level |
| 136 | enderstorage | EnderStorage-1.21.1-2.13.0.191 | BOTH | both | 74 | 49/16/5/4 | 0 | 17 | DeferredHolder, FluidStack, IEventBus | ItemStack, BlockPos, ServerPlayer |
| 137 | energymeter | energymeter-neoforge-1.21.1-0.5.2 | BOTH | both | 130 | 92/28/8/2 | 1 | 5 | DeferredHolder, IEnergyStorage, IPayloadContext | FriendlyByteBuf, Direction, BlockEntity |
| 138 | entangled | entangled-1.3.21-neoforge-mc1.21 | BOTH | both | 28 | 24/1/0/3 | 3 | 3 | BlockCapability, InterModEnqueueEvent, ICapabilityProvider | ResourceLocation, BlockPos, BlockState |
| 139 | entityculling | entityculling-neoforge-1.10.5-mc1.21.1 | BOTH | server/common | 32 | 19/0/0/13 | 9 | 0 | Dist, FMLEnvironment, Mod | Entity, AABB, EntityType |
| 140 | entityguardian | entityguardian-1.0 | BOTH | server/common | 2 | 2/0/0/0 | 1 | 2 | NeoForge, Mod, ServerStartingEvent | - |
| 141 | entityjs | entityjs-1.5.0 | BOTH | both | 415 | 324/38/0/53 | 14 | 10 | Dist, PartEntity, FMLEnvironment | Entity, LivingEntity, EntityType |
| 142 | euphoria_patcher | EuphoriaPatcher-1.10.0-r5.9-neoforge | BOTH | server/common | 1460 | 1456/0/0/4 | 21 | 0 | Mod, Dist, FMLEnvironment | Minecraft, ClientLevel, IntegratedServer |
| 143 | experiencelib | experiencelib-1.21.1-1.2.1 | BOTH | both | 87 | 81/3/0/3 | 0 | 1 | ItemCapability, INBTSerializable, BlockCapability | ResourceLocation, CompoundTag, Component |
| 144 | explorerscompass | explorerscompass-edited-1.21.1-3.0.4-neoforg | BOTH | both | 38 | 20/15/0/3 | 0 | 6 | ModConfigSpec, ModConfigSpec.BooleanValue, IPayloadContext | Player, ItemStack, BlockPos |
| 145 | extendedae | ExtendedAE-1.21-2.2.36-neoforge | BOTH | both | 483 +1 | 326/103/10/44 | 14 | 18 | Dist, OnlyIn, IItemHandler | ItemStack, ResourceLocation, MenuType |
| 146 | extrastorage | ExtraStorage-1.21.1-5.0.10 | BOTH | server/common | 107 | 85/0/12/10 | 0 | 7 | DeferredHolder, DeferredItem, IEventBus | Component, MutableComponent, ResourceLocation |
| 147 | extremesoundmuffler | ExtremeSoundMuffler-3.56_NeoForge-1.21 | BOTH | both | 22 | 8/2/0/12 | 3 | 6 | ModContainer, SubscribeEvent, EventBusSubscriber.Bus | Component, ResourceLocation, MutableComponent |
| 148 | fadingnightvision | fadingnightvision-1.3.0 | CLIENT | client (toml) | 132 +1 | 127/5/0/0 | 4 | 2 | - | - |
| 149 | fancymenu | fancymenu_neoforge_3.9.12_MC_1.21.1 | BOTH | both | 2449 +2 | 1545/114/0/790 | 102 | 10 | IEventBus, NeoForge, SubscribeEvent | Component, MutableComponent, Minecraft |
| 150 | farmersdelight | FarmersDelight-1.21.1-1.3.4 | BOTH | both | 292 | 217/32/28/15 | 14 | 24 | DeferredHolder, DeferredRegister, IItemHandler | ItemStack, BlockPos, Block |
| 151 | farmingforblockheads | farmingforblockheads-neoforge-1.21.1-21.1.13 | BOTH | both | 81 | 66/11/0/4 | 1 | 1 | Capabilities, Capabilities.ItemHandler, InvWrapper | ResourceLocation, ItemStack, BlockPos |
| 152 | fastitemframes | FastItemFrames-v21.1.6-1.21.1-NeoForge | BOTH | both | 20 | 14/5/1/0 | 4 | 0 | - | Item, TagKey, ItemStack |
| 153 | fdbosses | fdbosses-3.2-1.21.1 | BOTH | both | 448 | 262/128/0/58 | 10 | 46 | DeferredHolder, IPayloadContext, PacketDistributor | Vec3, Entity, Level |
| 154 | fdlib | fdlib-1.0.9-1.21.1 | BOTH | both | 535 | 361/37/6/131 | 13 | 35 | IPayloadContext, DeferredHolder, DeferredRegister | ResourceLocation, StreamCodec, FriendlyByteBuf |
| 155 | ferritecore | ferritecore-7.0.3-neoforge | BOTH | both | 77 | 68/1/0/8 | 20 | 1 | Mod, ModInfo, FMLLoader | Property, BlockState, VoxelShape |
| 156 | fluxnetworks | FluxNetworks-1.21.1-8.0.0 | BOTH | both | 222 | 109/78/31/4 | 0 | 20 | DeferredHolder, BlockCapability, RegisterEvent | ItemStack, BlockPos, BlockEntity |
| 157 | framedblocks | FramedBlocks-10.6.2 | BOTH | both | 1742 | 1264/332/70/76 | 12 | 46 | ModelData, FMLEnvironment, DeferredHolder | Direction, BlockState, Property |
| 158 | ftbauxilium | ftb-auxilium-neoforge-21.1.6 | BOTH | both | 20 | 11/1/0/8 | 1 | 3 | IEventBus, FMLPaths, FMLClientSetupEvent | Minecraft, LocalPlayer, ClientBrandRetriever |
| 159 | ftbbackups3 | ftb-backups-3-21.1.5 | BOTH | both | 38 | 29/9/0/0 | 0 | 14 | Event, NeoForge, IEventBus | ResourceLocation, MinecraftServer, CustomPacketPayload |
| 160 | ftbchunks | ftb-chunks-neoforge-2101.1.22 | BOTH | both | 238 | 105/132/0/1 | 12 | 6 | IEventBus, NeoForge, TicketController | ResourceLocation, Level, ResourceKey |
| 161 | ftbechoes | ftb-echoes-21.1.10 | BOTH | both | 109 | 60/39/8/2 | 0 | 13 | IPayloadContext, PacketDistributor, DeferredRegister | ResourceLocation, StreamCodec, Component |
| 162 | ftbessentials | ftb-essentials-neoforge-2101.1.10 | BOTH | server/common | 92 | 91/0/0/1 | 1 | 2 | EntityTeleportEvent.TeleportCommand, EntityTeleportEvent, NeoForge | ServerPlayer, Component, CommandSourceStack |
| 163 | ftbezcrystals | ftb-ez-crystals-21.1.1 | BOTH | server/common | 5 | 3/0/2/0 | 0 | 3 | DataMapType, RegisterDataMapTypesEvent, ModContainer | Block, ResourceLocation, Holder.Reference |
| 164 | ftbfiltersystem | ftb-filter-system-neoforge-21.1.4 | BOTH | both | 99 | 54/41/0/4 | 0 | 0 | Mod, Dist | ResourceLocation, ItemStack, HolderLookup.Provider |
| 165 | ftblibrary | ftb-library-neoforge-2101.1.35 | BOTH | both | 403 | 283/7/0/113 | 5 | 1 | Dist, OnlyIn, FluidStack | Component, MutableComponent, GuiGraphics |
| 166 | ftbmaterials | ftb-materials-21.1.4 | BOTH | server/common | 50 | 39/0/11/0 | 2 | 3 | DeferredHolder, DeferredRegister, DeferredRegister.Blocks | ResourceLocation, Item, Block |
| 167 | ftbobsidian | ftb-obsidian-21.1.0 | BOTH | both | 8 | 6/2/0/0 | 2 | 2 | Dist, FMLClientSetupEvent, AddPackFindersEvent | Pack, PackLocationInfo, Component |
| 168 | ftbpc | ftb-pack-companion-21.1.22 | BOTH | both | 140 | 111/7/9/13 | 16 | 18 | IEventBus, ModContainer, NeoForge | ResourceLocation, BlockPos, ResourceKey |
| 169 | ftbpmapi | ftb-pause-menu-api-21.1.3 | CLIENT | client (toml) | 25 | 0/23/2/0 | 1 | 2 | - | - |
| 170 | ftbpromoter | ftb-promoter-21.1.63 | CLIENT | client (toml) | 28 | 0/26/2/0 | 3 | 3 | - | - |
| 171 | ftbquests | ftb-quests-neoforge-2101.1.35 | BOTH | both | 476 | 308/154/0/14 | 2 | 5 | Dist, OnlyIn, IItemHandler | CustomPacketPayload, FriendlyByteBuf, ResourceLocation |
| 172 | ftbranks | ftb-ranks-neoforge-2101.1.4 | BOTH | server/common | 53 | 53/0/0/0 | 1 | 2 | NeoForge, PlayerEvent, Mod | ServerPlayer, MinecraftServer, Tag |
| 173 | ftbstuff | ftb-stuff-things-21.1.19 | BOTH | both | 302 | 247/22/17/16 | 2 | 13 | FluidStack, IItemHandler, DeferredHolder | ItemStack, BlockPos, BlockState |
| 174 | ftbteambases | ftb-team-bases-21.1.18 | BOTH | both | 122 | 113/9/0/0 | 9 | 15 | IPayloadContext, IEventBus, PacketDistributor | ResourceLocation, MinecraftServer, ResourceKey |
| 175 | ftbteams | ftb-teams-neoforge-2101.1.11 | BOTH | both | 134 | 95/38/0/1 | 0 | 0 | DeferredRegister, DeferredHolder, Mod | ResourceLocation, RegistryFriendlyByteBuf, ServerPlayer |
| 176 | ftbultimine | ftb-ultimine-neoforge-2101.1.15 | BOTH | both | 96 | 85/10/0/1 | 1 | 2 | EntityAttributeModificationEvent, Mod, IEventBus | BlockPos, Player, BlockState |
| 177 | ftbunearthed | ftb-unearthed-21.1.10 | BOTH | both | 86 | 68/5/11/2 | 1 | 13 | DeferredHolder, DeferredRegister, DeferredBlock | ItemStack, Level, Item |
| 178 | ftbxmodcompat | ftb-xmod-compat-neoforge-21.1.11 | BOTH | both | 102 | 90/1/0/11 | 2 | 1 | FluidStack, Mod | ItemStack, Player, ServerPlayer |
| 179 | ftboceanmobs | ftboceanmobs-21.1.4 | BOTH | both | 126 | 80/32/13/1 | 1 | 17 | DeferredHolder, DeferredRegister, DeferredBlock | Level, RandomSource, Entity |
| 180 | functionalstorage | functionalstorage-1.21.1-1.5.8 | BOTH | both | 179 | 112/25/23/19 | 0 | 6 | IItemHandler, DeferredHolder, FluidStack | ItemStack, BlockPos, Level |
| 181 | fusion | fusion-1.3.15a-neoforge-mc1.21.1 | BOTH | server/common | 471 | 288/0/4/179 | 40 | 1 | ModelData, RenderTypeGroup, ModelProperty | ResourceLocation, BlockState, Direction |
| 182 | fzzy_config | fzzy_config-0.7.6+1.21+neoforge | BOTH | both | 755 | 572/1/1/181 | 0 | 10 | ICommonPacketListener, NetworkRegistry, IPayloadContext | Component, ResourceLocation, MutableComponent |
| 183 | gag | gag-neoforge-1.21.1-5.2.0 | BOTH | both | 256 +1 | 211/16/14/15 | 5 | 39 | DeferredItem, SubscribeEvent, IPayloadContext | ResourceLocation, ItemStack, Level |
| 184 | gateways | GatewaysToEternity-1.21.1-5.1.0 | BOTH | both | 84 | 71/10/2/1 | 1 | 14 | IEventBus, FakePlayer, NeoForge | MutableComponent, Component, Item |
| 185 | gaze | gaze-1.1.7.1 | - | both | 112 | 74/16/0/22 | 0 | 14 | DeferredHolder, LivingDamageEvent, DeferredRegister | LivingEntity, Item, Level |
| 186 | geckolib | geckolib-neoforge-1.21.1-4.9.2 | BOTH | both | 262 | 204/7/0/51 | 7 | 1 | Event, ICancellableEvent, IEventBus | ResourceLocation, Entity, MultiBufferSource |
| 187 | glassential | Glassential-renewed-1.21.1-3.4.5 | BOTH | both | 77 | 58/6/9/4 | 2 | 6 | DeferredHolder, IPayloadContext, ModelData | BlockBehaviour.Properties, BlockBehaviour, BlockState |
| 188 | glodium | Glodium-1.21-2.2-neoforge | BOTH | both | 31 | 24/5/0/2 | 0 | 1 | IPayloadContext, ICapabilityProvider, RegisterCapabilitiesEvent | RegistryFriendlyByteBuf, StreamCodec, FriendlyByteBuf |
| 189 | guideme | guideme-21.1.17 | BOTH | both | 5102 | 5003/1/11/87 | 1 | 15 | FluidStack, IEventBus, NeoForge | ResourceLocation, ItemStack, Minecraft |
| 190 | hostilenetworks | HostileNeuralNetworks-1.21.1-6.5.1 | BOTH | both | 112 | 84/11/5/12 | 1 | 24 | IItemHandler, IEnergyStorage, ComponentItemHandler | ItemStack, ResourceLocation, Component |
| 191 | ibicf | ibicf-neo-7.0.1+mc1.21.1 | BOTH | server/common | 4 | 4/0/0/0 | 1 | 0 | Mod | - |
| 192 | iceberg | Iceberg-1.21.1-neoforge-1.3.2 | BOTH | both | 126 | 63/17/0/46 | 23 | 12 | SubscribeEvent, ModConfigSpec, IConfigSpec | ItemStack, RenderType, MultiBufferSource |
| 193 | idlecinematics | idlecinematics-neoforge-1.21.1-1.4.2 | CLIENT | client (toml) | 133 | 0/133/0/0 | 5 | 9 | - | - |
| 194 | immediatelyfast | ImmediatelyFast-NeoForge-1.6.13+1.21.1 | CLIENT | client (toml) | 75 | 0/75/0/0 | 37 | 0 | - | - |
| 195 | immersiveengineering | ImmersiveEngineering-1.21.1-12.4.2-194 | BOTH | both | 1902 +2 | 1457/376/2/67 | 46 | 91 | DeferredHolder, IItemHandler, FluidStack | ItemStack, Level, BlockPos |
| 196 | industrialforegoingsouls | industrial-foregoing-souls-1.21.1-1.10.7 | BOTH | both | 34 | 25/2/7/0 | 0 | 0 | DeferredHolder, BlockCapability, Dist | BlockPos, Level, ResourceLocation |
| 197 | industrialforegoing | industrialforegoing-1.21-3.6.39 | BOTH | both | 659 | 416/39/111/93 | 3 | 12 | FluidStack, DeferredHolder, OnlyIn | ItemStack, BlockPos, Level |
| 198 | inventoryessentials | inventoryessentials-neoforge-1.21.1-21.1.18 | BOTH | both | 52 | 34/15/0/3 | 6 | 0 | IEventBus, IItemHandler, SlotItemHandler | Slot, AbstractContainerMenu, Player |
| 199 | inventorysorter | inventorysorter-1.21.1-24.0.24 | - | server/common | 23 | 21/0/0/2 | 0 | 9 | ModConfigSpec, ModConfigSpec.ConfigValue, ModConfig.Type | Slot, ResourceLocation, AbstractContainerMenu |
| 200 | iris | iris-neoforge-1.8.14-beta.1+mc1.21.1 | CLIENT | client (toml) | 1476 +6 | 499/962/0/15 | 179 | 1 | Mod, IEventBus, ModelData | ResourceLocation, Direction, TextureAtlasSprite |
| 201 | ironchest | ironchest-1.21-neoforge-16.0.7 | BOTH | both | 89 | 67/12/7/3 | 0 | 7 | DeferredHolder, DeferredBlock, DeferredRegister | BlockState, BlockPos, BlockEntityType |
| 202 | ironfurnaces | ironfurnaces-neoforge-1.21.1-4.3.2 | BOTH | server/common | 165 | 155/0/0/10 | 0 | 6 | DeferredHolder, ModConfigSpec, Dist | BlockPos, Level, Item |
| 203 | irons_apothic | irons_apothic-2.2.1 | BOTH | server/common | 32 | 32/0/0/0 | 3 | 7 | AttributeTooltipContext, IEventBus, DeferredHolder | ItemStack, Registry, ResourceLocation |
| 204 | irons_apothic_invaders | irons_apothic_invaders_1.0.0 | - | dev | 0 | 0/0/0/0 | 0 | 0 | - | - |
| 205 | irons_jewelry | irons_jewelry-1.21.1-2.0.2 | BOTH | both | 152 | 117/5/4/26 | 4 | 23 | DeferredHolder, IEventBus, DeferredRegister | Holder, Component, ResourceLocation |
| 206 | irons_lib | irons_lib-1.21.1-2.1.0 | BOTH | both | 109 | 77/15/0/17 | 8 | 22 | DeferredHolder, DeferredRegister, IPayloadContext | ResourceLocation, ItemStack, BlockPos |
| 207 | irons_spellbooks | irons_spellbooks-1.21.1-3.16.3 | BOTH | both | 1187 | 894/67/13/213 | 33 | 99 | DeferredHolder, IEventBus, IPayloadContext | Entity, ResourceLocation, LivingEntity |
| 208 | irons_spellbooks_tweaks | irons_spellbooks_tweaks-1.5.0 | BOTH | server/common | 32 | 32/0/0/0 | 0 | 18 | SubscribeEvent, EventBusSubscriber, PlayerEvent | ResourceLocation, Player, Level |
| 209 | irregular_implements | irregular-implements-1.21.1-1.12.0-build.270 | BOTH | both | 1042 | 862/47/105/28 | 25 | 40 | DeferredHolder, DeferredItem, DeferredBlock | BlockPos, Level, BlockState |
| 210 | itemcollectors | itemcollectors-1.1.10-neoforge-mc1.21 | BOTH | server/common | 31 | 23/0/0/8 | 0 | 1 | Capabilities, BlockCapability, IItemHandler | BlockPos, BlockEntity, MutableComponent |
| 211 | jade | Jade-1.21.1-NeoForge-15.10.6 | BOTH | server/common | 301 | 218/0/0/83 | 6 | 19 | FluidStack, IItemHandler, ItemAbility | ResourceLocation, Component, MutableComponent |
| 212 | jags | jags-neo-21.1.7+mc1.21.1 | BOTH | both | 10 | 7/1/2/0 | 0 | 4 | - | Block, TagKey, BlockPos |
| 213 | jearchaeology | jearchaeology-1.21.1-1.2.0 | BOTH | server/common | 27 | 25/0/0/2 | 0 | 2 | DeferredHolder, EventBusSubscriber, ModConfigSpec | Ingredient, ResourceKey, LootTable |
| 214 | jei | jei-1.21.1-neoforge-19.51.0.418 | BOTH | server/common | 1039 | 777/0/0/262 | 0 | 1 | IKeyConflictContext, IEventBus, EventPriority | ItemStack, ResourceLocation, MethodsReturnNonnullByDefault |
| 215 | jumbofurnace | jumbofurnace-5.0.0.9 | BOTH | both | 40 | 32/8/0/0 | 0 | 6 | IItemHandler, ItemStackHandler, DeferredHolder | ItemStack, Level, Item |
| 216 | jumpoverfences | jumpoverfences-neoforge-1.21.1-1.6.1 | BOTH | server/common | 8 | 7/0/0/1 | 0 | 3 | ModConfigSpec, ModConfigSpec.ConfigValue, ModConfigSpec.Builder | LivingEntity, BuiltInRegistries, FenceBlock |
| 217 | justhammers | just-hammers-neoforge-21.1.4 | BOTH | both | 19 | 17/2/0/0 | 1 | 1 | IEventBus, Event, BlockDropsEvent | ItemStack, Item, Level |
| 218 | justdirefuels | justdirefuels-1.1.0 | BOTH | both | 14 | 11/3/0/0 | 5 | 1 | FluidStack, DataMapType, Capabilities | Fluid, ItemStack, Item |
| 219 | justdirethings | justdirethings-1.5.7 | BOTH | both | 619 | 422/135/28/34 | 3 | 47 | DeferredHolder, Capabilities, ItemCapability | ItemStack, Player, BlockPos |
| 220 | justdynathings | justdynathings-1.20.6-hotfix1 | BOTH | both | 314 | 256/2/14/42 | 1 | 6 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | BlockPos, BlockState, Block |
| 221 | justenoughbreeding | justenoughbreeding-neoforge-1.21.1-3.2.1 | BOTH | both | 78 | 55/2/0/21 | 0 | 1 | DeferredHolder, FMLPaths, ModList | EntityType, Ingredient, ResourceLocation |
| 222 | jei_mekanism_multiblocks | JustEnoughMekanismMultiblocks-1.21.1-7.20 | BOTH | both | 78 | 7/71/0/0 | 4 | 7 | ModConfigSpec, ModContainer, ModLoadingContext | Component, MutableComponent, ResourceLocation |
| 223 | justenoughprofessions | JustEnoughProfessions-neoforge-1.21.1-4.0.5 | BOTH | server/common | 12 | 7/0/0/5 | 0 | 1 | Dist, NeoForge, Mod | ResourceLocation, VillagerProfession, ItemStack |
| 224 | justzoom | justzoom_neoforge_2.1.0_MC_1.21.1 | BOTH | both | 24 | 11/5/0/8 | 5 | 1 | RegisterKeyMappingsEvent, Mod, IEventBus | KeyMapping, Minecraft, Component |
| 225 | kiwi | Kiwi-1.21.1-NeoForge-15.8.7 | - | both | 867 +1 | 784/42/5/36 | 39 | 32 | IEventBus, ModList, NeoForge | ResourceLocation, BlockState, Block |
| 226 | konkrete | konkrete_neoforge_1.9.9_MC_1.21 | BOTH | both | 411 | 359/12/0/40 | 12 | 0 | IEventBus, Mod, Dist | GuiGraphics, Minecraft, Font |
| 227 | kotlinforforge | kotlinforforge-5.12.0-all | BOTH | server/common | 4669 +11 | 4665/0/2/2 | 0 | 1 | Dist, FMLEnvironment, IEventBus | Vec3, Vec3i, Vec2 |
| 228 | kubeutils | kube-utils-21.1.3 | BOTH | server/common | 16 | 15/0/0/1 | 0 | 2 | PlayerEvent.PlayerChangedDimensionEvent, PlayerEvent, PlayerEvent.PlayerLoggedInEvent | ResourceLocation, CompoundTag, Player |
| 229 | kubejs_mekanism | kubejs-mekanism-neoforge-2101.1.7-build.18 | - | server/common | 16 | 16/0/0/0 | 0 | 0 | DataMapType, Mod, FluidType | ResourceLocation, Holder, ResourceKey |
| 230 | kubejs | kubejs-neoforge-2101.7.2-build.374 | BOTH | both | 1172 +3 | 1055/96/2/19 | 89 | 76 | FluidIngredient, FluidStack, IPayloadContext | ResourceLocation, ItemStack, ResourceKey |
| 231 | kubejs_actuallyadditions | kubejs_actuallyadditions-neoforge-1.21.1-0.3 | BOTH | server/common | 34 | 34/0/0/0 | 3 | 0 | FluidStack, Mod | ResourceLocation, Ingredient, ItemStack |
| 232 | kubejs_curios | kubejs_curios_neoforge_1.21.1-1.0.4 | BOTH | server/common | 33 | 29/0/0/4 | 6 | 7 | TriState, IEventBus, DeferredHolder | ItemStack, LivingEntity, Item |
| 233 | kubejs_enderio | kubejs_enderio-neoforge-1.21.1-0.13.0 | BOTH | server/common | 43 | 43/0/0/0 | 5 | 0 | DeferredHolder, SizedIngredient, ModList | ResourceLocation, ItemStack, TagKey |
| 234 | kubejspowah | kubejspowah-1.3.4 | BOTH | server/common | 7 | 7/0/0/0 | 0 | 0 | Mod | ResourceLocation, ItemStack, BuiltInRegistries |
| 235 | kubejstweaks | kubejstweaks-1.0.6 | BOTH | both | 120 +1 | 116/1/1/2 | 69 | 6 | ModList, IModFileInfo, FMLEnvironment | ResourceLocation, ResourceManager, Resource |
| 236 | cataclysm | L_Ender's Cataclysm 1.21.1-3.33 | BOTH | both | 1313 | 821/474/2/16 | 23 | 56 | DeferredHolder, ModConfigSpec, ModConfigSpec.Builder | LivingEntity, Entity, Level |
| 237 | laserio | laserio-1.9.11 | BOTH | both | 211 | 126/66/11/8 | 0 | 14 | DeferredHolder, IItemHandler, IPayloadContext | ItemStack, BlockPos, Player |
| 238 | leaderboards | leaderboards-1.21.1-NeoForge-2.0.4 | BOTH | both | 32 | 24/3/0/5 | 1 | 4 | PacketDistributor, RegisterCommandsEvent, PayloadRegistrar | Component, ResourceLocation, CustomPacketPayload |
| 239 | lionfishapi | lionfishapi-3.1 | BOTH | both | 51 | 15/35/0/1 | 2 | 4 | Event, IEventBus, IPayloadContext | Entity, LivingEntity, Level |
| 240 | lmft | lmft-1.1.1+1.21.9-neoforge | BOTH | server/common | 13 | 13/0/0/0 | 3 | 0 | Mod | Style, ChatFormatting, ResourceLocation |
| 241 | lodestone | lodestone-1.21.1-1.8.2 | BOTH | both | 419 | 254/30/31/104 | 15 | 26 | DeferredHolder, DeferredRegister, Dist | ResourceLocation, BlockPos, Level |
| 242 | lootjs | lootjs-neoforge-1.21.1-3.7.0 | BOTH | server/common | 168 | 168/0/0/0 | 20 | 1 | IGlobalLootModifier, ItemAbility, DeferredRegister | LootContext, ResourceLocation, ItemStack |
| 243 | lychee | Lychee-1.21.1-NeoForge-6.5.3 | BOTH | both | 552 +1 | 434/49/20/49 | 48 | 5 | NewRegistryEvent, Mod, IEventBus | StreamCodec, ResourceLocation, ItemStack |
| 244 | magic_coins | magic_coins-1.1.3 | BOTH | both | 48 | 40/6/0/2 | 0 | 10 | SubscribeEvent, EventBusSubscriber, IPayloadHandler | CustomPacketPayload, Player, ItemStack |
| 245 | magiccoinstweak | magiccoinstweak-1.0-SNAPSHOT | BOTH | server/common | 2 | 2/0/0/0 | 1 | 0 | Mod | Player |
| 246 | malum | malum-1.21.1-1.8.2 | BOTH | both | 1026 | 730/191/58/47 | 25 | 55 | DeferredHolder, Dist, OnlyIn | Level, ItemStack, BlockPos |
| 247 | mcjtylib | mcjtylib-1.21-9.0.21 | BOTH | both | 388 | 286/25/27/50 | 0 | 21 | BlockCapability, IPayloadContext, IItemHandler | ResourceLocation, ItemStack, Player |
| 248 | me_beam_former | me_beam_former-1.21.1-1.3.0 | BOTH | both | 37 | 31/5/0/1 | 0 | 6 | BlockCapability, DeferredHolder, Capabilities | BlockPos, Direction, Level |
| 249 | measurements | Measurements-neoforge-1.21.1-3.0.3 | BOTH | both | 28 | 18/7/2/1 | 0 | 8 | ModConfigSpec, IEventBus, DeferredRegister | ResourceKey, Registry, Player |
| 250 | megacells | megacells-4.11.0 | - | both | 86 | 75/10/0/1 | 8 | 14 | DeferredRegister, DeferredHolder, DeferredItem | ItemStack, Item, Item.Properties |
| 251 | mekagenjei | mekagenjei-1.2 | BOTH | both | 29 | 14/12/2/1 | 0 | 1 | DeferredHolder, IEventBus, ModContainer | Item, ResourceLocation, Block |
| 252 | mekanism | Mekanism-1.21.1-10.7.19.85 | BOTH | both | 2975 | 2206/711/25/33 | 0 | 92 | FluidStack, DeferredHolder, IPayloadContext | ItemStack, BlockPos, ResourceLocation |
| 253 | mekanism_lasers | mekanism_lasers-1.1.10.3-a | BOTH | both | 60 | 54/6/0/0 | 0 | 2 | ModConfigSpec, ModConfigSpec.LongValue, ModConfigSpec.IntValue | BlockPos, Block, BlockState |
| 254 | mekanism_ponders | mekanism_ponders-1.0.3-1.21.1 | BOTH | both | 1059 +2 | 336/582/2/139 | 49 | 27 | ModConfigSpec, ModConfigSpec.ConfigValue, ModConfig | Vec3, ResourceLocation, BlockPos |
| 255 | mekanism_unleashed | mekanism_unleashed-0.3.4 | BOTH | server/common | 15 | 14/0/0/1 | 10 | 1 | ModConfigSpec, Dist, ModConfig.Type | Screen, MutableComponent, Component |
| 256 | mekanismcurios | mekanismcurios-1.21.1-1.2.1 | BOTH | both | 20 | 17/2/0/1 | 11 | 4 | IPayloadContext, IEventBus, PayloadRegistrar | ItemStack, Player, InteractionHand |
| 257 | mekanismgenerators | MekanismGenerators-1.21.1-10.7.19.85 | - | both | 170 | 115/54/0/1 | 0 | 11 | FluidStack, FluidType, BlockCapabilityCache | BlockPos, BlockState, Holder |
| 258 | mekanisticrouters | mekanisticrouters-1.2.0 | BOTH | both | 23 | 14/6/3/0 | 1 | 5 | ModConfigSpec, ModConfigSpec.IntValue, DeferredHolder | ItemStack, Item, ResourceLocation |
| 259 | melody | melody_neoforge_1.0.10_MC_1.21 | BOTH | both | 22 | 14/3/0/5 | 3 | 0 | Mod, Dist, ModList | SoundSource, Minecraft, KeyMapping |
| 260 | merequester | merequester-neoforge-1.21.1-1.5.0 | BOTH | both | 75 | 51/17/0/7 | 3 | 6 | ModConfigSpec, ModConfigSpec.IntValue, INBTSerializable | Component, ItemStack, Player |
| 261 | metalbarrels | metalbarrels-neoforge-1.21.1-7 | BOTH | both | 41 | 29/3/9/0 | 0 | 5 | IItemHandler, ItemHandlerHelper, Capabilities | BlockState, BlockEntityType, BlockPos |
| 262 | mffs | mffs-5.4.44 | BOTH | both | 377 +1 | 303/3/13/58 | 4 | 22 | DeferredHolder, BlockCapability, ModConfigSpec | BlockPos, ItemStack, ResourceLocation |
| 263 | mecrh | mighty-ender-chicken-rehatched-21.1.5 | BOTH | both | 52 | 34/8/10/0 | 0 | 10 | DeferredHolder, DeferredRegister, PartEntity | Entity, Level, LivingEntity |
| 264 | mininggadgets | mininggadgets-1.18.7 | BOTH | both | 126 | 69/44/7/6 | 0 | 15 | DeferredHolder, IPayloadContext, ModConfigSpec.IntValue | ItemStack, Player, StreamCodec |
| 265 | mob_grinding_utils | mob_grinding_utils-1.1.10+mc1.21.1 | BOTH | both | 174 | 114/36/14/10 | 1 | 27 | DeferredHolder, Dist, OnlyIn | ItemStack, BlockPos, Level |
| 266 | mob_tag_fixes | mob_tag_fixes-1.0.0 | BOTH | server/common | 4 | 4/0/0/0 | 0 | 1 | ModConfigSpec, TagsUpdatedEvent, ModConfigSpec.BooleanValue | ResourceLocation, HolderSet, BuiltInRegistries |
| 267 | modernfix | modernfix-neoforge-5.27.24+mc1.21.1 | BOTH | server/common | 313 | 229/0/1/83 | 156 | 14 | ModContainer, ModList, IEventBus | ResourceLocation, ModelResourceLocation, BakedModel |
| 268 | modernworldcreation | modernworldcreation_neoforge_2.0.1_MC_1.21.1 | BOTH | both | 28 | 13/5/0/10 | 5 | 0 | IEventBus, Mod, Dist | Button.OnPress, CreateWorldScreen, MutableComponent |
| 269 | modonomicon | modonomicon-1.21.1-neoforge-1.120.4 | BOTH | both | 608 +3 | 376/105/102/25 | 0 | 22 | DeferredRegister, ModConfigSpec, IPayloadContext | ResourceLocation, RegistryFriendlyByteBuf, HolderLookup |
| 270 | modularrouters | modular-routers-13.2.7+mc1.21.1 | BOTH | both | 376 | 262/94/10/10 | 1 | 25 | ModConfigSpec, ModConfigSpec.IntValue, IItemHandler | ItemStack, Item, Level |
| 271 | moofluids | moofluids-1.21.1-NeoForge-1.3.6 | BOTH | both | 49 | 37/2/0/10 | 1 | 10 | DeferredHolder, FluidStack, DeferredRegister | Fluid, ResourceLocation, BuiltInRegistries |
| 272 | moonlight | moonlight-1.21.1-3.6.3-neoforge | BOTH | both | 967 +1 | 721/161/1/84 | 77 | 48 | ModConfigSpec, Dist, OnlyIn | ResourceLocation, BlockPos, ItemStack |
| 273 | more_immersive_wires | more-immersive-wires-1.21.1-1.1.9 | BOTH | server/common | 59 | 58/0/0/1 | 0 | 6 | DeferredHolder, ModConfigSpec, ModConfigSpec.Builder | BlockPos, BlockState, Level |
| 274 | morejs | morejs-neoforge-1.21.1-0.16.0 | BOTH | server/common | 109 | 107/0/0/2 | 26 | 4 | IEventBus, IBrewingRecipe, BrewingRecipe | ItemStack, VillagerTrades, RandomSource |
| 275 | moreoverlays | moreoverlays-1.24.2-mc1.21.1-neoforge | CLIENT | client (toml) | 42 | 0/42/0/0 | 1 | 13 | - | - |
| 276 | morered | morered-1.21.1-6.0.0.3 | BOTH | both | 106 | 79/25/0/2 | 2 | 18 | DeferredHolder, BlockCapability, PacketDistributor | BlockPos, BlockState, Direction |
| 277 | moreredxcctcompat | MoreRed-CCT-Compat-1.21.1-1.3.0 | BOTH | server/common | 4 | 4/0/0/0 | 0 | 2 | BlockCapability, ICapabilityProvider, Mod | Direction, BlockState, BlockPos |
| 278 | morphtool | Morph-o-Tool-1.8-39 | BOTH | both | 14 | 10/2/0/2 | 0 | 6 | ModConfigSpec, ModConfigSpec.BooleanValue, DeferredItem | ItemStack, Item, StreamCodec |
| 279 | mousetweaks | MouseTweaks-neoforge-mc1.21-2.26.1 | - | server/common | 22 | 16/0/0/6 | 1 | 4 | IConfigScreenFactory, ModContainer, Dist | Slot, ClickType, Screen |
| 280 | mru | mru-1.0.33+1.21.1-neoforge | BOTH | both | 34 | 24/7/0/3 | 0 | 2 | LoadingModList, ModInfo, IConfigurable | ItemStack, ResourceLocation, Player |
| 281 | nanny | NaNny-1.21.1-1.0.1 | SERVER | server/common | 2 | 2/0/0/0 | 0 | 7 | ModConfigSpec, ModConfigSpec.DoubleValue, ModConfigSpec.BooleanValue | LivingEntity, Component, DamageSource |
| 282 | nbtac | NBTac-NEOFORGE-1.21.1-2.0.1 | BOTH | server/common | 154 | 127/0/0/27 | 21 | 2 | IConfigScreenFactory, FMLLoadCompleteEvent, Dist | ResourceLocation, Item, Component |
| 283 | nerb | Not Enough Recipe Book-NEOFORGE-0.4.3+1.21 | BOTH | server/common | 18 | 16/0/0/2 | 6 | 0 | IEventBus, Mod | RecipeManager, ServerPlayer, ImageButton |
| 284 | occultengineering | occultengineering-1.21.1-0.13.1 | BOTH | both | 166 | 113/5/27/21 | 4 | 16 | DeferredHolder, IItemHandler, ItemStackHandler | ItemStack, ResourceLocation, BlockPos |
| 285 | occultism | occultism-1.21.1-neoforge-1.224.4 | BOTH | both | 950 | 589/152/161/48 | 0 | 44 | DeferredHolder, DeferredItem, ModConfigSpec | Level, ItemStack, BlockPos |
| 286 | occultism_kubejs | occultism_kubejs-1.21.1-neoforge-1.11.0 | - | server/common | 17 | 17/0/0/0 | 0 | 1 | ICondition, FMLClientSetupEvent, Dist | ResourceLocation, TagKey, Ingredient |
| 287 | octolib | OctoLib-NEOFORGE-0.6.2+1.21 | BOTH | both | 156 | 103/30/0/23 | 11 | 1 | Dist, OnlyIn, IEventBus | Vec3, Minecraft, CustomPacketPayload |
| 288 | omnitools | omnitools-1.2.2 | BOTH | both | 67 | 57/3/3/4 | 7 | 9 | DeferredItem, PlayerInteractEvent.RightClickBlock, PlayerInteractEvent | ItemStack, Player, Level |
| 289 | oracle_index | oracle_index-neoforge-1.3.1 | BOTH | both | 65 | 35/1/0/29 | 2 | 0 | Mod | Component, GuiGraphics, ResourceLocation |
| 290 | oritech | oritech-neoforge-1.21.1-1.2.12 | BOTH | both | 715 +1 | 512/114/6/83 | 6 | 11 | ModConfigSpec, ModConfigSpec.IntValue, ModConfigSpec.LongValue | BlockPos, BlockState, Level |
| 291 | oritechthings | oritechthings-0.0.46 | BOTH | both | 82 | 46/13/16/7 | 7 | 11 | DeferredHolder, DeferredBlock, DeferredRegister | BlockPos, Block, Level |
| 292 | overflowingbars | OverflowingBars-v21.1.1-1.21.1-NeoForge | BOTH | both | 21 | 9/12/0/0 | 0 | 2 | Mod | RangedAttribute, Holder, Attributes |
| 293 | particular | particular-1.21.1-NeoForge-1.5.7 | BOTH | both | 96 | 54/2/0/40 | 30 | 8 | ModConfigSpec, ModConfigSpec.BooleanValue, ModConfigSpec.DoubleValue | ParticleOptions, SimpleParticleType, ClientLevel |
| 294 | patchouli | Patchouli-1.21.1-93-NEOFORGE | BOTH | both | 198 | 63/123/4/8 | 10 | 17 | ModContainer, Event, ModConfigSpec.EnumValue | ResourceLocation, BlockPos, BlockState |
| 295 | petrock | petrock-neo-0.18.7 | BOTH | both | 91 | 44/30/15/2 | 0 | 7 | DeferredHolder, DeferredItem, DeferredRegister | Component, MutableComponent, CommandSourceStack |
| 296 | pickupnotifier | PickUpNotifier-v21.1.1-1.21.1-NeoForge | BOTH | both | 33 | 14/17/0/2 | 2 | 3 | ModConfigSpec.BooleanValue, ModConfigSpec, ModConfigSpec.ConfigValue | FriendlyByteBuf, Player, ItemStack |
| 297 | pipe_connector | pipe_connector-neoforge-0.5.28 | BOTH | both | 109 | 67/42/0/0 | 0 | 6 | ModConfigSpec, ModConfigSpec.IntValue, ModList | ItemStack, BlockPos, Player |
| 298 | pipegoggles | pipegoggles-2.0.4 | BOTH | both | 197 | 119/1/5/72 | 0 | 11 | DeferredHolder, DeferredItem, EventBusSubscriber | ResourceLocation, GuiGraphics, Screen |
| 299 | pipez | pipez-neoforge-1.21.1-1.2.31 | BOTH | both | 212 | 167/18/0/27 | 0 | 14 | DeferredHolder, BlockCapability, Capabilities | Direction, ItemStack, ResourceLocation |
| 300 | pipezretriever | pipezretriever-0.0.1 | BOTH | server/common | 15 | 14/0/0/1 | 9 | 1 | IPayloadContext, PayloadRegistrar, Mod | Direction, CustomPacketPayload, StreamCodec |
| 301 | placebo | Placebo-1.21.1-9.9.2 | BOTH | both | 164 | 140/4/9/11 | 6 | 24 | SubscribeEvent, IPayloadContext, IEventBus | ResourceLocation, ItemStack, StreamCodec |
| 302 | playeranimator | player-animation-lib-forge-2.0.4+1.21.1 | BOTH | both | 127 | 102/3/0/22 | 17 | 2 | LoadingModList, ModFileInfo | ResourceLocation, LivingEntity, ModelPart |
| 303 | pocketstorage | pocketstorage-1.2.5+1.21.1-b3 | BOTH | server/common | 32 | 21/0/7/4 | 0 | 10 | DeferredHolder, IItemHandler, ItemHandlerHelper | CompoundTag, ItemStack, Player |
| 304 | polylib | polylib-2100.1.0-build.183-neoforge | BOTH | both | 560 | 395/148/2/15 | 3 | 5 | Dist, IFluidHandler, IEventBus | ItemStack, Level, BlockPos |
| 305 | pop | pop-21.1.3 | BOTH | both | 16 | 8/7/0/1 | 0 | 6 | SubscribeEvent, PacketDistributor, IPayloadContext | Component, CustomPacketPayload, CommandBuildContext |
| 306 | powah | Powah-6.2.10 | BOTH | both | 351 | 246/76/9/20 | 0 | 15 | DataMapType, DeferredBlock, DeferredItem | ItemStack, BlockPos, Item |
| 307 | prickle | prickle-neoforge-1.21.1-21.1.11 | BOTH | server/common | 47 | 47/0/0/0 | 0 | 0 | IEventBus, Mod, FMLPaths | ResourceLocation, Ingredient, Style |
| 308 | productivemetalworks | productivemetalworks-1.21.1-1.15.1 | BOTH | both | 1210 +3 | 462/583/26/139 | 36 | 36 | FluidStack, DeferredHolder, ModConfigSpec | BlockPos, ResourceLocation, ItemStack |
| 309 | projecte | ProjectE-1.21.1-PE1.1.0 | BOTH | both | 721 | 685/5/2/29 | 0 | 37 | IItemHandler, IItemHandlerModifiable, ModConfigSpec | ItemStack, Player, Level |
| 310 | psi | Psi-neoforge-1.21.1-110 | BOTH | both | 652 +1 | 550/87/13/2 | 19 | 33 | IEventBus, RegisterCapabilitiesEvent, ModConfigSpec | Player, ItemStack, Entity |
| 311 | puzzleslib | PuzzlesLib-v21.1.60-mc1.21.1-NeoForge | BOTH | both | 981 | 597/290/83/11 | 20 | 42 | IEventBus, ModConfigSpec, ModConfigSpec.Builder | ResourceLocation, Entity, Holder |
| 312 | randombonemealflowers | randombonemealflowers-1.21.1-4.7 | BOTH | server/common | 21 | 21/0/0/0 | 0 | 2 | FMLLoadCompleteEvent, Mod, NeoForge | Level, MinecraftServer, class_1937 |
| 313 | rarcompat | rarcompat-1.21-0.9.7 | BOTH | both | 114 | 103/7/0/4 | 11 | 38 | EventBusSubscriber, SubscribeEvent, DeferredHolder | ItemStack, Player, LivingEntity |
| 314 | rechiseled | rechiseled-1.2.5-neoforge-mc1.21 | BOTH | server/common | 123 | 94/0/9/20 | 9 | 2 | TriPredicate, ModList, IModInfo | ResourceLocation, ItemLike, Item |
| 315 | rechiseled_chipped | rechiseled_chipped-2.0-1.21.1 | BOTH | server/common | 5 | 5/0/0/0 | 2 | 0 | IEventBus, ModContainer, Mod | Block, ResourceLocation, DefaultedRegistry |
| 316 | refinedtypes | refined-types-1.21.1-0.3.2 | BOTH | server/common | 172 +1 | 142/0/17/13 | 1 | 8 | ItemCapability, BlockCapability, FluidType | Component, BlockPos, MutableComponent |
| 317 | refineddelight | refineddelight-1.1.0 | BOTH | both | 30 | 28/2/0/0 | 0 | 3 | DeferredHolder, IEventBus, DeferredRegister | ItemStack, ResourceKey, Component |
| 318 | refinedsticks | refinedsticks-1.21.1-1.2.2 | BOTH | both | 10 | 8/2/0/0 | 0 | 6 | IPayloadHandler, RegisterPayloadHandlersEvent, EventBusSubscriber | ItemStack, BlockPos, ResourceKey |
| 319 | refinedstorage_curios_integration | refinedstorage-curios-integration-1.0.0 | - | server/common | 5 | 5/0/0/0 | 0 | 1 | Mod, IEventBus, FMLCommonSetupEvent | ItemStack, Player, LivingEntity |
| 320 | refinedstorage_jei_integration | refinedstorage-jei-integration-neoforge-1.0. | - | server/common | 33 | 21/0/0/12 | 0 | 1 | FluidStack, IEventBus, FMLCommonSetupEvent | AbstractContainerMenu, ItemStack, Rect2i |
| 321 | refinedstorage_mekanism_integration | refinedstorage-mekanism-integration-1.1.1 | - | server/common | 55 | 48/0/0/7 | 0 | 5 | ItemCapability, FluidType, RegisterMenuScreensEvent | ItemStack, Component, ResourceLocation |
| 322 | refinedstorage | refinedstorage-neoforge-2.0.9 | BOTH | server/common | 1508 | 1312/0/18/178 | 3 | 20 | ModConfigSpec, IItemHandler, ModConfigSpec.Builder | Component, ItemStack, ResourceLocation |
| 323 | refinedstorage_quartz_arsenal | refinedstorage-quartz-arsenal-neoforge-1.0.8 | - | server/common | 33 | 29/0/0/4 | 0 | 7 | ModConfigSpec, IEventBus, DeferredRegister | ItemStack, ResourceLocation, MenuType |
| 324 | relics | relics-1.21.1-0.10.7.6 | BOTH | both | 375 | 225/115/0/35 | 19 | 46 | DeferredHolder, EventBusSubscriber, SubscribeEvent | ItemStack, Player, Item |
| 325 | reliquified_ars_nouveau | reliquified_ars_nouveau-1.21.1-0.6.1 | BOTH | both | 71 | 52/9/0/10 | 10 | 21 | DeferredHolder, EventBusSubscriber, SubscribeEvent | ItemStack, Entity, Player |
| 326 | reliquified_lenders_cataclysm | reliquified_lenders_cataclysm-1.21.1-0.1.1 | BOTH | both | 26 | 22/2/0/2 | 1 | 7 | DeferredHolder, EventBusSubscriber, SubscribeEvent | Level, Entity, LivingEntity |
| 327 | reliquified_twilight_forest | reliquified_twilight_forest-1.21.1-0.5.2 | BOTH | both | 87 | 62/12/0/13 | 8 | 27 | DeferredHolder, EventBusSubscriber, SubscribeEvent | ItemStack, LivingEntity, Level |
| 328 | renderjs | renderjs-neoforge-2101.2.4 | BOTH | both | 35 | 7/16/0/12 | 3 | 10 | RenderLevelStageEvent, RenderLevelStageEvent.Stage, Environment | GuiGraphics, Minecraft, ItemStack |
| 329 | rep_ae2_bridge | rep_ae2_bridge-1.9.0.0.0-neoforge-1.21.1 | BOTH | both | 47 | 38/8/0/1 | 7 | 10 | DeferredHolder, IEventBus, DeferredBlock | Item, ResourceLocation, ItemStack |
| 330 | repeatable_trial_vaults | repeatable_trial_vaults-neoforge-1.21-1.0.3 | BOTH | server/common | 5 | 5/0/0/0 | 1 | 1 | NeoForge, Mod, ModContainer | Item, ItemStack, TagKey |
| 331 | replication | Replication-1.21.1-1.2.7 | BOTH | both | 207 | 101/78/17/11 | 0 | 0 | DeferredHolder, Dist, OnlyIn | ItemStack, CompoundTag, Level |
| 332 | replication_matter_overflow | replication_matter_overflow-1.21.1-NeoForge- | BOTH | both | 9 | 4/2/1/2 | 0 | 3 | DeferredHolder, IFluidHandler, DeferredBlock | BlockEntityType, CompoundTag, Direction |
| 333 | replication_rs2_bridge | replication_rs2_bridge-1.21.1-NeoForge-1.2.6 | BOTH | both | 50 | 41/7/0/2 | 5 | 8 | DeferredHolder, IEventBus, DeferredRegister | ItemStack, Level, ResourceLocation |
| 334 | resourcefulconfig | resourcefulconfig-neoforge-1.21-3.0.11 | BOTH | both | 186 | 108/77/0/1 | 4 | 2 | FMLLoader, ModContainer, FMLPaths | Component, Optionull, MutableComponent |
| 335 | resourcefullib | resourcefullib-neoforge-1.21-3.0.12 | BOTH | both | 222 | 171/46/1/4 | 1 | 5 | DeferredHolder, IEventBus, ModContainer | ResourceLocation, Tag, CompoundTag |
| 336 | respawningstructures | respawningstructures-1.21.1-4.4 | BOTH | server/common | 31 | 31/0/0/0 | 14 | 14 | SubscribeEvent, FMLClientSetupEvent, PlayerEvent | BlockPos, ServerLevel, ResourceKey |
| 337 | rftoolsbase | rftoolsbase-1.21-6.0.11 | BOTH | both | 182 | 152/13/7/10 | 0 | 10 | IItemHandler, Lazy, DeferredHolder | ItemStack, StreamCodec, Player |
| 338 | rftoolsbuilder | rftoolsbuilder-1.21-7.0.6 | BOTH | both | 222 | 165/28/5/24 | 0 | 12 | ModConfigSpec, ModConfigSpec.IntValue, DeferredHolder | BlockPos, Level, ItemStack |
| 339 | rftoolspower | rftoolspower-1.21-7.0.6 | BOTH | both | 104 | 76/16/7/5 | 0 | 5 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | BlockState, BlockPos, Level |
| 340 | rftoolsutility | rftoolsutility-1.21-7.0.12 | BOTH | both | 308 | 237/40/11/20 | 0 | 20 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | Level, BlockPos, Player |
| 341 | rgp_client | rgp-client-2.0.9+mc21.1-neoforge | BOTH | client | 173 | 0/173/0/0 | 7 | 1 | - | - |
| 342 | rhino | rhino-2101.2.8-build.91 | - | server/common | 390 | 390/0/0/0 | 0 | 0 | - | - |
| 343 | chicken_roost | roostultimate_1.21.1-curse-4.1.0 | BOTH | both | 202 | 167/8/0/27 | 0 | 11 | IItemHandler, DeferredBlock, ItemStackHandler | ItemStack, ResourceLocation, BlockPos |
| 344 | routers | routers-1.21.1-1.1.7 | BOTH | both | 67 | 42/6/7/12 | 2 | 5 | DeferredHolder, FluidStack, ModList | BlockPos, ItemStack, AbstractContainerMenu |
| 345 | rsinfinitybooster | rsinfinitybooster-neoforge-1.21.1-1.0.0.48 | BOTH | server/common | 11 | 9/0/2/0 | 1 | 2 | ModConfigSpec, ModConfigSpec.Builder, DeferredItem | Item, ItemStack, MutableComponent |
| 346 | rsrequestify | rsrequestify-1.21.1-3.0.0 | BOTH | both | 20 | 17/2/0/1 | 0 | 0 | DeferredHolder, BlockCapability, ModContainer | Component, Block, Level |
| 347 | schematicenergistics | schematicenergistics-1.21.1-1.5.4a | BOTH | both | 62 | 40/1/7/14 | 2 | 9 | IPayloadContext, PacketDistributor, DeferredHolder | BlockPos, CustomPacketPayload, ResourceLocation |
| 348 | searchables | Searchables-neoforge-1.21.1-1.0.2 | BOTH | server/common | 41 | 39/0/0/2 | 1 | 0 | IEventBus, Mod | MethodsReturnNonnullByDefault, FieldsAreNonnullByDefault, Component |
| 349 | servercore | servercore-neoforge-1.5.19+1.21.1 | BOTH | server/common | 692 | 691/0/0/1 | 60 | 5 | PermissionDynamicContext, PermissionNode, PermissionGatherEvent | Level, EntityType, ServerLevel |
| 350 | sg_economy | sg_economy-1.0.5 | BOTH | both | 28 | 22/6/0/0 | 0 | 14 | SubscribeEvent, EventBusSubscriber, PacketDistributor | CustomPacketPayload, Entity, StreamCodec |
| 351 | shield_api | shield_api-neoforge-2.2.0 | BOTH | both | 11 | 7/4/0/0 | 4 | 1 | Mod | Item, ItemStack, Ingredient |
| 352 | shrink | shrink-2.0.1.47-neoforge | BOTH | both | 22 | 17/3/0/2 | 0 | 1 | Dist, FMLPaths, Mod | ItemStack, Item, InteractionHand |
| 353 | sdrp | SimpleDiscordRichPresence-neoforge-88.0.1-bu | BOTH | server/common | 15 | 11/0/0/4 | 0 | 0 | FMLPaths, Dist, FMLEnvironment | Player, Level, I18n |
| 354 | simplemagnets | simplemagnets-1.1.12c-neoforge-mc1.21 | BOTH | both | 61 | 46/2/2/11 | 0 | 6 | EventBusSubscriber.Bus, EventBusSubscriber, SubscribeEvent | ItemStack, Player, BlockPos |
| 355 | simpleteleporters | SimpleTeleportersReforged-1.21.1-2.2.0 | BOTH | both | 34 | 21/2/7/4 | 0 | 5 | DeferredRegister, DeferredHolder, DeferredBlock | ResourceKey, BlockPos, Item |
| 356 | simpletomb | simpletomb-1.21.1-1.4.4 | BOTH | both | 38 | 19/1/0/18 | 0 | 14 | DeferredHolder, DeferredItem, ModConfigSpec | ClientLevel, Level, BlockPos |
| 357 | simplylight | simplylight-1.5.3+1.21.1-b4 | BOTH | server/common | 42 | 25/0/12/5 | 0 | 4 | DeferredRegister, IPayloadContext, DeferredRegister.Items | DyeColor, Block, BlockPos |
| 358 | simplyswords | simplyswords-neoforge-1.70.2-1.21.1 | BOTH | both | 1068 | 813/245/0/10 | 32 | 4 | Dist, OnlyIn, ModList | ItemStack, Level, LivingEntity |
| 359 | simplytooltips | SimplyTooltips-neoforge-0.1.5 | BOTH | both | 149 | 25/124/0/0 | 2 | 2 | IEventBus, Mod | Component, ItemStack, ResourceLocation |
| 360 | smartbrainlib | SmartBrainLib-neoforge-1.21.1-1.16.11 | BOTH | both | 130 | 129/1/0/0 | 0 | 2 | FluidType, IEventBus, DeferredHolder | LivingEntity, MemoryModuleType, ServerLevel |
| 361 | sodium | sodium-neoforge-0.8.13+mc1.21.1 | CLIENT | client (toml) | 871 +5 | 43/813/0/15 | 112 | 0 | IEventBus, Mod, ModelData | ResourceLocation, TextureAtlasSprite, Direction |
| 362 | solcarrot | solcarrot-1.21.1-1.16.6 | BOTH | both | 54 | 27/24/0/3 | 0 | 12 | EventBusSubscriber, SubscribeEvent, Dist | Player, MethodsReturnNonnullByDefault, ResourceLocation |
| 363 | sophisticatedbackpacks | sophisticatedbackpacks-1.21.1-3.26.2.2141 | BOTH | both | 385 | 319/37/11/18 | 1 | 48 | ModConfigSpec, DeferredHolder, IItemHandler | ItemStack, ResourceLocation, Player |
| 364 | sophisticatedcore | sophisticatedcore-1.21.1-1.5.1.2341 | BOTH | both | 831 | 679/82/8/62 | 4 | 31 | IItemHandler, IPayloadContext, ModConfigSpec | ItemStack, Player, ResourceLocation |
| 365 | sophisticateditemactions | sophisticateditemactions-1.21.1-0.5.16.423 | BOTH | both | 176 | 123/36/0/17 | 2 | 9 | IEventBus, IItemHandler, IPayloadContext | BlockPos, ItemStack, Vec3 |
| 366 | sophisticatedstorage | sophisticatedstorage-1.21.1-1.5.91.2127 | BOTH | both | 391 | 261/106/10/14 | 0 | 34 | IItemHandler, ModConfigSpec, IDataComponentHolderExtension | ItemStack, Item, Level |
| 367 | sophisticatedstoragecreateintegration | sophisticatedstoragecreateintegration-1.21.1 | BOTH | both | 49 | 36/8/0/5 | 0 | 4 | IEventBus, IPayloadContext, RegisterPayloadHandlersEvent | BlockPos, Level, CompoundTag |
| 368 | soulplied_energistics | soulplied_energistics-1.0.3 | BOTH | both | 13 | 9/4/0/0 | 0 | 3 | BlockCapability, IFluidHandler, IFluidHandler.FluidAction | BlockPos, Component, MutableComponent |
| 369 | spark | spark-1.10.124-neoforge | BOTH | both | 1624 | 1614/5/0/5 | 0 | 10 | SubscribeEvent, NeoForge, IEventBus | Entity, CommandSourceStack, MinecraftServer |
| 370 | squatgrow | squatgrow-neoforge-21.1.4+mc1.21.1 | BOTH | server/common | 27 | 26/0/0/1 | 1 | 0 | AttachmentType, Dist, NeoForgeRegistries | BlockState, BlockPos, Block |
| 371 | starbunclemania | starbunclemania-1.21.1-1.5.8 | - | both | 242 +1 | 178/17/31/16 | 17 | 22 | DeferredHolder, FluidStack, ModConfigSpec | Level, ItemStack, BlockPos |
| 372 | statuseffectbars | statuseffectbars-1.21.1-NeoForge-1.0.2 | BOTH | server/common | 13 | 6/0/0/7 | 5 | 0 | ModConfigSpec, ModConfigSpec.IntValue, ModConfigSpec.BooleanValue | MobEffectInstance, GuiGraphics, DeltaTracker |
| 373 | stepcrafter | stepcrafter-neoforge-1.21.1-0.1.9 | - | server/common | 224 | 181/0/15/28 | 14 | 7 | ModConfigSpec, ModConfigSpec.Builder, RegisterMenuScreensEvent | Component, ResourceLocation, AbstractContainerMenu |
| 374 | stonechest | stonechest-neoforge-1.21.1-1.3.0 | BOTH | both | 11 | 5/4/0/2 | 0 | 3 | DeferredHolder, IEventBus, DeferredRegister | BlockEntityType, BlockPos, BlockState |
| 375 | structureexpansion | structure-expansion-neoforge-88.0.1 | BOTH | server/common | 7 | 7/0/0/0 | 3 | 0 | Mod | CommandSourceStack, Commands, CommandBuildContext |
| 376 | structureessentials | structureessentials-1.21.1-5.0 | BOTH | server/common | 26 | 26/0/0/0 | 18 | 2 | ModifiableStructureInfo, ModifiableStructureInfo.StructureInfo, RegisterCommandsEvent | Structure, ResourceLocation, Holder |
| 377 | structures_tweaker | structures_tweaker-1.21.1-NeoForge-2.1.7 | BOTH | both | 36 | 33/3/0/0 | 5 | 20 | SubscribeEvent, ModList, ServerStartedEvent | ResourceLocation, BlockPos, ServerLevel |
| 378 | structurify | structurify-neoforge-2.0.35+mc1.21.1 | BOTH | both | 197 | 152/29/0/16 | 35 | 6 | ModContainer, RegisterPayloadHandlersEvent, IEventBus | ResourceLocation, Component, Heightmap |
| 379 | sfm | Super Factory Manager (SFM)-MC1.21.1-4.34.0 | BOTH | both | 821 | 678/123/2/18 | 1 | 0 | ModConfigSpec, IEventBus, ModConfigSpec.ConfigValue | BlockPos, Level, ItemStack |
| 380 | supermartijn642configlib | supermartijn642configlib-1.1.8-neoforge-mc1. | BOTH | server/common | 44 | 43/0/0/1 | 0 | 5 | IEventBus, NeoForge, ICustomConfigurationTask | CustomPacketPayload, ConfigurationTask.Type, ConfigurationTask |
| 381 | supermartijn642corelib | supermartijn642corelib-1.1.24-neoforge-mc1.2 | BOTH | server/common | 205 | 157/0/12/36 | 15 | 11 | ICondition, IEventBus, ICondition.IContext | ResourceLocation, ItemStack, Item |
| 382 | supplementaries | supplementaries-1.21.1-3.9.8-neoforge | BOTH | both | 1242 +2 | 913/245/0/84 | 112 | 39 | Dist, OnlyIn, IEventBus | Level, BlockPos, ItemStack |
| 383 | synesthesia | synesthesia-1.21.1-NeoForge-1.1.0 | CLIENT | client (toml) | 13 | 0/13/0/0 | 3 | 3 | - | - |
| 384 | tempad | tempad-1.21.1-3.0.4-all | BOTH | both | 639 +3 | 453/169/10/7 | 2 | 27 | MutableDataComponentHolder, AttachmentHolder, AttachmentType | ResourceLocation, Player, ItemStack |
| 385 | tesseract | tesseract-1.0.38-neoforge-mc1.21 | BOTH | server/common | 52 | 43/0/0/9 | 0 | 6 | IEventBus, IEnergyStorage, IFluidHandler | BlockPos, Component, ResourceLocation |
| 386 | tipsmod | tipsmod-neoforge-1.21.1-21.1.3 | BOTH | both | 19 | 5/9/0/5 | 3 | 0 | Mod | ResourceLocation, Screen, Component |
| 387 | titanium | titanium-1.21-4.0.50 | BOTH | both | 315 | 225/52/28/10 | 0 | 4 | Dist, OnlyIn, DeferredHolder | CompoundTag, ItemStack, ResourceLocation |
| 388 | toofast | toofast-1.21.0-0.4.3.6 | BOTH | server/common | 2 | 2/0/0/0 | 1 | 0 | Mod | - |
| 389 | toolbelt | ToolBelt-1.21.1-2.2.10 | BOTH | both | 76 | 47/22/4/3 | 0 | 24 | DeferredHolder, ItemCapability, IItemHandler | ItemStack, Player, ResourceLocation |
| 390 | toolkit | ToolKit-neoforge-87.0.3 | BOTH | server/common | 38 | 37/0/0/1 | 0 | 0 | DeferredHolder, Mod, IEventBus | CommandSourceStack, Commands, ServerPlayer |
| 391 | tooltipoverhaul | tooltipoverhaul-neoforge-1.21.1-1.5.1 | BOTH | both | 163 | 18/124/0/21 | 13 | 5 | RegisterClientReloadListenersEvent, IEventBus, FMLPaths | ItemStack, Screen, GuiGraphics |
| 392 | torchmaster | torchmaster-neoforge-1.21.1-21.1.11 | BOTH | both | 71 | 60/9/0/2 | 2 | 9 | ModConfigSpec, DeferredHolder, DeferredRegister | BlockPos, Level, ResourceLocation |
| 393 | trade_cycling | trade-cycling-neoforge-1.21.1-1.0.18 | BOTH | server/common | 59 | 56/0/0/3 | 3 | 6 | ModConfigSpec, ModConfigSpec.Builder, IEventBus | Merchant, ServerPlayer, MerchantContainer |
| 394 | transfer_labels | transfer_labels-0.1.8 | BOTH | both | 50 | 23/14/4/9 | 0 | 7 | INBTSerializable, Dist, OnlyIn | CompoundTag, HolderLookup.Provider, HolderLookup |
| 395 | trashcans | trashcans-1.1.0-neoforge-mc1.21 | BOTH | server/common | 54 | 42/0/0/12 | 0 | 2 | IFluidHandler, FluidStack, ItemCapability | ItemStack, BlockPos, ResourceLocation |
| 396 | trashslot | trashslot-neoforge-1.21.1-21.1.11 | BOTH | both | 47 | 23/18/0/6 | 0 | 0 | IEventBus, Mod | Player, ItemStack, ResourceLocation |
| 397 | trenzalore | trenzalore-neo-6.1.1+mc1.21.1 | BOTH | server/common | 15 | 15/0/0/0 | 0 | 1 | IGlobalLootModifier, IEventBus, LootModifier | ResourceKey, CreativeModeTab, Item |
| 398 | trophymanager | trophymanager-1.21.1-3.0.0 | BOTH | both | 39 | 27/6/2/4 | 0 | 10 | DeferredHolder, ModConfigSpec, ModConfigSpec.DoubleValue | Block, ItemStack, CompoundTag |
| 399 | twilightforest | twilightforest-1.21.1-4.8.3345-universal | BOTH | both | 1976 +1 | 1470/330/106/70 | 0 | 94 | DeferredHolder, DeferredBlock, DeferredItem | BlockPos, BlockState, RandomSource |
| 400 | underlay | underlay-1.0.4-hotfix.1-neoforge-mc1.21.1 | - | both | 34 | 23/7/0/4 | 11 | 15 | SubscribeEvent, ModelData, IEventBus | BlockPos, ServerLevel, BlockState |
| 401 | universalgrid | universalgrid-neoforge-1.21.1-0.3.2 | - | server/common | 62 | 49/0/0/13 | 10 | 7 | ModConfigSpec, IEventBus, FMLClientSetupEvent | ItemStack, Player, ResourceLocation |
| 402 | utilitarian | utilitarian-1.21.1-0.19.2 | BOTH | both | 70 | 53/5/11/1 | 5 | 23 | DeferredHolder, ModConfigSpec, ModConfigSpec.IntValue | BlockState, BlockPos, Block |
| 403 | villagerconfig | villagerconfig-neoforge-4.5.4+1.21.1 | BOTH | both | 188 | 183/1/2/2 | 15 | 4 | ModList, RegisterCommandsEvent, PayloadRegistrar | ResourceLocation, Level, ServerLevel |
| 404 | watut | watut-neoforge-1.21.0-1.2.7 | BOTH | both | 70 | 27/24/0/19 | 18 | 9 | RegisterClientCommandsEvent, InputEvent.Key, ClientTickEvent | Player, ResourceLocation, Level |
| 405 | wits | wits-neoforge-1.3.1 | BOTH | server/common | 3 | 3/0/0/0 | 0 | 1 | RegisterCommandsEvent, NeoForge, Mod | RegistryAccess, Structure, ServerLevel |
| 406 | woodenshears | woodenshears-neoforge-1.21-3.2.3.0 | BOTH | server/common | 11 | 11/0/0/0 | 1 | 2 | ModLoadingContext, ModConfig.Type, IConfigSpec | Item, DataComponentType, DataComponents |
| 407 | xnet | xnet-1.21-7.0.7 | BOTH | both | 186 | 143/32/7/4 | 0 | 11 | ModConfigSpec, ModConfigSpec.IntValue, DeferredBlock | BlockPos, Level, BlockState |
| 408 | xnetgases | xnetgases-1.21.1-6.0.2 | BOTH | server/common | 18 | 18/0/0/0 | 0 | 1 | NeoForgeStreamCodecs, ModConfigSpec.IntValue, ModConfigSpec | StreamCodec, Direction, BlockPos |
| 409 | xp_synthesiser | xp_synthesiser-1.0.3 | BOTH | both | 14 | 11/1/0/2 | 0 | 4 | ItemStackHandler, DeferredHolder, ModConfigSpec | ItemStack, Item, Level |
| 410 | xycraft_core | xycraft_core-0.7.53-all | BOTH | both | 1108 +2 | 1046/21/16/25 | 8 | 30 | DeferredHolder, BlockCapability, AttachmentType | BlockPos, BlockState, ItemStack |
| 411 | xycraft_machines | xycraft_machines-0.7.53 | BOTH | both | 505 | 399/41/27/38 | 4 | 44 | AttachmentType, FluidStack, DeferredHolder | BlockPos, ItemStack, BlockState |
| 412 | xycraft_world | xycraft_world-0.7.53 | BOTH | both | 50 | 41/1/8/0 | 0 | 5 | DeferredHolder, DeferredRegister, IBlockCapabilityProvider | BlockPos, BlockState, BlockBehaviour.Properties |
| 413 | yeetusexperimentus | yeetusexperimentus-neoforge-87.0.0 | BOTH | server/common | 6 | 6/0/0/0 | 3 | 0 | Mod | - |
| 414 | yet_another_config_lib_v3 | yet_another_config_lib_v3-3.8.2+1.21.1-neofo | - | server/common | 761 +8 | 691/0/0/70 | 7 | 1 | Dist, Mod, RegisterClientReloadListenersEvent | Component, MutableComponent, GuiGraphics |

Nested Jar-in-Jar jars: 106 in 48 mods, 93 distinct by SHA-1, 92 distinct file names. The 25 most shared (host mods, classes, own `mods.toml`):

| Nested jar | Host mods | Classes | mods.toml |
|:--|--:|--:|:--|
| `sable-companion-common-1.21.1-1.6.0.jar` | 5 | 14 | yes |
| `fabric-api-base-0.4.42+d1308ded19.jar` | 3 | 17 | yes |
| `GrandPower-3.0.2.jar` | 2 | 8 | yes |
| `animated-gif-lib-for-java-animated-gif-lib-1.7.jar` | 2 | 5 | no |
| `conditional-mixin-neoforge-0.6.4.jar` | 2 | 19 | yes |
| `fabric-block-view-api-v2-1.0.10+9afaaf8c19.jar` | 2 | 12 | yes |
| `fabric-rendering-data-attachment-v1-0.3.48+73761d2e19.jar` | 2 | 8 | yes |
| `flywheel-neoforge-1.21.1-1.0.6.jar` | 2 | 555 | yes |
| `mixinsquared-neoforge-0.3.3.jar` | 2 | 3 | no |
| `ponder-neoforge-1.0.82+mc1.21.1.jar` | 2 | 470 | yes |
| `BlockModelSplitter-2.0.1.jar` | 1 | 19 | no |
| `DualCodecs-0.1.2.jar` | 1 | 42 | no |
| `GraphLib3-3.0.5.jar` | 1 | 9 | no |
| `Placebo-1.21.1-9.9.0.jar` | 1 | 158 | yes |
| `Primitive Collections-0.8.9.jar` | 1 | 526 | no |
| `Quack-0.4.10.115.jar` | 1 | 284 | no |
| `Registrate-MC1.21-1.3.0+67.jar` | 1 | 88 | no |
| `ae2addonlib-1.0.3-1.21.1.jar` | 1 | 92 | yes |
| `ae2wtlib_api-19.2.0.jar` | 1 | 38 | yes |
| `ae2wtlib_api-19.2.5.jar` | 1 | 39 | yes |
| `app.jar` | 1 | 757 | no |
| `bcprov-jdk18on-1.78.1.jar` | 1 | 4245 | no |
| `better-advanced-tooltips-2101.1.0-build.1.jar` | 1 | 9 | yes |
| `cobalt-0.9.9.jar` | 1 | 186 | no |
| `codecui-neoforge-1.21.1-1.3.6.jar` | 1 | 115 | yes |

Mods by class side: both 279, server/common 116, client (toml) 17, dev 1, client 1.

## 3. NeoForge classes by number of mods

Distribution: how many NeoForge classes have the given number of server-side mods.

| Mods (srv) | NeoForge classes | Of them moved or GONE in 26.3 |
|:--|--:|--:|
| 1 | 129 | 16 |
| 2 | 68 | 7 |
| 3-4 | 79 | 15 |
| 5-9 | 116 | 15 |
| 10-24 | 107 | 14 |
| 25-49 | 68 | 8 |
| 50-99 | 43 | 11 |
| 100-199 | 26 | 2 |
| 200-414 | 8 | 0 |
| 0 (client or dev only) | 303 | 118 |

Classes used by 8 or more mods (281 classes). `net.neoforged.` is dropped from the names.

| Class | Mods (srv) | Mods (all) | Refs | S/C/D/M | 26.3 | Members n/u |
|:--|--:|--:|--:|--:|:--|:--|
| `bus.api.IEventBus` | 342 | 376 | 1612 | 1362/264/114/250 | same | 10/0 |
| `fml.common.Mod` | 340 | 411 | 397 | 329/91/63/68 | same | - |
| `fml.ModContainer` | 246 | 285 | 438 | 328/73/56/110 | same | 11/0 |
| `neoforge.registries.DeferredHolder` | 224 | 230 | 5678 | 5178/450/535/500 | same | 25/3 |
| `neoforge.common.NeoForge` | 219 | 248 | 640 | 489/181/36/151 | same | 1/0 |
| `neoforge.registries.DeferredRegister` | 207 | 211 | 1237 | 1172/28/140/65 | same | 17/3 |
| `bus.api.SubscribeEvent` | 207 | 271 | 1013 | 878/494/112/135 | same | 2/0 |
| `fml.ModList` | 200 | 224 | 459 | 366/66/38/93 | same | 13/0 |
| `api.distmarker.Dist` | 199 | 326 | 1319 | 734/1146/60/585 | same | 8/0 |
| `neoforge.network.handling.IPayloadContext` | 183 | 184 | 1128 | 1026/8/7/102 | same | 9/0 |
| `neoforge.common.ModConfigSpec` | 182 | 195 | 1992 | 1726/236/39/266 | same | 10/0 |
| `neoforge.common.ModConfigSpec$Builder` | 178 | 191 | 686 | 677/33/16/9 | same | 35/0 |
| `neoforge.network.event.RegisterPayloadHandlersEvent` | 170 | 176 | 237 | 203/3/20/34 | same | 1/0 |
| `neoforge.network.registration.PayloadRegistrar` | 167 | 171 | 200 | 178/1/8/22 | same | 11/0 |
| `neoforge.network.handling.IPayloadHandler` | 166 | 170 | 219 | 200/1/6/19 | same | 1/0 |
| `fml.config.ModConfig` | 165 | 191 | 306 | 237/30/16/69 | same | 6/0 |
| `fml.config.ModConfig$Type` | 165 | 191 | 278 | 211/27/16/67 | same | 10/2 |
| `fml.config.IConfigSpec` | 162 | 188 | 218 | 169/25/14/49 | same | 1/0 |
| `neoforge.network.PacketDistributor` | 157 | 163 | 583 | 462/230/3/121 | same | 8/1 |
| `neoforge.capabilities.BlockCapability` | 153 | 158 | 1025 | 962/10/25/63 | same | 6/4 |
| `fml.event.lifecycle.FMLCommonSetupEvent` | 148 | 168 | 194 | 154/5/62/40 | same | 2/0 |
| `neoforge.items.IItemHandler` | 139 | 140 | 1514 | 1446/25/21/68 | GONE | 8/6 |
| `neoforge.capabilities.Capabilities` | 138 | 141 | 827 | 752/25/24/75 | same | - |
| `neoforge.common.ModConfigSpec$BooleanValue` | 138 | 152 | 617 | 532/123/7/85 | same | 8/0 |
| `fml.loading.FMLEnvironment` | 132 | 146 | 296 | 174/13/18/122 | same | 2/2 |
| `neoforge.capabilities.RegisterCapabilitiesEvent` | 131 | 145 | 455 | 402/0/25/53 | same | 7/0 |
| `fml.common.EventBusSubscriber` | 126 | 209 | 662 | 606/407/91/56 | same | - |
| `neoforge.common.ModConfigSpec$IntValue` | 119 | 123 | 879 | 806/76/6/73 | same | 5/0 |
| `neoforge.capabilities.ICapabilityProvider` | 119 | 131 | 370 | 335/0/15/35 | same | 1/0 |
| `neoforge.capabilities.Capabilities$ItemHandler` | 112 | 116 | 434 | 405/9/13/29 | GONE | 4/4 |
| `neoforge.registries.NeoForgeRegistries` | 111 | 116 | 202 | 192/1/37/10 | same | 8/0 |
| `neoforge.fluids.FluidStack` | 108 | 110 | 1440 | 1232/178/73/208 | same | 58/13 |
| `neoforge.registries.DeferredItem` | 104 | 105 | 728 | 646/63/333/82 | same | 8/2 |
| `neoforge.common.ModConfigSpec$ConfigValue` | 104 | 112 | 469 | 385/56/5/84 | same | 7/0 |
| `bus.api.EventPriority` | 99 | 122 | 172 | 140/55/14/32 | same | 7/0 |
| `neoforge.event.entity.player.PlayerEvent` | 96 | 100 | 167 | 152/6/6/15 | same | 1/0 |
| `bus.api.Event` | 93 | 104 | 404 | 362/80/1/42 | same | 1/0 |
| `neoforge.event.RegisterCommandsEvent` | 93 | 98 | 110 | 93/1/7/17 | same | 3/0 |
| `neoforge.fluids.capability.IFluidHandler` | 89 | 89 | 649 | 607/17/17/42 | GONE | 7/7 |
| `neoforge.capabilities.ItemCapability` | 88 | 89 | 484 | 429/30/12/55 | same | 2/1 |
| `neoforge.registries.DeferredRegister$Items` | 86 | 90 | 200 | 184/6/26/16 | same | 12/4 |
| `fml.event.config.ModConfigEvent` | 86 | 92 | 109 | 96/8/2/13 | same | 1/0 |
| `neoforge.event.entity.player.PlayerInteractEvent` | 85 | 89 | 160 | 140/22/5/20 | same | 5/0 |
| `api.distmarker.OnlyIn` | 84 | 111 | 949 | 502/665/12/447 | same | - |
| `neoforge.network.IContainerFactory` | 84 | 93 | 101 | 88/0/12/13 | same | 1/0 |
| `neoforgespi.language.IModInfo` | 81 | 92 | 133 | 93/17/18/40 | same | 12/0 |
| `fml.event.lifecycle.FMLClientSetupEvent` | 80 | 181 | 122 | 40/130/49/82 | same | 1/0 |
| `neoforge.registries.DeferredBlock` | 78 | 82 | 668 | 595/99/262/73 | same | 7/2 |
| `neoforge.capabilities.Capabilities$FluidHandler` | 78 | 80 | 302 | 269/11/11/33 | GONE | 3/3 |
| `neoforge.fluids.capability.IFluidHandler$FluidAction` | 77 | 77 | 398 | 379/7/13/19 | GONE | 6/6 |
| `neoforge.common.Tags` | 77 | 130 | 255 | 222/8/321/33 | same | 1/0 |
| `neoforge.registries.NeoForgeRegistries$Keys` | 77 | 83 | 119 | 113/1/31/6 | same | 12/0 |
| `fml.loading.FMLPaths` | 77 | 89 | 100 | 84/12/4/16 | same | 5/0 |
| `neoforge.energy.IEnergyStorage` | 75 | 75 | 465 | 425/23/5/40 | GONE | 8/6 |
| `neoforge.fluids.FluidType` | 75 | 78 | 398 | 318/47/12/80 | same | 37/8 |
| `neoforge.event.BuildCreativeModeTabContentsEvent` | 75 | 89 | 103 | 87/3/19/16 | same | 17/0 |
| `neoforge.capabilities.Capabilities$EnergyStorage` | 73 | 74 | 267 | 232/5/7/35 | GONE | 3/3 |
| `neoforge.common.extensions.IMenuTypeExtension` | 70 | 79 | 70 | 60/0/11/10 | same | 1/0 |
| `neoforge.items.ItemStackHandler` | 69 | 69 | 504 | 472/10/4/32 | GONE | 17/17 |
| `neoforge.items.ItemHandlerHelper` | 69 | 69 | 247 | 233/0/4/14 | GONE | 5/5 |
| `fml.common.EventBusSubscriber$Bus` | 69 | 128 | 189 | 177/166/47/12 | GONE | 2/2 |
| `neoforge.common.ModConfigSpec$DoubleValue` | 68 | 71 | 304 | 287/25/2/17 | same | 2/0 |
| `neoforge.attachment.AttachmentType` | 68 | 68 | 262 | 255/8/21/7 | same | 4/0 |
| `neoforge.registries.DeferredRegister$Blocks` | 67 | 67 | 130 | 115/1/44/15 | same | 10/3 |
| `neoforge.event.entity.player.PlayerInteractEvent$RightClickBlock` | 67 | 69 | 106 | 92/11/2/14 | same | 17/4 |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedInEvent` | 66 | 72 | 84 | 75/5/5/9 | same | 1/0 |
| `neoforge.items.IItemHandlerModifiable` | 65 | 65 | 341 | 328/6/1/13 | GONE | 7/7 |
| `neoforge.event.level.BlockEvent` | 65 | 66 | 141 | 128/0/1/13 | same | 1/0 |
| `fml.loading.FMLLoader` | 64 | 73 | 111 | 95/13/6/16 | same | 7/2 |
| `neoforge.common.util.FakePlayer` | 63 | 64 | 192 | 184/1/4/8 | same | 45/4 |
| `neoforge.server.ServerLifecycleHooks` | 59 | 60 | 159 | 136/1/2/23 | same | 1/0 |
| `neoforge.attachment.AttachmentType$Builder` | 57 | 61 | 125 | 124/3/20/1 | same | 8/2 |
| `neoforge.items.SlotItemHandler` | 55 | 55 | 186 | 176/2/0/10 | GONE | 15/15 |
| `neoforge.event.level.BlockEvent$BreakEvent` | 52 | 52 | 97 | 87/0/0/10 | GONE | 7/7 |
| `neoforge.event.tick.ServerTickEvent` | 52 | 54 | 74 | 59/1/1/15 | same | 1/0 |
| `neoforge.event.EventHooks` | 51 | 52 | 198 | 181/0/2/17 | same | 37/14 |
| `neoforge.common.Tags$Items` | 51 | 111 | 138 | 123/6/243/15 | same | 113/2 |
| `neoforge.event.entity.EntityJoinLevelEvent` | 49 | 53 | 73 | 67/10/2/6 | same | 5/0 |
| `fml.event.config.ModConfigEvent$Reloading` | 48 | 54 | 66 | 56/5/2/10 | same | 2/0 |
| `neoforge.registries.DeferredRegister$DataComponents` | 47 | 50 | 66 | 62/0/27/4 | same | 5/2 |
| `neoforge.registries.RegistryBuilder` | 47 | 47 | 61 | 60/1/1/1 | same | 8/1 |
| `neoforge.registries.RegisterEvent` | 46 | 55 | 84 | 73/1/17/11 | same | 5/1 |
| `neoforge.event.entity.living.LivingDeathEvent` | 46 | 47 | 77 | 69/1/1/8 | same | 4/0 |
| `neoforge.common.ItemAbility` | 45 | 47 | 153 | 139/0/5/14 | same | 4/0 |
| `neoforge.fluids.capability.IFluidHandlerItem` | 45 | 46 | 148 | 133/9/5/15 | GONE | 9/8 |
| `fml.ModLoadingContext` | 45 | 49 | 70 | 53/4/6/17 | same | 5/0 |
| `neoforge.capabilities.EntityCapability` | 44 | 44 | 130 | 121/1/0/9 | same | 3/3 |
| `fml.event.config.ModConfigEvent$Loading` | 43 | 49 | 59 | 54/4/2/5 | same | 2/0 |
| `neoforge.event.tick.ServerTickEvent$Post` | 43 | 45 | 55 | 43/1/1/12 | same | 2/0 |
| `neoforge.common.conditions.ICondition` | 42 | 79 | 131 | 127/1/237/4 | same | 6/0 |
| `neoforge.common.CommonHooks` | 42 | 43 | 111 | 74/3/1/37 | same | 31/8 |
| `neoforge.event.level.LevelEvent` | 42 | 50 | 81 | 69/42/3/12 | same | 1/0 |
| `neoforge.common.ItemAbilities` | 41 | 43 | 114 | 102/0/5/12 | same | 32/17 |
| `neoforge.fluids.FluidUtil` | 41 | 41 | 101 | 94/5/0/7 | moved | 13/13 |
| `neoforge.capabilities.IBlockCapabilityProvider` | 41 | 44 | 87 | 80/0/6/7 | same | 1/0 |
| `neoforge.event.AddReloadListenerEvent` | 41 | 50 | 59 | 49/0/11/10 | GONE | 4/4 |
| `neoforge.common.util.TriState` | 40 | 48 | 89 | 74/19/4/15 | GONE | 8/8 |
| `neoforge.event.tick.LevelTickEvent` | 40 | 46 | 62 | 55/11/2/7 | same | 1/0 |
| `neoforge.event.server.ServerStartedEvent` | 40 | 43 | 50 | 41/0/4/9 | same | 1/0 |
| `fml.InterModComms` | 40 | 47 | 47 | 44/5/6/3 | same | 2/0 |
| `neoforge.common.util.INBTSerializable` | 39 | 39 | 174 | 168/1/17/6 | GONE | 2/2 |
| `neoforge.items.wrapper.InvWrapper` | 39 | 41 | 80 | 74/1/2/6 | GONE | 3/3 |
| `neoforge.common.ModConfigSpec$EnumValue` | 39 | 40 | 71 | 66/17/1/5 | same | 2/0 |
| `neoforge.event.entity.living.LivingDamageEvent` | 38 | 39 | 133 | 115/0/1/18 | same | - |
| `neoforge.fluids.capability.templates.FluidTank` | 37 | 37 | 237 | 223/28/4/14 | GONE | 21/21 |
| `neoforge.event.entity.living.LivingIncomingDamageEvent` | 37 | 38 | 89 | 80/0/1/9 | same | 10/0 |
| `neoforge.event.server.ServerStoppedEvent` | 36 | 38 | 54 | 48/0/2/6 | same | 1/0 |
| `neoforge.event.tick.PlayerTickEvent` | 36 | 42 | 47 | 45/16/0/2 | same | 1/0 |
| `neoforge.capabilities.BlockCapabilityCache` | 35 | 35 | 131 | 126/0/0/5 | same | 6/0 |
| `fml.loading.LoadingModList` | 35 | 46 | 44 | 41/14/5/3 | same | 5/0 |
| `neoforge.network.codec.NeoForgeStreamCodecs` | 34 | 34 | 114 | 112/0/0/2 | same | 4/1 |
| `fml.LogicalSide` | 34 | 35 | 99 | 83/9/15/16 | same | 5/0 |
| `fml.IExtensionPoint` | 34 | 68 | 36 | 1/29/5/35 | same | - |
| `neoforge.fluids.FluidType$Properties` | 32 | 33 | 80 | 55/1/3/25 | same | 20/0 |
| `neoforge.common.conditions.ICondition$IContext` | 32 | 34 | 62 | 62/1/2/0 | same | 3/1 |
| `neoforge.event.tick.LevelTickEvent$Post` | 32 | 37 | 41 | 36/8/2/5 | same | 2/0 |
| `neoforge.event.entity.living.LivingDropsEvent` | 31 | 31 | 42 | 41/0/1/1 | same | 7/0 |
| `fml.event.lifecycle.InterModEnqueueEvent` | 31 | 34 | 41 | 36/2/3/5 | same | 1/0 |
| `neoforge.registries.NewRegistryEvent` | 31 | 34 | 41 | 37/1/4/4 | same | 2/0 |
| `neoforge.fluids.BaseFlowingFluid` | 30 | 30 | 101 | 97/2/13/4 | same | 7/0 |
| `bus.api.ICancellableEvent` | 30 | 32 | 69 | 61/3/0/8 | same | 2/0 |
| `neoforge.common.NeoForgeMod` | 30 | 35 | 66 | 58/0/15/8 | same | 9/0 |
| `neoforge.event.level.LevelEvent$Unload` | 30 | 39 | 55 | 43/36/2/12 | same | 2/0 |
| `neoforgespi.language.IModFileInfo` | 30 | 32 | 50 | 43/4/3/7 | same | 6/1 |
| `neoforge.common.Tags$Blocks` | 30 | 56 | 49 | 39/1/52/10 | same | 60/0 |
| `neoforge.common.loot.IGlobalLootModifier` | 29 | 32 | 73 | 72/0/16/1 | same | 3/1 |
| `neoforge.event.OnDatapackSyncEvent` | 29 | 31 | 35 | 31/1/1/4 | same | 3/0 |
| `neoforge.event.entity.EntityAttributeCreationEvent` | 29 | 32 | 35 | 28/0/4/7 | same | 1/0 |
| `neoforge.common.util.Lazy` | 28 | 36 | 135 | 116/37/16/19 | same | 3/0 |
| `neoforge.event.entity.player.ItemTooltipEvent` | 28 | 68 | 40 | 21/51/1/19 | same | 5/0 |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedOutEvent` | 28 | 29 | 32 | 26/1/0/6 | same | 1/0 |
| `neoforge.registries.datamaps.DataMapType` | 27 | 42 | 132 | 117/9/32/15 | same | 3/2 |
| `neoforge.event.entity.living.LivingDamageEvent$Pre` | 27 | 27 | 87 | 78/0/0/9 | same | 6/0 |
| `neoforge.event.tick.EntityTickEvent` | 27 | 28 | 65 | 56/2/0/9 | same | 1/0 |
| `neoforge.common.crafting.ICustomIngredient` | 27 | 29 | 49 | 48/1/8/1 | same | 4/1 |
| `neoforge.common.util.FakePlayerFactory` | 27 | 27 | 40 | 37/0/0/3 | same | 2/0 |
| `neoforge.event.level.LevelEvent$Load` | 27 | 32 | 39 | 37/12/1/2 | same | 2/0 |
| `neoforge.event.entity.player.PlayerInteractEvent$LeftClickBlock` | 27 | 31 | 38 | 33/10/3/5 | same | 14/4 |
| `neoforge.fluids.BaseFlowingFluid$Properties` | 26 | 27 | 76 | 75/0/4/1 | same | 7/0 |
| `neoforge.energy.EnergyStorage` | 26 | 27 | 69 | 63/6/2/6 | GONE | 11/11 |
| `neoforge.event.entity.player.PlayerEvent$PlayerChangedDimensionEvent` | 26 | 27 | 29 | 27/0/1/2 | same | 3/0 |
| `neoforge.event.server.ServerStartingEvent` | 26 | 31 | 28 | 25/0/5/3 | same | 1/0 |
| `neoforge.attachment.IAttachmentHolder` | 25 | 25 | 98 | 91/3/0/7 | same | 14/0 |
| `neoforge.event.entity.living.LivingDamageEvent$Post` | 25 | 26 | 61 | 50/0/1/11 | same | 4/1 |
| `neoforge.common.loot.LootModifier` | 25 | 26 | 45 | 45/0/1/0 | same | 2/2 |
| `fml.loading.moddiscovery.ModFileInfo` | 24 | 33 | 32 | 27/11/2/5 | same | 3/0 |
| `neoforge.event.tick.PlayerTickEvent$Post` | 24 | 30 | 30 | 28/13/0/2 | same | 1/0 |
| `neoforge.event.server.ServerAboutToStartEvent` | 24 | 28 | 29 | 26/0/4/3 | same | 1/0 |
| `neoforge.event.entity.player.PlayerEvent$PlayerRespawnEvent` | 24 | 25 | 28 | 28/1/1/0 | same | 2/0 |
| `neoforge.event.server.ServerStoppingEvent` | 24 | 25 | 28 | 23/0/1/5 | same | 1/0 |
| `fml.event.lifecycle.FMLLoadCompleteEvent` | 24 | 29 | 25 | 13/7/0/12 | same | 1/0 |
| `neoforge.fluids.crafting.FluidIngredient` | 23 | 24 | 96 | 88/0/7/8 | same | 19/10 |
| `neoforge.common.SoundAction` | 23 | 23 | 44 | 39/0/1/5 | same | 1/0 |
| `neoforge.event.level.BlockEvent$EntityPlaceEvent` | 23 | 23 | 34 | 31/0/0/3 | same | 9/0 |
| `neoforge.fluids.BaseFlowingFluid$Flowing` | 23 | 23 | 34 | 32/0/7/2 | same | 4/1 |
| `neoforge.event.entity.player.PlayerEvent$Clone` | 23 | 24 | 33 | 30/1/1/3 | same | 3/0 |
| `neoforge.event.entity.player.PlayerInteractEvent$RightClickItem` | 23 | 24 | 29 | 26/1/0/3 | same | 8/0 |
| `neoforge.common.ModConfigSpec$LongValue` | 22 | 22 | 102 | 96/0/0/6 | same | 2/0 |
| `neoforge.common.crafting.IngredientType` | 22 | 24 | 47 | 45/0/6/2 | same | 2/0 |
| `neoforge.event.entity.living.LivingEvent` | 22 | 22 | 44 | 44/0/0/0 | same | 1/0 |
| `neoforgespi.locating.IModFile` | 22 | 26 | 33 | 31/5/3/2 | same | 8/3 |
| `neoforge.event.entity.EntityTeleportEvent` | 22 | 22 | 31 | 31/0/0/0 | same | 14/1 |
| `neoforgespi.language.ModFileScanData` | 21 | 24 | 46 | 45/2/1/1 | same | 5/0 |
| `neoforge.fluids.BaseFlowingFluid$Source` | 21 | 22 | 25 | 25/0/2/0 | same | 2/1 |
| `neoforge.common.MutableDataComponentHolder` | 20 | 20 | 89 | 87/4/0/2 | same | 7/0 |
| `neoforge.common.SoundActions` | 20 | 20 | 41 | 38/0/1/3 | same | 4/0 |
| `neoforgespi.language.ModFileScanData$AnnotationData` | 20 | 23 | 38 | 37/2/1/1 | same | 5/0 |
| `neoforge.registries.RegisterEvent$RegisterHelper` | 20 | 22 | 34 | 30/0/5/4 | same | 2/1 |
| `neoforge.event.entity.player.ItemEntityPickupEvent` | 20 | 21 | 29 | 27/0/2/2 | same | - |
| `neoforge.common.Tags$EntityTypes` | 20 | 23 | 28 | 23/0/11/5 | same | 3/0 |
| `neoforge.common.util.BlockSnapshot` | 20 | 20 | 28 | 26/0/0/2 | same | 5/0 |
| `fml.loading.moddiscovery.ModInfo` | 20 | 22 | 21 | 21/2/2/0 | same | 3/0 |
| `fml.util.ObfuscationReflectionHelper` | 19 | 23 | 35 | 27/2/3/8 | same | 5/0 |
| `neoforge.common.DeferredSpawnEggItem` | 19 | 19 | 27 | 19/1/0/8 | GONE | 3/3 |
| `neoforge.event.tick.LevelTickEvent$Pre` | 19 | 20 | 26 | 22/4/0/4 | same | 1/0 |
| `neoforge.event.TagsUpdatedEvent` | 19 | 21 | 23 | 17/3/0/6 | same | 3/2 |
| `neoforge.fluids.crafting.SizedFluidIngredient` | 18 | 20 | 160 | 130/4/17/30 | same | 14/5 |
| `neoforge.event.entity.player.PlayerEvent$BreakSpeed` | 18 | 18 | 25 | 24/0/0/1 | same | 8/0 |
| `neoforge.event.tick.EntityTickEvent$Post` | 18 | 18 | 24 | 19/0/0/5 | same | 1/0 |
| `neoforge.registries.datamaps.DataMapType$Builder` | 18 | 19 | 22 | 22/0/2/0 | same | 2/0 |
| `neoforge.event.tick.PlayerTickEvent$Pre` | 18 | 19 | 21 | 21/3/0/0 | same | 1/0 |
| `neoforge.common.EffectCure` | 17 | 18 | 35 | 35/0/1/0 | GONE | - |
| `neoforge.common.damagesource.DamageContainer` | 17 | 17 | 29 | 25/0/0/4 | same | 9/0 |
| `neoforge.event.entity.player.ItemEntityPickupEvent$Pre` | 17 | 18 | 25 | 23/0/2/2 | same | 5/2 |
| `neoforge.event.entity.RegisterSpawnPlacementsEvent` | 17 | 17 | 22 | 18/0/1/4 | same | 2/0 |
| `neoforge.event.level.BlockDropsEvent` | 17 | 17 | 22 | 20/0/0/2 | same | 12/0 |
| `neoforge.event.level.ChunkEvent` | 17 | 19 | 22 | 20/4/1/2 | same | 2/0 |
| `neoforge.event.tick.ServerTickEvent$Pre` | 17 | 17 | 20 | 16/0/0/4 | same | 1/0 |
| `neoforge.event.entity.RegisterSpawnPlacementsEvent$Operation` | 17 | 17 | 19 | 16/0/0/3 | same | 3/0 |
| `neoforge.common.crafting.SizedIngredient` | 16 | 18 | 111 | 97/12/19/14 | same | 13/3 |
| `neoforge.event.entity.living.FinalizeSpawnEvent` | 16 | 16 | 24 | 24/0/0/0 | same | 13/1 |
| `neoforge.event.entity.living.LivingEntityUseItemEvent` | 16 | 16 | 24 | 17/0/0/7 | same | 4/0 |
| `neoforge.event.entity.player.PlayerInteractEvent$EntityInteract` | 16 | 16 | 21 | 17/0/0/4 | same | 7/0 |
| `neoforge.registries.datamaps.RegisterDataMapTypesEvent` | 16 | 18 | 21 | 21/0/3/0 | same | 1/0 |
| `neoforge.event.entity.EntityAttributeModificationEvent` | 16 | 18 | 20 | 20/0/2/0 | same | 4/0 |
| `neoforge.fluids.FluidActionResult` | 16 | 16 | 18 | 15/0/0/3 | GONE | 5/5 |
| `neoforge.items.wrapper.CombinedInvWrapper` | 15 | 15 | 50 | 45/0/0/5 | GONE | 9/9 |
| `fml.util.thread.EffectiveSide` | 15 | 16 | 32 | 27/2/0/5 | same | 1/0 |
| `neoforge.event.entity.player.AttackEntityEvent` | 15 | 16 | 25 | 19/1/1/6 | same | 4/0 |
| `neoforge.event.level.ChunkEvent$Unload` | 15 | 18 | 20 | 18/3/1/2 | same | 3/0 |
| `neoforge.registries.DataPackRegistryEvent` | 15 | 15 | 19 | 19/0/2/0 | GONE | - |
| `neoforge.registries.DataPackRegistryEvent$NewRegistry` | 15 | 15 | 19 | 19/0/2/0 | GONE | 3/3 |
| `neoforge.event.entity.living.LivingEvent$LivingJumpEvent` | 15 | 15 | 17 | 17/0/0/0 | same | 1/0 |
| `neoforge.entity.PartEntity` | 14 | 15 | 81 | 44/1/1/37 | same | 55/8 |
| `neoforge.common.world.BiomeModifier` | 14 | 25 | 25 | 25/1/18/0 | same | - |
| `neoforge.event.entity.living.LivingFallEvent` | 14 | 14 | 19 | 19/0/0/0 | same | 6/2 |
| `neoforge.event.entity.living.LivingChangeTargetEvent` | 14 | 15 | 18 | 17/0/1/1 | same | 6/0 |
| `neoforge.fluids.IFluidTank` | 13 | 14 | 58 | 52/14/2/6 | GONE | 7/7 |
| `neoforge.fluids.SimpleFluidContent` | 13 | 13 | 53 | 45/2/0/8 | same | 9/0 |
| `neoforge.common.world.chunk.TicketController` | 13 | 13 | 29 | 27/0/3/2 | same | 5/2 |
| `fml.event.IModBusEvent` | 13 | 16 | 24 | 24/8/0/0 | same | - |
| `neoforge.common.world.ModifiableBiomeInfo` | 13 | 13 | 21 | 21/0/0/0 | same | - |
| `neoforge.common.world.ModifiableBiomeInfo$BiomeInfo` | 13 | 13 | 21 | 21/0/0/0 | same | - |
| `neoforge.common.world.ModifiableBiomeInfo$BiomeInfo$Builder` | 13 | 13 | 21 | 21/0/0/0 | same | 4/0 |
| `neoforge.event.entity.living.MobEffectEvent` | 13 | 14 | 21 | 21/0/1/0 | same | 2/0 |
| `neoforge.event.entity.living.LivingHealEvent` | 13 | 13 | 18 | 17/0/0/1 | same | 4/0 |
| `neoforge.event.AddPackFindersEvent` | 13 | 21 | 17 | 13/8/4/4 | same | 3/1 |
| `neoforge.event.entity.living.LivingExperienceDropEvent` | 13 | 13 | 17 | 16/0/0/1 | same | 6/0 |
| `neoforge.common.world.BiomeModifier$Phase` | 13 | 13 | 16 | 16/0/0/0 | same | 6/0 |
| `neoforge.event.entity.player.PlayerEvent$StartTracking` | 13 | 13 | 16 | 16/0/0/0 | same | 2/0 |
| `neoforge.event.tick.EntityTickEvent$Pre` | 12 | 13 | 42 | 38/2/0/4 | same | 2/0 |
| `neoforge.common.Tags$Fluids` | 12 | 15 | 25 | 22/1/10/3 | same | 7/0 |
| `neoforge.common.conditions.ConditionalOps` | 12 | 14 | 21 | 21/0/4/0 | same | 4/0 |
| `neoforge.items.wrapper.PlayerMainInvWrapper` | 12 | 12 | 20 | 19/0/0/1 | GONE | 3/3 |
| `neoforge.event.village.VillagerTradesEvent` | 12 | 12 | 17 | 15/0/0/2 | GONE | 3/3 |
| `neoforge.event.entity.ProjectileImpactEvent` | 12 | 12 | 16 | 16/0/0/0 | same | 6/0 |
| `neoforge.network.registration.HandlerThread` | 12 | 12 | 16 | 12/0/0/4 | same | 4/0 |
| `neoforge.event.level.ExplosionEvent` | 12 | 13 | 15 | 15/0/1/0 | same | - |
| `neoforge.common.world.chunk.LoadingValidationCallback` | 12 | 12 | 13 | 12/0/0/1 | same | - |
| `neoforge.items.ComponentItemHandler` | 11 | 11 | 38 | 34/4/0/4 | GONE | 7/7 |
| `neoforge.entity.IEntityWithComplexSpawn` | 11 | 11 | 31 | 30/0/0/1 | same | - |
| `neoforge.event.ItemAttributeModifierEvent` | 11 | 11 | 30 | 25/0/0/5 | same | 7/1 |
| `neoforge.event.level.ChunkWatchEvent` | 11 | 11 | 16 | 15/0/0/1 | same | - |
| `neoforge.common.Tags$DamageTypes` | 11 | 17 | 15 | 14/0/11/1 | same | 3/0 |
| `neoforge.common.util.DeferredSoundType` | 11 | 12 | 15 | 11/1/1/4 | same | 1/0 |
| `neoforge.event.entity.living.MobSpawnEvent` | 11 | 11 | 14 | 14/0/0/0 | same | - |
| `neoforge.common.EffectCures` | 11 | 12 | 13 | 13/0/1/0 | GONE | 3/3 |
| `neoforge.common.IShearable` | 11 | 11 | 13 | 12/0/0/1 | same | 3/1 |
| `neoforge.event.entity.player.PlayerEvent$ItemCraftedEvent` | 11 | 11 | 11 | 10/0/1/1 | same | 4/0 |
| `neoforge.items.wrapper.RecipeWrapper` | 10 | 10 | 25 | 24/0/0/1 | GONE | 3/3 |
| `fml.ModLoader` | 10 | 14 | 16 | 12/15/0/4 | same | 4/0 |
| `neoforge.event.entity.player.PlayerInteractEvent$EntityInteractSpecific` | 10 | 10 | 15 | 12/0/0/3 | GONE | 9/9 |
| `neoforge.common.crafting.DataComponentIngredient` | 10 | 17 | 14 | 13/0/7/1 | same | 8/5 |
| `neoforge.event.entity.EntityEvent` | 10 | 10 | 14 | 14/0/0/0 | same | 2/0 |
| `fml.event.lifecycle.InterModProcessEvent` | 10 | 14 | 13 | 9/1/3/4 | same | 3/0 |
| `neoforge.common.world.chunk.RegisterTicketControllersEvent` | 10 | 13 | 12 | 10/0/3/2 | same | 1/0 |
| `neoforge.event.level.ExplosionEvent$Detonate` | 10 | 11 | 12 | 12/0/1/0 | same | 4/1 |
| `neoforge.event.entity.living.LivingEquipmentChangeEvent` | 10 | 10 | 11 | 11/0/0/0 | same | 5/0 |
| `neoforge.event.entity.player.PlayerContainerEvent` | 10 | 10 | 11 | 11/0/0/0 | same | 1/0 |
| `neoforge.common.UsernameCache` | 10 | 10 | 10 | 8/0/0/2 | same | 2/0 |
| `neoforge.event.level.ChunkEvent$Load` | 10 | 12 | 10 | 10/4/0/0 | same | 3/0 |
| `neoforge.network.handling.DirectionalPayloadHandler` | 10 | 10 | 10 | 8/0/0/2 | GONE | 1/1 |
| `fml.common.asm.enumextension.EnumProxy` | 9 | 10 | 19 | 14/4/0/5 | same | 2/0 |
| `neoforge.common.world.BiomeGenerationSettingsBuilder` | 9 | 9 | 14 | 14/0/0/0 | same | 5/2 |
| `neoforge.event.AnvilUpdateEvent` | 9 | 11 | 12 | 11/1/1/1 | same | 11/2 |
| `neoforge.event.entity.EntityInvulnerabilityCheckEvent` | 9 | 9 | 12 | 12/0/0/0 | same | 5/0 |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Finish` | 9 | 9 | 12 | 8/0/0/4 | same | 5/0 |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Start` | 9 | 9 | 12 | 9/0/0/3 | same | 5/0 |
| `neoforge.fluids.FluidInteractionRegistry` | 9 | 9 | 12 | 8/0/0/4 | same | 2/0 |
| `neoforge.common.world.chunk.TicketHelper` | 9 | 9 | 11 | 10/0/0/1 | same | 6/0 |
| `neoforge.event.entity.living.LivingKnockBackEvent` | 9 | 9 | 11 | 10/0/0/1 | same | 11/0 |
| `neoforge.event.entity.player.ItemEntityPickupEvent$Post` | 9 | 9 | 11 | 11/0/0/0 | same | 5/0 |
| `neoforge.event.village.WandererTradesEvent` | 9 | 9 | 11 | 10/0/0/1 | GONE | 3/3 |
| `neoforge.fluids.FluidInteractionRegistry$InteractionInformation` | 9 | 9 | 11 | 7/0/0/4 | same | 6/0 |
| `fml.javafmlmod.FMLModContainer` | 9 | 9 | 10 | 9/1/0/1 | same | 2/0 |
| `neoforge.event.entity.EntityLeaveLevelEvent` | 9 | 13 | 10 | 8/8/0/2 | same | 2/0 |
| `neoforge.event.entity.player.PlayerContainerEvent$Open` | 9 | 9 | 10 | 10/0/0/0 | same | 3/0 |
| `neoforge.event.TagsUpdatedEvent$UpdateCause` | 9 | 11 | 9 | 7/2/0/2 | GONE | 2/2 |
| `neoforge.event.entity.EntityMountEvent` | 9 | 9 | 9 | 8/1/0/1 | same | 7/0 |
| `neoforge.common.util.NeoForgeExtraCodecs` | 8 | 8 | 23 | 23/0/0/0 | same | 9/0 |
| `neoforge.capabilities.ICapabilityInvalidationListener` | 8 | 8 | 15 | 15/0/0/0 | same | - |
| `neoforge.common.SpecialPlantable` | 8 | 8 | 13 | 11/0/0/2 | same | 2/0 |
| `neoforge.event.entity.living.MobEffectEvent$Added` | 8 | 8 | 13 | 13/0/0/0 | same | 4/0 |
| `neoforge.common.conditions.WithConditions` | 8 | 16 | 11 | 11/0/11/0 | same | 4/0 |
| `neoforge.network.connection.ConnectionType` | 8 | 9 | 11 | 10/1/0/1 | same | 2/0 |
| `neoforge.event.BlockEntityTypeAddBlocksEvent` | 8 | 9 | 9 | 9/0/1/0 | same | 1/0 |
| `neoforge.items.wrapper.SidedInvWrapper` | 8 | 8 | 9 | 7/0/0/2 | GONE | 1/1 |
| `neoforge.common.extensions.IBlockExtension` | 8 | 8 | 8 | 7/0/0/1 | same | 1/0 |
| `neoforge.event.brewing.RegisterBrewingRecipesEvent` | 8 | 8 | 8 | 8/0/0/0 | GONE | 1/1 |
| `neoforge.event.entity.item.ItemTossEvent` | 8 | 8 | 8 | 8/0/0/0 | same | 3/0 |
| `neoforge.event.entity.living.LivingShieldBlockEvent` | 8 | 8 | 8 | 7/0/0/1 | same | 10/4 |

Long tail: server-side classes used by fewer than 8 mods, grouped by package group. The number in brackets is the mods count. `*` marks a class that is moved or GONE in 26.3.

- **neoforge.event** (119): LootTableLoadEvent(7), EntityTeleportEvent.ChorusFruit(7)*, MobEffectEvent.Applicable(7), MobEffectEvent.Applicable.Result(7), MobSpawnEvent.PositionCheck(7), AdvancementEvent(7), PlayerInteractEvent.LeftClickEmpty(7), PlayerXpEvent(7), BlockEvent.BlockToolModificationEvent(7), ModifyDefaultComponentsEvent(6), ServerChatEvent(6), EntityTeleportEvent.TeleportCommand(6), EntityTravelToDimensionEvent(6), MobEffectEvent.Remove(6), AdvancementEvent.AdvancementEarnEvent(6), PlayerXpEvent.PickupXp(6), BlockEvent.NeighborNotifyEvent(6), ChunkWatchEvent.Sent(6), ExplosionEvent.Start(6), CommandEvent(5), EntityTeleportEvent.EnderEntity(5), EntityTeleportEvent.EnderPearl(5), LivingBreatheEvent(5), LivingConversionEvent(5), MobEffectEvent.Expired(5), MobSpawnEvent.PositionCheck.Result(5), CriticalHitEvent(5), PlayerContainerEvent.Close(5), PlayerEvent.HarvestCheck(5), PlayerInteractEvent.LeftClickBlock.Action(5), ChunkDataEvent(5), ChunkDataEvent.Load(5), ChunkWatchEvent.UnWatch(5), ChunkWatchEvent.Watch(5), LevelEvent.Save(5), LivingEntityUseItemEvent.Stop(4), LivingEntityUseItemEvent.Tick(4), LivingEvent.LivingVisibilityEvent(4), MobDespawnEvent(4), MobDespawnEvent.Result(4), MobSpawnEvent.SpawnPlacementCheck(4), MobSpawnEvent.SpawnPlacementCheck.Result(4), BonemealEvent(4), PlayerDestroyItemEvent(4), PlayerEvent.ItemSmeltedEvent(4), UseItemOnBlockEvent(4), BlockEvent.EntityMultiPlaceEvent(4), BlockEvent.FarmlandTrampleEvent(4), ChunkDataEvent.Save(4), GameShuttingDownEvent(3), GrindstoneEvent(3), GrindstoneEvent.OnPlaceItem(3), GrindstoneEvent.OnTakeItem(3), EntityEvent.Size(3), EntityMobGriefingEvent(3), ArmorHurtEvent(3), BabyEntitySpawnEvent(3), EnderManAngerEvent(3)*, LivingConversionEvent.Post(3), LivingConversionEvent.Pre(3), LivingDestroyBlockEvent(3), PlayerRespawnPositionEvent(3), PlayerSpawnPhantomsEvent(3), PlayerSpawnPhantomsEvent.Result(3), FurnaceFuelBurnTimeEvent(3)*, ChunkTicketLevelUpdatedEvent(3), NoteBlockEvent(3), NoteBlockEvent.Play(3), VillageSiegeEvent(3), ItemStackedOnOtherEvent(2), PlayLevelSoundEvent(2), PlayLevelSoundEvent.AtEntity(2), PlayLevelSoundEvent.AtPosition(2), GetEnchantmentLevelEvent(2), EntityEvent.EntityConstructing(2), EntityStruckByLightningEvent(2), EntityTeleportEvent.SpreadPlayersCommand(2), ItemExpireEvent(2), AnimalTameEvent(2), ArmorHurtEvent.ArmorEntry(2), LivingGetProjectileEvent(2), AnvilRepairEvent(2)*, ArrowLooseEvent(2), PlayerEvent.NameFormat(2), PlayerEvent.SaveToFile(2), PlayerInteractEvent.RightClickEmpty(2), UseItemOnBlockEvent.UsePhase(2), BlockEvent.FluidPlaceBlockEvent(2), BlockGrowFeatureEvent(2), ExplosionKnockbackEvent(2), LevelEvent.PotentialSpawns(2), SleepFinishedTimeEvent(2), CropGrowEvent(2), CropGrowEvent.Post(2), RegisterGameTestsEvent(1), StatAwardEvent(1), EntityEvent.EnteringSection(1), ItemEvent(1), LivingChangeTargetEvent.ILivingTargetType(1), LivingChangeTargetEvent.LivingTargetType(1), LivingUseTotemEvent(1), MobSplitEvent(1), AdvancementEvent.AdvancementProgressEvent(1), AdvancementEvent.AdvancementProgressEvent.ProgressType(1), ArrowNockEvent(1), CanContinueSleepingEvent(1), CanPlayerSleepEvent(1), ItemFishedEvent(1), PlayerEvent.LoadFromFile(1), PlayerEvent.StopTracking(1), PlayerWakeUpEvent(1), PlayerXpEvent.XpChange(1), TradeWithVillagerEvent(1), BlockEvent.PortalSpawnEvent(1), ModifyCustomSpawnersEvent(1), PistonEvent(1), PistonEvent.Pre(1), CropGrowEvent.Pre(1), CropGrowEvent.Pre.Result(1)
- **neoforge.common** (86): ModConfigSpec.ValueSpec(7), PercentageAttribute(7), ModLoadedCondition(7), CompoundIngredient(7), IItemStackExtension(7), RecipeMatcher(7), MobSpawnSettingsBuilder(7), BrewingRecipe(6)*, IBrewingRecipe(6)*, LogicalSidedProvider(6)*, AuxiliaryLightManager(6), TicketSet(6), BasicItemListing(5)*, ModConfigSpec.Range(5), SimpleTier(5)*, IntersectionIngredient(5), AttributeTooltipContext(5), Tags.Biomes(4), DifferenceIngredient(4), ICommonPacketListener(4), TriPredicate(4), BooleanAttribute(3), CreativeModeTabRegistry(3), ModConfigSpec.RestartType(3), NeoForgeConfig(3)*, NeoForgeConfig.Client(3)*, TranslatableEnum(3), IAttributeExtension(3), IItemExtension(3), ILevelExtension(3), IOwnedSpawner(3), AttributeUtil(3), FarmlandWaterManager(2), AndCondition(2), FalseCondition(2)*, BlockTagIngredient(2), DamageContainer.Reduction(2), IAbstractMinecartExtension(2)*, IBaseRailBlockExtension(2), IBlockEntityExtension(2), IFriendlyByteBufExtension(2), AddTableLootModifier(2), CanItemPerformAbility(2), LootTableIdCondition(2), ConcatenatedListView(2), DataComponentUtil(2)*, ItemStackMap(2), Size2i(2), BiomeSpecialEffectsBuilder(2), ClimateSettingsBuilder(2), ModifiableStructureInfo(2), ModifiableStructureInfo.StructureInfo(2), PieceBeardifierModifier(2), StructureModifier(2), ExtendPoiTypesEvent(2), IOUtilities(1), LenientUnboundedMapCodec(1), WorldWorkerManager(1)*, WorldWorkerManager.IWorker(1)*, ConditionContext(1), ItemExistsCondition(1)*, NotCondition(1), OrCondition(1), TagEmptyCondition(1), TrueCondition(1)*, CraftingHelper(1)*, IDeathMessageProvider(1), IReductionFunction(1), BubbleColumnDirection(1), IBlockStateExtension(1), IDataComponentHolderExtension(1), IHolderExtension(1), IMobEffectExtension(1), IServerCommonPacketListenerExtension(1), TooltipFlagExtension(1), LootModifierManager(1), LootTableIdCondition.Builder(1), AABBTicket(1), FriendlyByteBufUtil(1), InsertingContents(1), BiomeModifiers(1), BiomeModifiers.AddSpawnsBiomeModifier(1), ModifiableStructureInfo.StructureInfo.Builder(1), StructureModifier.Phase(1), StructureSettingsBuilder(1), StructureSettingsBuilder.StructureSpawnOverrideBuilder(1)
- **neoforge.registries** (34): GameData(6), BakeCallback(5), NeoForgeDataMaps(5), RegistryCallback(4), DataMapsUpdatedEvent(4), Waxable(4), AdvancedDataMapType(3), Compostable(3)*, FurnaceFuel(3)*, Oxidizable(3), AnyHolderSet(3), ModifyRegistriesEvent(2), AddCallback(2), AdvancedDataMapType.Builder(2), DataMapValueRemover(2), OrHolderSet(2), DataPackRegistriesHooks(1), IdMappingEvent(1)*, RegistryManager(1), RegistrySnapshot(1), ClearCallback(1), DataMapEntry(1), DataMapEntry.Removal(1), DataMapFile(1), DataMapValueMerger(1), DataMapValueRemover.Default(1), IWithData(1), BiomeVillagerType(1), MonsterRoomMob(1), ParrotImitation(1), RaidHeroGift(1), VibrationFrequency(1), HolderSetType(1), ICustomHolderSet(1)
- **fml.loading** (20): ModFile(5), ModAnnotation(5), ModAnnotation.EnumHolder(5), FMLConfig(3), FMLConfig.ConfigValue(2), BuiltInLanguageLoader(1), ImmediateWindowHandler(1), JarVersionLookupHandler(1), StringUtils(1), TracingPrintStream(1), VersionInfo(1), ModFileParser(1), ModFileParser.MixinConfig(1), JarInJarDependencyLocator(1), JarModsDotTomlModFileReader(1), Scanner(1), ProgressMeter(1), StartupNotificationManager(1), CommonLaunchHandler(1)*, TopologicalSort(1)
- **neoforge.fluids** (20): FluidHandlerItemStack(7)*, FluidInteractionRegistry.HasFluidInteraction(6), FluidBucketWrapper(6)*, DataComponentFluidIngredient(6), CompoundFluidIngredient(4), TagFluidIngredient(4)*, EmptyFluidHandler(3)*, VoidFluidHandler(3)*, FluidIngredientType(3), FluidType.DripstoneDripInfo(2), BucketPickupHandlerWrapper(2)*, SingleFluidIngredient(2)*, FluidInteractionRegistry.FluidInteraction(1), FluidStackLinkedSet(1), RegisterCauldronFluidContentEvent(1), FluidHandlerItemStack.Consumable(1)*, FluidHandlerItemStack.SwapEmpty(1)*, FluidHandlerItemStackSimple(1)*, DifferenceFluidIngredient(1), IntersectionFluidIngredient(1)
- **neoforgespi** (15): IConfigurable(6), IModInfo.ModVersion(3), ModFileScanData.ClassData(2), IDependencyLocator(2), ModFileDiscoveryAttributes(2), ModFileInfoParser(2), Environment(1)*, IIssueReporting(1), ICoreMod(1)*, IModInfo.DependencyType(1), IModLanguageLoader(1), IDiscoveryPipeline(1), IModFileCandidateLocator(1), IModFileReader(1), InvalidModFileException(1)
- **neoforge.server** (12): EnumArgument(7), PermissionAPI(7), PermissionDynamicContext(7), PermissionNode(7), PermissionGatherEvent(6), PermissionGatherEvent.Nodes(6), PermissionDynamicContextKey(6), PermissionNode.PermissionResolver(6), PermissionType(6), PermissionTypes(6), ConfigCommand(1)*, ModIdArgument(1)
- **fml.(root)** (10): InterModComms.IMCMessage(7), ModLoadingIssue(4), CrashReportCallables(3), ModLoadingException(3), Logging(2), IBindingsProvider(1), ModLoadingIssue.Severity(1), VersionChecker(1), VersionChecker.CheckResult(1), VersionChecker.Status(1)
- **neoforge.items** (8): ItemHandlerCopySlot(7)*, RangedWrapper(7)*, EmptyItemHandler(6)*, PlayerArmorInvWrapper(4)*, PlayerOffhandInvWrapper(4)*, PlayerInvWrapper(3)*, VanillaInventoryCodeHooks(1)*, ForwardingItemHandler(1)*
- **bus** (8): EventBus(2), EventListener(2), EventBusErrorMessage(1), ListenerList(1), LockHelper(1), SubscribeEventListener(1), BusBuilder(1), IEventExceptionHandler(1)
- **neoforge.network** (5): RegisterConfigurationTasksEvent(7), NetworkRegistry(7), ICustomConfigurationTask(5), MainThreadPayloadHandler(3), PacketAndPayloadAcceptor(1)*
- **fml.event** (4): ModConfigEvent.Unloading(6), FMLConstructModEvent(4), FMLDedicatedServerSetupEvent(2), ParallelDispatchEvent(2)
- **fml.util** (3): SidedThreadGroup(7), SidedThreadGroups(7), ObfuscationReflectionHelper.UnableToFindFieldException(1)
- **neoforge.attachment** (3): IAttachmentSerializer(7), IAttachmentCopyHandler(5), AttachmentHolder(3)
- **neoforge.capabilities** (3): BaseCapability(4), CapabilityRegistry(1), CapabilityRegistry.CapabilityConstructor(1)
- **fml.config** (3): IConfigSpec.ILoadedConfig(5), ConfigTracker(4), ModConfigs(3)
- **neoforge.energy** (2): ComponentEnergyStorage(4)*, EmptyEnergyStorage(3)*
- **neoforge.internal** (2): NeoForgeVersion(3)*, BrandingControl(1)
- **neoforge.resource** (2): ResourcePackLoader(6), ContextAwareReloadListener(2)
- **fml.mclanguageprovider** (1): MinecraftModContainer(1)
- **fml.javafmlmod** (1): AutomaticEventSubscriber(1)
- **coremod** (1): ASMAPI(2)*
- **fml.i18n** (1): MavenVersionTranslator(1)

Client-only or dev-only NeoForge classes (303, not listed): neoforge.client 255, neoforge.common 29, neoforge.data 5, neoforge.gametest 3, fml.earlydisplay 3, neoforgespi 3, fml.common 2, neoforge.event 1.

## 4. Minecraft classes by number of mods

| Mods (srv) | Minecraft classes | Of them moved or GONE in 26.3 |
|:--|--:|--:|
| 1 | 1097 | 521 |
| 2-4 | 729 | 176 |
| 5-9 | 396 | 70 |
| 10-24 | 359 | 75 |
| 25-49 | 162 | 30 |
| 50-99 | 109 | 7 |
| 100-199 | 86 | 5 |
| 200-414 | 42 | 1 |
| 0 (client or dev only) | 1136 | - |

Top 200 `net.minecraft.*` classes by server-side mods. `net.minecraft.` is dropped from the names.

| # | Class | Mods (srv) | Mods (all) | Refs | 26.3 | Members n/u |
|--:|:--|--:|--:|--:|:--|:--|
| 1 | `resources.ResourceLocation` | 367 | 386 | 13144 | moved | 31/28 |
| 2 | `network.chat.Component` | 335 | 363 | 9090 | same | 31/1 |
| 3 | `world.item.ItemStack` | 327 | 341 | 15659 | same | 166/44 |
| 4 | `network.chat.MutableComponent` | 325 | 351 | 6872 | same | 48/28 |
| 5 | `world.item.Item` | 312 | 326 | 8704 | same | 87/44 |
| 6 | `world.level.Level` | 310 | 324 | 13802 | same | 266/74 |
| 7 | `resources.ResourceKey` | 306 | 324 | 4485 | same | 13/4 |
| 8 | `world.entity.player.Player` | 302 | 311 | 10357 | same | 362/63 |
| 9 | `core.BlockPos` | 296 | 303 | 13843 | same | 68/6 |
| 10 | `world.level.block.state.BlockState` | 280 | 285 | 10604 | same | 144/36 |
| 11 | `world.level.block.Block` | 275 | 285 | 7384 | same | 140/50 |
| 12 | `core.registries.Registries` | 265 | 280 | 1921 | same | 101/8 |
| 13 | `server.level.ServerPlayer` | 263 | 265 | 3096 | same | 276/46 |
| 14 | `world.level.ItemLike` | 262 | 280 | 2915 | same | 2/0 |
| 15 | `core.Holder` | 258 | 276 | 5210 | same | 20/1 |
| 16 | `nbt.CompoundTag` | 257 | 262 | 4762 | same | 56/24 |
| 17 | `core.registries.BuiltInRegistries` | 257 | 274 | 1998 | same | 79/14 |
| 18 | `network.codec.StreamCodec` | 252 | 255 | 4845 | same | 16/0 |
| 19 | `server.level.ServerLevel` | 252 | 254 | 4153 | same | 219/40 |
| 20 | `core.Direction` | 249 | 258 | 7546 | same | 51/6 |
| 21 | `world.entity.Entity` | 249 | 267 | 6291 | same | 355/62 |
| 22 | `ChatFormatting` | 248 | 272 | 2135 | same | 35/7 |
| 23 | `core.HolderLookup` | 245 | 270 | 3626 | same | 6/0 |
| 24 | `core.HolderLookup$Provider` | 239 | 266 | 3488 | same | 8/1 |
| 25 | `core.Registry` | 239 | 249 | 2017 | same | 59/23 |
| 26 | `core.DefaultedRegistry` | 239 | 261 | 1474 | same | 41/18 |
| 27 | `world.level.block.entity.BlockEntity` | 235 | 240 | 5643 | same | 49/14 |
| 28 | `tags.TagKey` | 232 | 257 | 2822 | same | 10/3 |
| 29 | `core.component.DataComponentType` | 232 | 243 | 2169 | same | 9/0 |
| 30 | `network.RegistryFriendlyByteBuf` | 226 | 229 | 3508 | same | 95/4 |
| 31 | `world.entity.LivingEntity` | 219 | 239 | 5006 | same | 453/78 |
| 32 | `world.phys.Vec3` | 219 | 238 | 4752 | same | 50/2 |
| 33 | `world.item.Item$Properties` | 217 | 217 | 2559 | same | 19/7 |
| 34 | `world.InteractionHand` | 214 | 216 | 2532 | same | 5/0 |
| 35 | `nbt.Tag` | 210 | 217 | 1906 | same | 12/1 |
| 36 | `network.protocol.common.custom.CustomPacketPayload` | 209 | 210 | 2940 | same | 5/0 |
| 37 | `network.FriendlyByteBuf` | 209 | 212 | 2237 | same | 110/5 |
| 38 | `world.entity.player.Inventory` | 207 | 213 | 2795 | same | 50/15 |
| 39 | `network.codec.ByteBufCodecs` | 204 | 206 | 2122 | same | 38/0 |
| 40 | `util.RandomSource` | 200 | 221 | 3556 | same | 17/1 |
| 41 | `network.protocol.common.custom.CustomPacketPayload$Type` | 200 | 201 | 1873 | same | 3/2 |
| 42 | `server.MinecraftServer` | 200 | 203 | 1448 | same | 93/36 |
| 43 | `world.level.BlockGetter` | 198 | 207 | 2457 | same | 15/4 |
| 44 | `core.RegistryAccess` | 198 | 209 | 1696 | same | 12/3 |
| 45 | `world.level.block.state.properties.Property` | 197 | 201 | 3800 | same | 9/1 |
| 46 | `world.level.block.entity.BlockEntityType` | 191 | 195 | 3230 | same | 26/20 |
| 47 | `util.Mth` | 191 | 224 | 2133 | same | 74/5 |
| 48 | `world.phys.BlockHitResult` | 191 | 203 | 1914 | same | 9/1 |
| 49 | `sounds.SoundEvent` | 190 | 198 | 2493 | same | 6/3 |
| 50 | `world.level.block.state.BlockBehaviour` | 186 | 190 | 2590 | same | 3/2 |
| 51 | `world.InteractionResult` | 186 | 188 | 1605 | same | 15/12 |
| 52 | `world.level.block.Blocks` | 183 | 199 | 1705 | same | 654/187 |
| 53 | `world.item.CreativeModeTab` | 183 | 194 | 431 | same | 12/0 |
| 54 | `world.item.Items` | 180 | 223 | 1201 | same | 651/74 |
| 55 | `world.item.BlockItem` | 179 | 188 | 789 | same | 39/11 |
| 56 | `world.level.block.state.BlockBehaviour$Properties` | 178 | 181 | 2546 | same | 65/8 |
| 57 | `core.Vec3i` | 177 | 190 | 1745 | same | 21/0 |
| 58 | `core.Holder$Reference` | 177 | 203 | 1093 | same | 17/0 |
| 59 | `world.entity.EntityType` | 174 | 182 | 2445 | same | 148/130 |
| 60 | `world.item.TooltipFlag` | 173 | 185 | 1420 | same | 6/0 |
| 61 | `world.inventory.AbstractContainerMenu` | 172 | 179 | 2193 | same | 40/3 |
| 62 | `core.NonNullList` | 172 | 178 | 1514 | same | 33/0 |
| 63 | `world.item.Item$TooltipContext` | 168 | 179 | 1372 | same | 6/0 |
| 64 | `world.phys.shapes.VoxelShape` | 167 | 173 | 1624 | same | 18/0 |
| 65 | `sounds.SoundSource` | 165 | 169 | 1553 | same | 12/0 |
| 66 | `world.level.LevelAccessor` | 162 | 166 | 1457 | same | 145/26 |
| 67 | `sounds.SoundEvents` | 162 | 177 | 1265 | same | 644/37 |
| 68 | `world.phys.AABB` | 161 | 186 | 1785 | same | 52/0 |
| 69 | `world.entity.item.ItemEntity` | 160 | 160 | 1010 | same | 105/16 |
| 70 | `world.inventory.MenuType` | 157 | 159 | 1332 | same | 17/1 |
| 71 | `world.level.material.Fluid` | 155 | 165 | 1307 | same | 21/8 |
| 72 | `world.level.block.state.StateDefinition` | 154 | 159 | 1358 | same | 5/0 |
| 73 | `world.level.LevelReader` | 153 | 156 | 1170 | same | 43/5 |
| 74 | `core.component.DataComponents` | 153 | 165 | 841 | same | 58/4 |
| 75 | `network.chat.Style` | 149 | 177 | 738 | same | 30/3 |
| 76 | `core.component.DataComponentType$Builder` | 148 | 157 | 179 | same | 5/0 |
| 77 | `world.level.block.entity.BlockEntityType$BlockEntitySupplier` | 144 | 145 | 288 | same | 1/0 |
| 78 | `world.item.CreativeModeTab$Builder` | 142 | 147 | 177 | same | 14/3 |
| 79 | `world.item.crafting.Ingredient` | 141 | 159 | 1516 | same | 38/24 |
| 80 | `world.level.block.state.StateDefinition$Builder` | 141 | 142 | 1272 | same | 4/0 |
| 81 | `world.level.material.FluidState` | 141 | 152 | 953 | same | 26/6 |
| 82 | `world.inventory.Slot` | 139 | 146 | 1141 | same | 32/2 |
| 83 | `world.level.block.state.properties.BooleanProperty` | 137 | 139 | 1971 | same | 3/0 |
| 84 | `world.item.context.BlockPlaceContext` | 137 | 140 | 1182 | same | 22/0 |
| 85 | `world.level.block.state.properties.DirectionProperty` | 135 | 138 | 1956 | GONE | 5/5 |
| 86 | `world.Container` | 135 | 138 | 1127 | same | 22/2 |
| 87 | `world.phys.shapes.CollisionContext` | 134 | 139 | 990 | same | 7/0 |
| 88 | `world.item.context.UseOnContext` | 134 | 135 | 647 | same | 15/0 |
| 89 | `core.particles.ParticleOptions` | 133 | 137 | 1539 | same | 1/0 |
| 90 | `world.level.block.state.properties.BlockStateProperties` | 133 | 135 | 1255 | same | 107/11 |
| 91 | `world.InteractionResultHolder` | 131 | 132 | 700 | moved | 8/8 |
| 92 | `world.item.CreativeModeTab$Output` | 131 | 135 | 189 | same | 5/0 |
| 93 | `network.protocol.Packet` | 130 | 131 | 407 | same | - |
| 94 | `Util` | 129 | 164 | 588 | moved | 45/45 |
| 95 | `nbt.ListTag` | 129 | 131 | 545 | same | 28/10 |
| 96 | `world.phys.HitResult` | 127 | 143 | 846 | same | 4/0 |
| 97 | `commands.CommandSourceStack` | 127 | 130 | 650 | same | 23/4 |
| 98 | `util.StringRepresentable` | 127 | 131 | 639 | same | 5/1 |
| 99 | `world.level.block.SoundType` | 127 | 128 | 532 | same | 115/0 |
| 100 | `world.item.CreativeModeTab$ItemDisplayParameters` | 127 | 132 | 176 | same | 5/0 |
| 101 | `network.codec.StreamDecoder` | 123 | 126 | 674 | same | 1/0 |
| 102 | `commands.Commands` | 123 | 126 | 507 | same | 6/0 |
| 103 | `core.Direction$Axis` | 122 | 134 | 1419 | same | 19/0 |
| 104 | `world.MenuProvider` | 121 | 122 | 683 | same | 2/0 |
| 105 | `world.item.CreativeModeTab$DisplayItemsGenerator` | 121 | 125 | 135 | same | 1/0 |
| 106 | `world.phys.shapes.Shapes` | 119 | 123 | 717 | same | 13/1 |
| 107 | `world.item.crafting.Recipe` | 118 | 136 | 1329 | same | 17/9 |
| 108 | `world.item.crafting.RecipeHolder` | 118 | 122 | 1135 | same | 4/2 |
| 109 | `world.item.crafting.RecipeInput` | 118 | 122 | 815 | same | 3/0 |
| 110 | `world.level.material.Fluids` | 118 | 124 | 502 | same | 5/0 |
| 111 | `world.item.crafting.RecipeType` | 116 | 120 | 1012 | same | 9/1 |
| 112 | `core.particles.SimpleParticleType` | 114 | 117 | 859 | same | 2/0 |
| 113 | `world.level.block.entity.BlockEntityTicker` | 114 | 114 | 515 | same | 1/0 |
| 114 | `world.damagesource.DamageSource` | 113 | 114 | 1384 | same | 19/0 |
| 115 | `core.particles.ParticleTypes` | 113 | 115 | 877 | same | 87/3 |
| 116 | `world.entity.EquipmentSlot` | 113 | 120 | 709 | same | 16/1 |
| 117 | `world.item.crafting.RecipeSerializer` | 112 | 114 | 839 | same | 8/5 |
| 118 | `world.level.block.entity.BlockEntityType$Builder` | 112 | 115 | 124 | GONE | 2/2 |
| 119 | `world.level.ChunkPos` | 111 | 114 | 682 | same | 32/5 |
| 120 | `world.ItemInteractionResult` | 110 | 110 | 678 | GONE | 10/9 |
| 121 | `world.item.crafting.RecipeManager` | 109 | 115 | 499 | same | 17/11 |
| 122 | `world.level.material.MapColor` | 107 | 112 | 269 | same | 70/1 |
| 123 | `server.packs.resources.ResourceManager` | 105 | 136 | 351 | same | 8/6 |
| 124 | `world.level.material.FlowingFluid` | 103 | 108 | 476 | same | 12/0 |
| 125 | `core.component.DataComponentMap` | 103 | 109 | 453 | same | 16/0 |
| 126 | `world.level.block.EntityBlock` | 102 | 104 | 435 | same | 3/0 |
| 127 | `network.chat.FormattedText` | 101 | 172 | 481 | same | 6/0 |
| 128 | `server.network.ServerGamePacketListenerImpl` | 101 | 102 | 224 | same | 21/2 |
| 129 | `world.level.block.state.StateHolder` | 99 | 99 | 498 | same | 5/1 |
| 130 | `network.codec.StreamCodec$CodecOperation` | 97 | 98 | 352 | same | 1/0 |
| 131 | `tags.BlockTags` | 96 | 133 | 350 | same | 95/3 |
| 132 | `network.codec.StreamEncoder` | 96 | 99 | 331 | same | 1/0 |
| 133 | `world.level.block.state.properties.EnumProperty` | 95 | 102 | 1121 | same | 7/2 |
| 134 | `world.effect.MobEffectInstance` | 94 | 97 | 859 | same | 32/5 |
| 135 | `core.HolderSet` | 94 | 106 | 318 | same | 16/0 |
| 136 | `nbt.NbtOps` | 94 | 98 | 260 | same | 7/0 |
| 137 | `server.players.PlayerList` | 93 | 95 | 239 | same | 22/2 |
| 138 | `network.protocol.game.ClientboundBlockEntityDataPacket` | 91 | 91 | 215 | same | 5/0 |
| 139 | `world.level.block.state.properties.IntegerProperty` | 90 | 91 | 562 | same | 7/1 |
| 140 | `core.component.DataComponentPatch` | 90 | 94 | 293 | same | 14/2 |
| 141 | `world.level.block.Rotation` | 88 | 91 | 795 | same | 17/0 |
| 142 | `core.RegistryAccess$Frozen` | 87 | 94 | 219 | same | 8/2 |
| 143 | `util.StringRepresentable$EnumCodec` | 83 | 86 | 316 | same | 11/0 |
| 144 | `world.entity.Mob` | 82 | 82 | 878 | same | 241/41 |
| 145 | `world.item.DyeColor` | 82 | 91 | 691 | same | 35/0 |
| 146 | `world.level.block.RenderShape` | 81 | 97 | 302 | same | 3/1 |
| 147 | `world.entity.player.Abilities` | 81 | 84 | 293 | same | 8/2 |
| 148 | `core.HolderLookup$RegistryLookup` | 81 | 92 | 212 | same | 9/0 |
| 149 | `world.entity.MobCategory` | 80 | 83 | 179 | same | 17/0 |
| 150 | `world.entity.ai.attributes.Attributes` | 79 | 84 | 673 | same | 30/0 |
| 151 | `world.item.crafting.CraftingInput` | 79 | 83 | 309 | same | 12/1 |
| 152 | `world.level.biome.Biome` | 79 | 87 | 308 | same | 18/6 |
| 153 | `resources.RegistryOps` | 78 | 80 | 240 | same | 12/1 |
| 154 | `world.level.storage.loot.parameters.LootContextParam` | 77 | 79 | 268 | GONE | 2/2 |
| 155 | `world.level.storage.loot.parameters.LootContextParams` | 77 | 79 | 255 | same | 12/12 |
| 156 | `core.Position` | 75 | 90 | 400 | same | 3/0 |
| 157 | `tags.ItemTags` | 74 | 110 | 162 | same | 65/10 |
| 158 | `util.profiling.ProfilerFiller` | 74 | 90 | 135 | same | 7/0 |
| 159 | `world.entity.ai.attributes.AttributeModifier` | 73 | 76 | 469 | same | 10/5 |
| 160 | `world.item.CreativeModeTabs` | 73 | 86 | 86 | same | 20/0 |
| 161 | `world.effect.MobEffect` | 72 | 79 | 438 | same | 18/5 |
| 162 | `core.particles.ParticleType` | 72 | 85 | 402 | same | 2/0 |
| 163 | `world.level.BlockAndTintGetter` | 71 | 110 | 422 | GONE | 12/12 |
| 164 | `stats.Stats` | 71 | 76 | 207 | same | 84/75 |
| 165 | `world.phys.HitResult$Type` | 70 | 91 | 277 | same | 6/0 |
| 166 | `world.item.Rarity` | 70 | 76 | 202 | same | 9/0 |
| 167 | `world.level.chunk.LevelChunk` | 70 | 75 | 201 | same | 38/6 |
| 168 | `world.level.gameevent.GameEvent` | 68 | 70 | 316 | same | 38/0 |
| 169 | `world.item.enchantment.Enchantment` | 68 | 75 | 298 | same | 28/3 |
| 170 | `world.inventory.InventoryMenu` | 68 | 101 | 176 | same | 17/6 |
| 171 | `server.packs.resources.PreparableReloadListener` | 68 | 93 | 111 | same | 3/1 |
| 172 | `world.effect.MobEffects` | 67 | 77 | 386 | same | 35/10 |
| 173 | `core.BlockPos$MutableBlockPos` | 67 | 77 | 321 | same | 37/1 |
| 174 | `server.level.ServerChunkCache` | 67 | 68 | 149 | same | 24/6 |
| 175 | `network.Connection` | 67 | 69 | 137 | same | 10/0 |
| 176 | `world.flag.FeatureFlagSet` | 67 | 118 | 113 | same | 4/0 |
| 177 | `world.level.storage.loot.LootParams` | 66 | 68 | 209 | same | 4/3 |
| 178 | `stats.StatType` | 66 | 72 | 180 | same | 5/0 |
| 179 | `world.level.block.Mirror` | 65 | 66 | 410 | same | 10/0 |
| 180 | `world.entity.ai.attributes.AttributeModifier$Operation` | 65 | 68 | 403 | same | 11/0 |
| 181 | `world.level.ClipContext` | 65 | 72 | 234 | same | 5/0 |
| 182 | `world.item.crafting.CraftingRecipe` | 65 | 69 | 229 | same | 15/8 |
| 183 | `world.level.storage.loot.LootParams$Builder` | 65 | 67 | 202 | same | 9/6 |
| 184 | `world.item.enchantment.EnchantmentHelper` | 65 | 65 | 195 | same | 42/2 |
| 185 | `world.level.chunk.ChunkAccess` | 65 | 70 | 183 | same | 37/6 |
| 186 | `util.FormattedCharSequence` | 65 | 109 | 178 | same | 5/0 |
| 187 | `world.entity.EntityType$EntityFactory` | 65 | 66 | 135 | same | 1/0 |
| 188 | `world.level.Explosion` | 64 | 65 | 220 | same | 29/22 |
| 189 | `core.UUIDUtil` | 64 | 65 | 203 | same | 7/0 |
| 190 | `world.entity.EntityType$Builder` | 64 | 64 | 90 | same | 18/2 |
| 191 | `world.level.ClipContext$Fluid` | 63 | 70 | 224 | same | 5/0 |
| 192 | `nbt.NbtUtils` | 63 | 66 | 202 | same | 11/4 |
| 193 | `stats.Stat` | 63 | 67 | 182 | same | 2/0 |
| 194 | `core.HolderSet$Named` | 63 | 78 | 138 | same | 10/0 |
| 195 | `world.damagesource.DamageSources` | 62 | 62 | 394 | same | 42/2 |
| 196 | `world.entity.ai.attributes.AttributeInstance` | 62 | 63 | 203 | same | 17/5 |
| 197 | `util.ExtraCodecs` | 61 | 63 | 215 | same | 25/1 |
| 198 | `core.GlobalPos` | 60 | 60 | 360 | same | 8/0 |
| 199 | `world.entity.Entity$RemovalReason` | 60 | 60 | 286 | same | 7/0 |
| 200 | `world.phys.Vec2` | 60 | 68 | 210 | same | 12/0 |

Top 40 `com.mojang.*` classes (Brigadier, DataFixerUpper, logging, authlib).

| # | Class | Mods (srv) | Mods (all) | Refs | 26.3 |
|--:|:--|--:|--:|--:|:--|
| 1 | `serialization.Codec` | 244 | 255 | 4114 | same |
| 2 | `serialization.MapCodec` | 220 | 230 | 3518 | same |
| 3 | `serialization.codecs.PrimitiveCodec` | 196 | 205 | 1428 | same |
| 4 | `datafixers.kinds.App` | 195 | 204 | 2299 | same |
| 5 | `datafixers.kinds.Applicative` | 195 | 204 | 2250 | same |
| 6 | `datafixers.Products` | 195 | 204 | 2246 | same |
| 7 | `serialization.codecs.RecordCodecBuilder` | 191 | 202 | 2166 | same |
| 8 | `serialization.codecs.RecordCodecBuilder$Instance` | 191 | 202 | 2153 | same |
| 9 | `serialization.DataResult` | 158 | 169 | 965 | same |
| 10 | `datafixers.util.Function3` | 156 | 159 | 727 | same |
| 11 | `serialization.DynamicOps` | 138 | 146 | 764 | same |
| 12 | `datafixers.Products$P2` | 132 | 134 | 765 | same |
| 13 | `datafixers.util.Pair` | 129 | 139 | 791 | same |
| 14 | `datafixers.Products$P3` | 125 | 129 | 478 | same |
| 15 | `datafixers.types.Type` | 125 | 128 | 148 | same |
| 16 | `brigadier.exceptions.CommandSyntaxException` | 124 | 130 | 531 | same |
| 17 | `brigadier.context.CommandContext` | 122 | 127 | 487 | same |
| 18 | `brigadier.Command` | 121 | 126 | 405 | same |
| 19 | `brigadier.builder.LiteralArgumentBuilder` | 120 | 127 | 458 | same |
| 20 | `brigadier.builder.ArgumentBuilder` | 120 | 126 | 440 | same |
| 21 | `brigadier.CommandDispatcher` | 120 | 128 | 292 | same |
| 22 | `brigadier.tree.LiteralCommandNode` | 118 | 125 | 179 | same |
| 23 | `datafixers.util.Function4` | 112 | 117 | 417 | same |
| 24 | `logging.LogUtils` | 107 | 126 | 192 | same |
| 25 | `brigadier.arguments.ArgumentType` | 105 | 109 | 349 | same |
| 26 | `brigadier.builder.RequiredArgumentBuilder` | 104 | 108 | 284 | same |
| 27 | `datafixers.Products$P1` | 102 | 111 | 453 | same |
| 28 | `datafixers.Products$P4` | 92 | 99 | 281 | same |
| 29 | `datafixers.util.Function5` | 90 | 95 | 229 | same |
| 30 | `authlib.GameProfile` | 89 | 92 | 343 | same |
| 31 | `datafixers.util.Either` | 78 | 88 | 339 | same |
| 32 | `datafixers.util.Function6` | 72 | 74 | 152 | same |
| 33 | `datafixers.Products$P5` | 68 | 75 | 158 | same |
| 34 | `math.Axis` | 67 | 139 | 296 | same |
| 35 | `brigadier.suggestion.SuggestionsBuilder` | 64 | 67 | 139 | same |
| 36 | `serialization.JsonOps` | 62 | 76 | 180 | same |
| 37 | `brigadier.arguments.StringArgumentType` | 58 | 62 | 102 | same |
| 38 | `datafixers.Products$P6` | 55 | 58 | 108 | same |
| 39 | `datafixers.util.Function7` | 51 | 52 | 128 | same |
| 40 | `brigadier.suggestion.SuggestionProvider` | 49 | 50 | 100 | same |

Third-party packages that most mods reference (three package segments, mod count): `org.jetbrains.annotations` 307, `com.google.common` 249, `org.slf4j.Logger` 235, `org.apache.commons` 201, `it.unimi.dsi` 196, `com.google.gson` 192, `org.apache.logging` 186, `org.joml.Matrix4f` 182, `org.joml.Quaternionf` 169, `io.netty.buffer` 144, `mezz.jei.api` 142, `org.slf4j.LoggerFactory` 129, `org.joml.Vector3f` 122, `org.lwjgl.glfw` 105, `com.llamalad7.mixinextras` 76, `org.objectweb.asm` 75, `org.apache.maven` 63, `top.theillusivec4.curios` 62, `dev.emi.emi` 59, `org.joml.Vector3fc` 58, `org.joml.Quaternionfc` 55, `snownee.jade.api` 52, `org.joml.Matrix4fc` 51, `org.lwjgl.opengl` 49.

## 5. Events subscribed

Sources: `@SubscribeEvent` methods (event class = the method parameter) and `IEventBus#addListener` calls (event class = the lambda or method reference parameter, or a `Class` argument). **Bus** is `mod` when the event implements `IModBusEvent` (checked in the 21.1.248 jars) and `game` otherwise. It agrees with the explicit `@EventBusSubscriber(bus = ...)` in all but 2 of 636 handlers that name a bus. **Mods (srv)** counts mods that subscribe in a server-side or common context. **Mods (client)** counts mods whose only subscriptions are `Dist.CLIENT` subscribers or client events (`neoforge.client`, `FMLClientSetupEvent`). Events in the datagen and game test packages are dev-only and listed in the dev line. **Ann** is the number of annotated handlers, **Lis** the number of `addListener` registrations, **RC** the number of handlers with `receiveCanceled = true`.

`@EventBusSubscriber` dist of the annotated handlers: both dists 1459, `Dist.CLIENT` only 987, `Dist.DEDICATED_SERVER` only 4. 1216 handlers sit in classes without the annotation (registered in code with `register(this)` or `addListener`). Priority: NORMAL 3320, HIGH 144, LOWEST 99, LOW 56, HIGHEST 47. `addListener` calls whose event class could not be resolved: 9.

Handlers by bus and dist. `addListener` registrations have no dist (the call is in common or client code).

| Bus | Dist of the subscriber | Handlers | Mods |
|:--|:--|--:|--:|
| game | addListener | 1174 | 167 |
| game | both | 1046 | 92 |
| game | code | 935 | 129 |
| game | client | 479 | 90 |
| game | server | 4 | 1 |
| mod | addListener | 1378 | 276 |
| mod | client | 508 | 124 |
| mod | both | 413 | 138 |
| mod | code | 281 | 60 |

| Event | Bus | Mods (srv) | Mods (client) | Ann | Lis | RC | 26.3 |
|:--|:--|--:|--:|--:|--:|--:|:--|
| `neoforge.network.event.RegisterPayloadHandlersEvent` | mod | 173 | 0 | 66 | 135 | 0 | same |
| `fml.event.lifecycle.FMLCommonSetupEvent` | mod | 165 | 0 | 57 | 158 | 0 | same |
| `neoforge.capabilities.RegisterCapabilitiesEvent` | mod | 133 | 0 | 92 | 133 | 0 | same |
| `neoforge.event.RegisterCommandsEvent` | game | 96 | 1 | 53 | 52 | 0 | same |
| `neoforge.event.BuildCreativeModeTabContentsEvent` | mod | 77 | 1 | 26 | 60 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedInEvent` | game | 69 | 4 | 60 | 32 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$RightClickBlock` | game | 63 | 3 | 74 | 34 | 0 | same |
| `neoforge.registries.RegisterEvent` | mod | 54 | 0 | 21 | 59 | 0 | same |
| `neoforge.event.entity.EntityJoinLevelEvent` | game | 52 | 3 | 56 | 21 | 0 | same |
| `fml.event.config.ModConfigEvent$Reloading` | mod | 51 | 1 | 41 | 22 | 0 | same |
| `neoforge.event.AddReloadListenerEvent` | game | 48 | 0 | 24 | 41 | 0 | GONE |
| `neoforge.event.entity.living.LivingDeathEvent` | game | 45 | 1 | 61 | 9 | 2 | same |
| `fml.event.config.ModConfigEvent$Loading` | mod | 45 | 1 | 40 | 16 | 0 | same |
| `neoforge.event.tick.ServerTickEvent$Post` | game | 44 | 0 | 41 | 12 | 0 | same |
| `neoforge.event.server.ServerStartedEvent` | game | 42 | 0 | 28 | 20 | 0 | same |
| `neoforge.event.level.BlockEvent$BreakEvent` | game | 37 | 0 | 34 | 13 | 0 | GONE |
| `neoforge.event.server.ServerStoppedEvent` | game | 36 | 0 | 23 | 29 | 0 | same |
| `fml.event.config.ModConfigEvent` | mod | 36 | 3 | 30 | 9 | 0 | same |
| `neoforge.event.entity.living.LivingIncomingDamageEvent` | game | 35 | 0 | 75 | 10 | 0 | same |
| `neoforge.event.tick.LevelTickEvent$Post` | game | 33 | 1 | 22 | 19 | 0 | same |
| `fml.event.lifecycle.InterModEnqueueEvent` | mod | 32 | 0 | 3 | 32 | 0 | same |
| `neoforge.event.entity.EntityAttributeCreationEvent` | mod | 32 | 0 | 17 | 18 | 0 | same |
| `neoforge.event.level.LevelEvent$Unload` | game | 31 | 7 | 35 | 44 | 0 | same |
| `neoforge.registries.NewRegistryEvent` | mod | 31 | 0 | 12 | 23 | 0 | same |
| `neoforge.event.entity.living.LivingDropsEvent` | game | 30 | 0 | 33 | 7 | 2 | same |
| `neoforge.event.OnDatapackSyncEvent` | game | 28 | 1 | 20 | 11 | 0 | same |
| `neoforge.event.server.ServerStartingEvent` | game | 28 | 0 | 23 | 8 | 0 | same |
| `neoforge.event.level.LevelEvent$Load` | game | 27 | 4 | 29 | 15 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedOutEvent` | game | 27 | 1 | 21 | 11 | 0 | same |
| `neoforge.event.server.ServerAboutToStartEvent` | game | 27 | 0 | 11 | 16 | 0 | same |
| `neoforge.event.entity.living.LivingDamageEvent$Pre` | game | 26 | 0 | 38 | 5 | 0 | same |
| `neoforge.event.tick.PlayerTickEvent$Post` | game | 26 | 5 | 36 | 6 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$LeftClickBlock` | game | 26 | 4 | 26 | 11 | 1 | same |
| `neoforge.event.entity.player.PlayerEvent$PlayerChangedDimensionEvent` | game | 26 | 0 | 22 | 7 | 0 | same |
| `fml.event.lifecycle.FMLLoadCompleteEvent` | mod | 25 | 3 | 10 | 20 | 0 | same |
| `neoforge.event.entity.living.LivingDamageEvent$Post` | game | 24 | 0 | 40 | 4 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$PlayerRespawnEvent` | game | 24 | 1 | 21 | 8 | 0 | same |
| `neoforge.event.server.ServerStoppingEvent` | game | 24 | 0 | 16 | 11 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$Clone` | game | 23 | 1 | 23 | 10 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$RightClickItem` | game | 21 | 1 | 20 | 3 | 0 | same |
| `neoforge.event.AddPackFindersEvent` | mod | 19 | 1 | 6 | 16 | 0 | same |
| `neoforge.event.tick.LevelTickEvent$Pre` | game | 18 | 2 | 14 | 10 | 0 | same |
| `neoforge.event.entity.EntityAttributeModificationEvent` | mod | 18 | 0 | 11 | 10 | 0 | same |
| `neoforge.registries.datamaps.RegisterDataMapTypesEvent` | mod | 18 | 0 | 11 | 9 | 0 | same |
| `neoforge.event.tick.EntityTickEvent$Post` | game | 17 | 0 | 15 | 8 | 0 | same |
| `neoforge.event.tick.PlayerTickEvent$Pre` | game | 17 | 2 | 19 | 4 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$BreakSpeed` | game | 17 | 0 | 18 | 3 | 0 | same |
| `neoforge.event.TagsUpdatedEvent` | game | 16 | 2 | 8 | 13 | 0 | same |
| `neoforge.event.entity.RegisterSpawnPlacementsEvent` | mod | 16 | 0 | 9 | 8 | 0 | same |
| `neoforge.event.entity.player.AttackEntityEvent` | game | 15 | 1 | 18 | 6 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$EntityInteract` | game | 15 | 0 | 16 | 2 | 0 | same |
| `neoforge.event.tick.ServerTickEvent$Pre` | game | 15 | 0 | 13 | 5 | 0 | same |
| `neoforge.event.entity.living.FinalizeSpawnEvent` | game | 15 | 0 | 11 | 5 | 0 | same |
| `neoforge.registries.DataPackRegistryEvent$NewRegistry` | mod | 15 | 0 | 5 | 11 | 0 | GONE |
| `neoforge.event.entity.player.ItemEntityPickupEvent$Pre` | game | 14 | 0 | 13 | 4 | 0 | same |
| `neoforge.event.entity.living.LivingChangeTargetEvent` | game | 14 | 0 | 14 | 2 | 0 | same |
| `neoforge.event.entity.living.LivingEvent$LivingJumpEvent` | game | 14 | 0 | 11 | 5 | 0 | same |
| `neoforge.event.level.BlockEvent$EntityPlaceEvent` | game | 14 | 0 | 14 | 2 | 0 | same |
| `fml.event.lifecycle.InterModProcessEvent` | mod | 14 | 0 | 2 | 12 | 0 | same |
| `neoforge.event.level.ChunkEvent$Unload` | game | 13 | 2 | 13 | 5 | 0 | same |
| `neoforge.event.entity.living.LivingHealEvent` | game | 13 | 0 | 14 | 2 | 0 | same |
| `neoforge.event.entity.living.LivingFallEvent` | game | 13 | 0 | 12 | 3 | 0 | same |
| `neoforge.event.level.BlockDropsEvent` | game | 13 | 0 | 13 | 1 | 0 | same |
| `neoforge.common.world.chunk.RegisterTicketControllersEvent` | mod | 13 | 0 | 3 | 10 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$StartTracking` | game | 12 | 0 | 13 | 3 | 0 | same |
| `neoforge.event.entity.living.LivingExperienceDropEvent` | game | 12 | 0 | 11 | 1 | 0 | same |
| `neoforge.event.village.VillagerTradesEvent` | game | 12 | 0 | 5 | 7 | 0 | GONE |
| `neoforge.event.entity.ProjectileImpactEvent` | game | 11 | 0 | 12 | 4 | 1 | same |
| `neoforge.event.ItemAttributeModifierEvent` | game | 11 | 0 | 9 | 4 | 0 | same |
| `neoforge.event.tick.EntityTickEvent$Pre` | game | 10 | 1 | 13 | 2 | 0 | same |
| `neoforge.event.entity.EntityLeaveLevelEvent` | game | 10 | 2 | 6 | 8 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$LeftClickEmpty` | game | 10 | 4 | 11 | 3 | 0 | same |
| `neoforge.event.level.ExplosionEvent$Detonate` | game | 10 | 0 | 8 | 2 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$EntityInteractSpecific` | game | 9 | 0 | 10 | 2 | 0 | GONE |
| `neoforge.event.level.ChunkEvent$Load` | game | 9 | 3 | 8 | 4 | 0 | same |
| `neoforge.event.AnvilUpdateEvent` | game | 9 | 0 | 6 | 3 | 0 | same |
| `neoforge.event.entity.EntityTeleportEvent` | game | 9 | 0 | 7 | 2 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$ItemCraftedEvent` | game | 9 | 0 | 4 | 5 | 0 | same |
| `neoforge.event.village.WandererTradesEvent` | game | 9 | 0 | 6 | 3 | 0 | GONE |
| `neoforge.event.entity.EntityInvulnerabilityCheckEvent` | game | 8 | 0 | 9 | 0 | 0 | same |
| `neoforge.event.entity.EntityMountEvent` | game | 8 | 1 | 8 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingEquipmentChangeEvent` | game | 8 | 0 | 7 | 2 | 0 | same |
| `neoforge.event.level.BlockEvent$BlockToolModificationEvent` | game | 8 | 0 | 4 | 5 | 0 | same |
| `neoforge.event.BlockEntityTypeAddBlocksEvent` | mod | 8 | 0 | 2 | 6 | 0 | same |
| `neoforge.event.entity.living.LivingKnockBackEvent` | game | 8 | 0 | 8 | 0 | 0 | same |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Finish` | game | 7 | 0 | 9 | 0 | 0 | same |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Start` | game | 7 | 0 | 8 | 1 | 0 | same |
| `neoforge.event.entity.player.AdvancementEvent$AdvancementEarnEvent` | game | 7 | 0 | 6 | 3 | 0 | same |
| `neoforge.event.entity.living.MobEffectEvent$Added` | game | 7 | 0 | 8 | 0 | 0 | same |
| `neoforge.event.entity.living.MobEffectEvent$Applicable` | game | 7 | 0 | 6 | 2 | 0 | same |
| `neoforge.event.ModifyDefaultComponentsEvent` | mod | 7 | 0 | 4 | 3 | 0 | same |
| `neoforge.event.brewing.RegisterBrewingRecipesEvent` | game | 7 | 0 | 6 | 1 | 0 | GONE |
| `neoforge.event.entity.item.ItemTossEvent` | game | 7 | 0 | 5 | 2 | 0 | same |
| `neoforge.event.entity.player.ItemEntityPickupEvent$Post` | game | 7 | 0 | 6 | 1 | 0 | same |
| `neoforge.event.ServerChatEvent` | game | 6 | 0 | 10 | 1 | 0 | same |
| `neoforge.event.level.ChunkWatchEvent$Sent` | game | 6 | 0 | 11 | 0 | 0 | same |
| `neoforge.event.entity.EntityTravelToDimensionEvent` | game | 6 | 1 | 4 | 4 | 0 | same |
| `neoforge.event.LootTableLoadEvent` | game | 6 | 0 | 5 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingShieldBlockEvent` | game | 6 | 0 | 6 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerContainerEvent$Open` | game | 6 | 0 | 4 | 2 | 0 | same |
| `neoforge.network.event.RegisterConfigurationTasksEvent` | mod | 6 | 0 | 2 | 4 | 0 | same |
| `neoforge.server.permission.events.PermissionGatherEvent$Nodes` | game | 6 | 0 | 2 | 4 | 0 | same |
| `fml.event.lifecycle.FMLConstructModEvent` | mod | 5 | 0 | 34 | 3 | 0 | same |
| `neoforge.event.entity.living.MobSpawnEvent$PositionCheck` | game | 5 | 0 | 6 | 0 | 0 | same |
| `neoforge.event.CommandEvent` | game | 5 | 0 | 3 | 2 | 0 | same |
| `neoforge.event.entity.EntityTeleportEvent$ChorusFruit` | game | 5 | 0 | 4 | 1 | 0 | GONE |
| `neoforge.event.entity.living.LivingBreatheEvent` | game | 5 | 0 | 5 | 0 | 0 | same |
| `neoforge.event.entity.living.MobEffectEvent$Remove` | game | 5 | 0 | 4 | 1 | 0 | same |
| `neoforge.event.entity.player.CriticalHitEvent` | game | 5 | 0 | 5 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$HarvestCheck` | game | 5 | 0 | 4 | 1 | 0 | same |
| `neoforge.event.level.ChunkDataEvent$Load` | game | 5 | 0 | 5 | 0 | 0 | same |
| `neoforge.event.level.LevelEvent$Save` | game | 5 | 0 | 3 | 2 | 0 | same |
| `bus.api.Event` | game | 4 | 0 | 0 | 4 | 0 | same |
| `neoforge.event.entity.EntityTeleportEvent$EnderPearl` | game | 4 | 0 | 3 | 1 | 0 | same |
| `neoforge.event.entity.living.MobEffectEvent$Expired` | game | 4 | 0 | 4 | 0 | 0 | same |
| `neoforge.event.entity.living.MobSpawnEvent$SpawnPlacementCheck` | game | 4 | 0 | 3 | 1 | 0 | same |
| `neoforge.event.level.ChunkDataEvent$Save` | game | 4 | 0 | 4 | 0 | 0 | same |
| `neoforge.event.level.ChunkWatchEvent$UnWatch` | game | 4 | 0 | 3 | 1 | 0 | same |
| `neoforge.event.level.ChunkWatchEvent$Watch` | game | 4 | 0 | 2 | 2 | 0 | same |
| `neoforge.event.level.ExplosionEvent$Start` | game | 4 | 0 | 4 | 0 | 0 | same |
| `neoforge.registries.datamaps.DataMapsUpdatedEvent` | game | 4 | 0 | 2 | 2 | 0 | same |
| `neoforge.event.entity.living.ArmorHurtEvent` | game | 3 | 0 | 2 | 2 | 0 | same |
| `neoforge.event.entity.player.PlayerInteractEvent$RightClickEmpty` | game | 3 | 1 | 4 | 0 | 0 | same |
| `neoforge.event.entity.player.UseItemOnBlockEvent` | game | 3 | 0 | 2 | 2 | 0 | same |
| `neoforge.event.level.ChunkTicketLevelUpdatedEvent` | game | 3 | 0 | 3 | 1 | 0 | same |
| `neoforge.event.RegisterGameTestsEvent` | mod | 3 | 0 | 2 | 1 | 0 | same |
| `neoforge.event.entity.EntityMobGriefingEvent` | game | 3 | 0 | 1 | 2 | 0 | same |
| `neoforge.event.entity.EntityTeleportEvent$EnderEntity` | game | 3 | 0 | 2 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingConversionEvent$Pre` | game | 3 | 0 | 1 | 2 | 0 | same |
| `neoforge.event.entity.living.LivingDestroyBlockEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.living.LivingEvent$LivingVisibilityEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.living.MobDespawnEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerContainerEvent$Close` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerDestroyItemEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$ItemSmeltedEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerSpawnPhantomsEvent` | game | 3 | 0 | 1 | 2 | 0 | same |
| `neoforge.event.furnace.FurnaceFuelBurnTimeEvent` | game | 3 | 0 | 1 | 2 | 0 | GONE |
| `neoforge.event.level.BlockEvent$FarmlandTrampleEvent` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.level.NoteBlockEvent$Play` | game | 3 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.village.VillageSiegeEvent` | game | 3 | 0 | 2 | 1 | 0 | same |
| `neoforge.common.world.poi.ExtendPoiTypesEvent` | mod | 2 | 0 | 0 | 3 | 0 | same |
| `neoforge.event.GameShuttingDownEvent` | game | 2 | 1 | 2 | 1 | 0 | same |
| `neoforge.event.entity.EntityEvent$Size` | game | 2 | 0 | 3 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$NameFormat` | game | 2 | 0 | 0 | 3 | 0 | same |
| `neoforge.event.level.ExplosionKnockbackEvent` | game | 2 | 0 | 2 | 1 | 0 | same |
| `neoforge.event.level.SleepFinishedTimeEvent` | game | 2 | 0 | 2 | 1 | 0 | same |
| `fml.event.lifecycle.FMLDedicatedServerSetupEvent` | mod | 2 | 0 | 0 | 2 | 0 | same |
| `neoforge.event.ItemStackedOnOtherEvent` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.enchanting.GetEnchantmentLevelEvent` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.EntityEvent$EntityConstructing` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.item.ItemExpireEvent` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.living.BabyEntitySpawnEvent` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.living.EnderManAngerEvent` | game | 2 | 0 | 2 | 0 | 0 | GONE |
| `neoforge.event.entity.living.LivingConversionEvent$Post` | game | 2 | 0 | 1 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingEntityUseItemEvent` | game | 2 | 0 | 1 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Stop` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Tick` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$SaveToFile` | game | 2 | 0 | 1 | 1 | 0 | same |
| `neoforge.event.entity.player.PlayerRespawnPositionEvent` | game | 2 | 0 | 1 | 1 | 0 | same |
| `neoforge.event.entity.player.PlayerXpEvent$PickupXp` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.level.BlockEvent$EntityMultiPlaceEvent` | game | 2 | 0 | 1 | 1 | 0 | same |
| `neoforge.event.level.BlockEvent$NeighborNotifyEvent` | game | 2 | 0 | 2 | 0 | 0 | same |
| `neoforge.event.level.block.CropGrowEvent$Post` | game | 2 | 0 | 1 | 1 | 0 | same |
| `net.minecraft.world.item.crafting.RecipeHolder` | game | 1 | 0 | 0 | 2 | 0 | same |
| `neoforge.event.PlayLevelSoundEvent$AtPosition` | game | 1 | 1 | 2 | 0 | 0 | same |
| `fml.event.config.ModConfigEvent$Unloading` | mod | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.GrindstoneEvent$OnPlaceItem` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.GrindstoneEvent$OnTakeItem` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.PlayLevelSoundEvent$AtEntity` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.StatAwardEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.EntityEvent$EnteringSection` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.EntityStruckByLightningEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.EntityTeleportEvent$TeleportCommand` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.entity.living.AnimalTameEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.living.LivingGetProjectileEvent` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.entity.living.LivingUseTotemEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.living.MobSplitEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.AdvancementEvent$AdvancementProgressEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.AnvilRepairEvent` | game | 1 | 0 | 1 | 0 | 0 | GONE |
| `neoforge.event.entity.player.ArrowLooseEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.ArrowNockEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.BonemealEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.CanPlayerSleepEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.ItemFishedEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerEvent$LoadFromFile` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.entity.player.PlayerXpEvent$XpChange` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.entity.player.TradeWithVillagerEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.level.BlockEvent$FluidPlaceBlockEvent` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.level.BlockGrowFeatureEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.level.LevelEvent$PotentialSpawns` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.level.ModifyCustomSpawnersEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.level.PistonEvent$Pre` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.level.block.CropGrowEvent$Pre` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.tick.LevelTickEvent` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.event.tick.PlayerTickEvent` | game | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.event.tick.ServerTickEvent` | game | 1 | 0 | 0 | 1 | 0 | same |
| `neoforge.fluids.RegisterCauldronFluidContentEvent` | mod | 1 | 0 | 1 | 0 | 0 | same |
| `neoforge.registries.IdMappingEvent` | game | 1 | 0 | 0 | 1 | 0 | GONE |
| `neoforge.registries.ModifyRegistriesEvent` | mod | 1 | 0 | 0 | 1 | 0 | same |

Client events (106 classes, 300 mods): FMLClientSetupEvent(178), RegisterMenuScreensEvent(116), EntityRenderersEvent.RegisterRenderers(100), RegisterKeyMappingsEvent(82), ClientTickEvent.Post(75), RenderLevelStageEvent(71), ItemTooltipEvent(61), RegisterColorHandlersEvent.Item(58), RegisterParticleProvidersEvent(53), RegisterClientExtensionsEvent(43), ClientPlayerNetworkEvent.LoggingOut(43), RegisterColorHandlersEvent.Block(40), RegisterClientReloadListenersEvent(39), InputEvent.Key(39), RenderHighlightEvent.Block(38), RegisterGuiLayersEvent(38), RegisterClientCommandsEvent(36), ClientTickEvent.Pre(34), ModelEvent.RegisterGeometryLoaders(33), EntityRenderersEvent.RegisterLayerDefinitions(33), RegisterClientTooltipComponentFactoriesEvent(32), ClientPlayerNetworkEvent.LoggingIn(30), ModelEvent.RegisterAdditional(29), EntityRenderersEvent.AddLayers(26), RegisterShadersEvent(25), InputEvent.MouseScrollingEvent(20), ScreenEvent.Init.Post(19), RecipesUpdatedEvent(18), ModelEvent.ModifyBakingResult(16), ScreenEvent.Render.Post(16), ScreenEvent.Opening(14), RenderFrameEvent.Pre(14), TextureAtlasStitchedEvent(14), ScreenEvent.MouseButtonPressed.Pre(13), ModelEvent.BakingCompleted(13), RenderGuiLayerEvent.Post(12), CustomizeGuiOverlayEvent.DebugText(12), RenderTooltipEvent.GatherComponents(12), RenderGuiEvent.Post(12), ViewportEvent.RenderFog(12), RenderGuiLayerEvent.Pre(12), InputEvent.MouseButton.Pre(11), RenderHandEvent(11), RenderLivingEvent.Pre(11), RenderLivingEvent.Post(10), ScreenEvent.KeyPressed.Pre(10), InputEvent.InteractionKeyMappingTriggered(10), ComputeFovModifierEvent(10), ScreenEvent.Render.Pre(9), MovementInputUpdateEvent(9), ....

Dev events: GatherDataEvent(124).

Mod-defined event classes that mods subscribe to (events of other mods and own events): 115 classes in 66 mods. Mods that call `IEventBus#post` to fire events: 94 (section 11, row `IEventBus#post`).

## 6. Registries

Registry usage comes from three bytecode patterns: `DeferredRegister` fields and their `register*` call sites (the registry is read from the field type or the `create` call), `Registry.register(BuiltInRegistries.X, ...)` call sites, and `RegisterEvent#register(Registries.X, ...)` call sites. **Touch** is the number of mods that read the registry field (`Registries.X`, `BuiltInRegistries.X`, `NeoForgeRegistries.X`), for registration or lookup. Call sites are not entries: a loop or a helper makes one call site register many entries, so the counts are a lower bound. 370 `DeferredRegister` call sites could not be tied to a registry (receiver not a static field) and are not in the table.

| Registry | Touch | Mods registering | DeferredRegister mods | DR call sites | Registry.register mods | Calls | RegisterEvent mods | Calls |
|:--|--:|--:|--:|--:|--:|--:|--:|--:|
| item | 235 | 132 | 121 | 2969 | 7 | 16 | 8 | 8 |
| data_component_type | 138 | 113 | 104 | 851 | 6 | 6 | 3 | 4 |
| creative_mode_tab | 151 | 112 | 103 | 221 | 2 | 2 | 7 | 8 |
| block | 201 | 98 | 92 | 1564 | 3 | 3 | 4 | 4 |
| block_entity_type | 143 | 90 | 86 | 558 | 2 | 2 | 2 | 2 |
| menu | 113 | 78 | 71 | 366 | 4 | 52 | 3 | 3 |
| recipe_serializer | 98 | 74 | 69 | 343 | 4 | 7 | 2 | 2 |
| sound_event | 92 | 55 | 49 | 453 | 1 | 1 | 5 | 5 |
| attachment_type | 66 | 54 | 54 | 293 | 0 | 0 | 0 | 0 |
| recipe_type | 72 | 49 | 46 | 192 | 3 | 6 | 1 | 1 |
| entity_type | 126 | 46 | 46 | 466 | 0 | 0 | 0 | 0 |
| particle_type | 49 | 41 | 40 | 413 | 1 | 3 | 1 | 2 |
| global_loot_modifier_serializer | 27 | 22 | 22 | 58 | 0 | 0 | 0 | 0 |
| command_argument_type | 28 | 21 | 20 | 50 | 0 | 0 | 1 | 1 |
| mob_effect | 57 | 19 | 19 | 197 | 0 | 0 | 0 | 0 |
| trigger_type | 28 | 19 | 14 | 47 | 3 | 3 | 2 | 2 |
| fluid | 81 | 16 | 15 | 63 | 1 | 1 | 0 | 0 |
| fluid_type | 29 | 16 | 15 | 56 | 0 | 0 | 1 | 1 |
| loot_function_type | 29 | 16 | 13 | 59 | 2 | 2 | 2 | 2 |
| condition_codec | 20 | 16 | 15 | 31 | 0 | 0 | 1 | 1 |
| ingredient_type | 18 | 14 | 12 | 28 | 0 | 0 | 2 | 2 |
| attribute | 41 | 13 | 13 | 36 | 0 | 0 | 0 | 0 |
| feature | 17 | 13 | 13 | 81 | 0 | 0 | 0 | 0 |
| armor_material | 25 | 11 | 11 | 51 | 0 | 0 | 0 | 0 |
| biome_modifier_serializer | 11 | 10 | 8 | 18 | 1 | 1 | 1 | 1 |
| point_of_interest_type | 19 | 9 | 9 | 24 | 0 | 0 | 0 | 0 |
| entity_data_serializer | 14 | 9 | 9 | 29 | 0 | 0 | 0 | 0 |
| condition_serializer | 11 | 9 | 7 | 15 | 2 | 2 | 0 | 0 |
| placement_modifier_type | 13 | 8 | 8 | 17 | 0 | 0 | 0 | 0 |
| structure_type | 12 | 8 | 8 | 23 | 0 | 0 | 0 | 0 |
| loot_condition_type | 16 | 7 | 7 | 25 | 0 | 0 | 0 | 0 |
| structure_processor | 14 | 7 | 7 | 21 | 0 | 0 | 0 | 0 |
| loot_pool_entry_type | 13 | 7 | 5 | 8 | 0 | 0 | 2 | 2 |
| villager_profession | 19 | 6 | 6 | 34 | 0 | 0 | 0 | 0 |
| structure_piece | 9 | 6 | 6 | 13 | 0 | 0 | 0 | 0 |
| potion | 29 | 5 | 5 | 35 | 0 | 0 | 0 | 0 |
| custom_stat | 16 | 5 | 4 | 8 | 0 | 0 | 1 | 1 |
| item_sub_predicate_type | 11 | 5 | 4 | 8 | 0 | 0 | 1 | 1 |
| chunk_generator | 5 | 4 | 3 | 5 | 1 | 1 | 0 | 0 |
| enchantment_effect_component_type | 9 | 3 | 3 | 4 | 0 | 0 | 0 | 0 |
| loot_number_provider_type | 8 | 3 | 2 | 9 | 1 | 1 | 0 | 0 |
| structure_placement | 7 | 3 | 3 | 6 | 0 | 0 | 0 | 0 |
| map_decoration_type | 5 | 2 | 2 | 4 | 0 | 0 | 0 | 0 |
| trunk_placer_type | 5 | 2 | 2 | 5 | 0 | 0 | 0 | 0 |
| memory_module_type | 5 | 2 | 2 | 20 | 0 | 0 | 0 | 0 |
| sensor_type | 5 | 2 | 2 | 7 | 0 | 0 | 0 | 0 |
| height_provider_type | 5 | 2 | 2 | 4 | 0 | 0 | 0 | 0 |
| structure_pool_element | 5 | 2 | 2 | 3 | 0 | 0 | 0 | 0 |
| enchantment | 60 | 1 | 1 | 1 | 0 | 0 | 0 | 0 |
| damage_type | 45 | 1 | 1 | 1 | 0 | 0 | 0 | 0 |
| registry | 28 | 1 | 0 | 0 | 1 | 1 | 0 | 0 |
| placed_feature | 11 | 1 | 1 | 1 | 0 | 0 | 0 | 0 |
| configured_feature | 9 | 1 | 1 | 1 | 0 | 0 | 0 | 0 |
| entity_sub_predicate_type | 4 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| block_type | 4 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |
| rule_test | 4 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| int_provider_type | 4 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |
| foliage_placer_type | 4 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| tree_decorator_type | 4 | 1 | 1 | 5 | 0 | 0 | 0 | 0 |
| enchantment_entity_effect_type | 3 | 1 | 1 | 4 | 0 | 0 | 0 | 0 |
| blockstate_provider_type | 3 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| carver | 3 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |
| fluid_ingredient_type | 3 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |
| block_state_provider_type | 2 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| biome_source | 2 | 1 | 0 | 0 | 1 | 1 | 0 | 0 |
| structure_modifier_serializer | 2 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| density_function_type | 1 | 1 | 1 | 2 | 0 | 0 | 0 | 0 |
| holder_set_type | 1 | 1 | 1 | 3 | 0 | 0 | 0 | 0 |
| dimension | 55 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| biome | 51 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| loot_table | 39 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| structure | 29 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| dimension_type | 19 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| jukebox_song | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| template_pool | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| structure_set | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| banner_pattern | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| painting_variant | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| stat_type | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| villager_type | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| level_stem | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| game_event | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| processor_list | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| wolf_variant | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

Registries that no vanilla or NeoForge registry field names (registries that mods define themselves, or generic type labels): 65 labels in 37 mods, for example `fanprocessingtype`(3), `camocontainerfactory`(3), `guielementtype`(3), `machinecomponenttype`(3), `requirementtype`(3), `arminteractionpointtype`(2), `itemattributetype`(2), `datatype`(2), `fdmodelinfo`(2), `jsonconfig`(2), `animation`(2), `screeneffecttype`(2).

Entry counts from resources, over all mods (files in `assets/<mod>` and `data/<mod>`; overrides in `minecraft`, `neoforge` and `c` namespaces are not counted). As in #35, `blockstates/*.json` is a block count, `models/item/*.json` an item count, and `block.*` and `item.*` language keys are a cross-check. The counts rest on complete resources.

| Resource | Count | Mods with it |
|:--|--:|--:|
| Block states files (blocks) | 23804 | 162 |
| Item model files (items) | 32115 | 208 |
| Block model files | 42716 | 159 |
| Language keys (`en_us.json`, all) | 86446 | 335 |
| `block.*` language keys | 22426 | 156 |
| `item.*` language keys | 9528 | 187 |
| `entity.*` language keys | 835 | 45 |
| `effect.*` language keys | 379 | 25 |
| `enchantment.*` language keys | 638 | 17 |
| `fluid_type.*` language keys | 118 | 16 |
| Recipes (`data/*/recipe`) | 33018 | 188 |
| Recipes (`data/*/recipes`, old path) | 23 | 10 |
| Loot tables (`loot_table`) | 16727 | 152 |
| Loot tables (`loot_tables`, old path) | 8 | 4 |
| Advancements (`advancement`) | 19757 | 128 |
| Advancements (`advancements`, old path) | 19 | 7 |
| Texture files | 54753 | 285 |
| Sound files | 2251 | 71 |
| Tag files (`data/*/tags/**`) | 4060 | 160 |
| Worldgen JSON files (`worldgen/**`) | 889 | 30 |
| NeoForge data files (`neoforge/**`, mod namespace) | 152 | 31 |

Worldgen files by kind: configured_feature 270, placed_feature 264, template_pool 181, structure 64, structure_set 51, biome 33, processor_list 16, density_function 4, configured_carver 3, noise_settings 3.

Top 20 mods by registered blocks and items (block states files and item model files; the `DeferredRegister` call sites for block and item registries are the cross-check).

| Mod | Block states files | Item model files | DR block calls | DR item calls | `block.*` keys | `item.*` keys |
|:--|--:|--:|--:|--:|--:|--:|
| chipped | 6981 | 6993 | 0 | 0 | 6967 | 6 |
| rechiseled | 3627 | 3628 | 0 | 0 | 2418 | 0 |
| chisel | 2880 | 2886 | 3 | 9 | 2880 | 6 |
| betterblockz | 1500 | 1513 | 3 | 16 | 1683 | 48 |
| enderio | 825 | 944 | 12 | 9 | 212 | 128 |
| twilightforest | 523 | 983 | 157 | 261 | 666 | 292 |
| create | 643 | 776 | 0 | 0 | 701 | 204 |
| immersiveengineering | 390 | 711 | 3 | 6 | 846 | 314 |
| malum | 326 | 669 | 327 | 2 | 322 | 244 |
| ftbmaterials | 177 | 690 | 2 | 3 | 177 | 690 |
| cabletiers | 388 | 404 | 0 | 0 | 28 | 5 |
| refinedstorage | 352 | 427 | 0 | 0 | 38 | 122 |
| occultism | 164 | 596 | 2 | 382 | 167 | 833 |
| xycraft_world | 332 | 330 | 0 | 0 | 313 | 8 |
| ars_nouveau | 200 | 447 | 13 | 15 | 198 | 247 |
| mekanism | 201 | 402 | 0 | 0 | 200 | 168 |
| supplementaries | 277 | 302 | 0 | 0 | 258 | 44 |
| irregular_implements | 191 | 360 | 0 | 4 | 190 | 104 |
| oritech | 211 | 293 | 0 | 0 | 203 | 134 |
| ae2 | 102 | 378 | 2 | 3 | 102 | 262 |

Other registration-like call sites:

| What | Mods | Call sites |
|:--|--:|--:|
| Brigadier commands (`Commands.literal`) | 122 | 1411 |
| `CommandDispatcher.register` | 124 | 239 |
| Custom game rules (`GameRules.register`) | 5 | 9 |
| `Registry.register` (direct) | 33 | 159 |

## 7. Capabilities, attachments, data maps, config specs, payloads, loot modifiers, biome modifiers

Each row counts mods (server-side or mixed classes) and, where it makes sense, the call sites, classes or files. Row labels say which bytecode or resource pattern was counted. Mods that use NeoForge 21.1 names are counted under the 21.1 name.

### 7.1 Capabilities

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| any class of `neoforge.capabilities` | 169 | - |  |
| built-in `Capabilities.ItemHandler.BLOCK` | 107 | - |  |
| built-in `Capabilities.ItemHandler.ENTITY` | 18 | - |  |
| built-in `Capabilities.ItemHandler.ITEM` | 22 | - |  |
| built-in `Capabilities.FluidHandler.BLOCK` | 70 | - |  |
| built-in `Capabilities.FluidHandler.ENTITY` | 4 | - |  |
| built-in `Capabilities.FluidHandler.ITEM` | 42 | - |  |
| built-in `Capabilities.EnergyStorage.BLOCK` | 60 | - |  |
| built-in `Capabilities.EnergyStorage.ENTITY` | 5 | - |  |
| built-in `Capabilities.EnergyStorage.ITEM` | 47 | - |  |
| `RegisterCapabilitiesEvent.registerBlock` | 42 | 147 | call sites |
| `RegisterCapabilitiesEvent.registerBlockEntity` | 103 | 493 | call sites |
| `RegisterCapabilitiesEvent.registerEntity` | 16 | 32 | call sites |
| `RegisterCapabilitiesEvent.registerItem` | 57 | 177 | call sites |
| custom capability type: `BlockCapability.create*` | 37 | 90 | call sites |
| custom capability type: `EntityCapability.create*` | 18 | 30 | call sites |
| custom capability type: `ItemCapability.create*` | 24 | 60 | call sites |
| `BlockCapabilityCache.create` | 34 | 114 | call sites |
| classes implementing `ICapabilityProvider` | 5 | 7 | classes |
| calls `getCapability(BlockCapability, ...)` on a level | 95 | - |  |

### 7.2 Attachments

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| any class of `neoforge.attachment` | 70 | - |  |
| `AttachmentType.builder` (attachment type definitions) | 50 | 203 | call sites |
| `AttachmentType.Builder.serialize` (persisted) | 47 | 174 | call sites |
| `AttachmentType.Builder.sync` | 10 | 19 | call sites |
| `AttachmentType.Builder.copyOnDeath` | 25 | 37 | call sites |
| calls `getData(AttachmentType or Supplier)` on a holder | 54 | - |  |
| calls `setData(AttachmentType or Supplier)` on a holder | 39 | - |  |
| calls `hasData(AttachmentType or Supplier)` on a holder | 13 | - |  |
| calls `removeData(AttachmentType or Supplier)` on a holder | 8 | - |  |
| classes implementing `IAttachmentHolder` | 2 | 2 | classes |

### 7.3 Data maps

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| any class of `registries.datamaps` | 28 | - |  |
| `DataMapType.builder` (data map type definitions) | 18 | 63 | call sites |
| `AdvancedDataMapType` (any call) | 2 | 19 | call sites |
| `RegisterDataMapTypesEvent` | 16 | - |  |
| calls `getData(DataMapType)` on a holder or registry | 18 | - |  |
| JSON files under `data/*/data_maps` or `neoforge/data_maps` | 36 | 100 | files |

### 7.4 Config specs

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| `ModConfigSpec` (class referenced) | 182 | - |  |
| `ModConfigSpec.Builder.define*` options | 182 | 3577 | call sites |
| `ModContainer.registerConfig` | 185 | 304 | call sites |
| `ModConfig.Type.COMMON` | 102 | - |  |
| `ModConfig.Type.CLIENT` | 77 | - |  |
| `ModConfig.Type.SERVER` | 75 | - |  |
| `ModConfig.Type.STARTUP` | 17 | - |  |
| `ModContainer.registerExtensionPoint` (config screen) | 72 | 75 | call sites |

### 7.5 Payloads

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| `PayloadRegistrar.playToServer` | 122 | 524 | call sites |
| `PayloadRegistrar.playToClient` | 125 | 476 | call sites |
| `PayloadRegistrar.playBidirectional` | 33 | 59 | call sites |
| `PayloadRegistrar.configurationToServer` | 1 | 1 | call sites |
| `PayloadRegistrar.configurationToClient` | 6 | 6 | call sites |
| `PayloadRegistrar.configurationBidirectional` | 0 | 0 | call sites |
| `PayloadRegistrar.commonToServer` | 2 | 3 | call sites |
| `PayloadRegistrar.commonToClient` | 1 | 1 | call sites |
| `PayloadRegistrar.commonBidirectional` | 3 | 3 | call sites |
| `PayloadRegistrar.versioned` | 58 | 67 | call sites |
| `PayloadRegistrar.optional` | 45 | 59 | call sites |
| classes implementing `CustomPacketPayload` | 188 | 1120 | classes |
| `IPayloadContext.enqueueWork` | 113 | - |  |
| `IPayloadContext.reply` | 5 | - |  |
| `IPayloadContext.player` | 154 | - |  |
| `IPayloadContext.connection` | 5 | - |  |
| `IPayloadContext.flow` | 41 | - |  |
| `IPayloadContext.listener` | 1 | - |  |
| `IPayloadContext.disconnect` | 11 | - |  |
| `IPayloadContext.finishCurrentTask` | 0 | - |  |
| `PacketDistributor.sendToServer` | 103 | - |  |
| `PacketDistributor.sendToPlayer` | 121 | - |  |
| `RegisterConfigurationTasksEvent` / `ICustomConfigurationTask` | 7 | - |  |

### 7.6 Loot modifiers

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| classes extending `LootModifier` | 25 | 40 | classes |
| classes implementing `IGlobalLootModifier` | 0 | 0 | classes |
| any class of `common.loot` | 30 | - |  |
| JSON files under `data/<mod>/loot_modifiers` | 20 | 170 | files |
| uses the `GLOBAL_LOOT_MODIFIER_SERIALIZERS` registry | 27 | - |  |

### 7.7 Biome modifiers

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| classes implementing `BiomeModifier` | 13 | 14 | classes |
| any class of `common.world` | 35 | - |  |
| JSON files under `data/<mod>/neoforge/biome_modifier` | 31 | 150 | files |
| JSON files under `data/<mod>/neoforge/structure_modifier` | 1 | 1 | files (structure modifiers) |
| uses the `BIOME_MODIFIER_SERIALIZERS` or `STRUCTURE_MODIFIER_SERIALIZERS` registry | 14 | - |  |

### 7.8 Other

| Pattern | Mods | Count | Unit |
|:--|--:|--:|:--|
| classes extending `FluidType` | 24 | 36 | classes |
| classes extending `BaseFlowingFluid` | 7 | 17 | classes |
| `IMenuTypeExtension.create` (menu with extra data) | 79 | 231 | call sites |
| classes implementing `IItemHandler` | 45 | 78 | classes |
| classes implementing `IFluidHandler` | 38 | 68 | classes |
| classes implementing `IEnergyStorage` | 38 | 81 | classes |
| classes implementing `INBTSerializable` | 38 | 115 | classes |

## 8. Mixins

A mixin into vanilla code has no direct port: it patches bytecode of the Java server. Section 9 sizes the native primitives that replace it. The numbers show how much of the pack depends on it. This section counts mixin classes; section 9 gives their members and target methods. Mixin classes are the classes with `@Mixin`. Targets come from `@Mixin(value = ..., targets = ...)`. A target is **vanilla** when it is in `net.minecraft` or `com.mojang`, **NeoForge** in `net.neoforged`, **other mod** when the package belongs to another jar of the pack, **own** when it belongs to the same jar, and **unknown** otherwise (libraries not in the pack). A mixin counts as **client** when it is listed in the `client` array of its mixin config or targets `net.minecraft.client`/`blaze3d`.

Totals: 3508 mixin classes in 259 of 414 mods. Config side: common 1931, client 1539, server 5, not listed in any config 33. Mixin targets (a class can have several): vanilla server-side 1488, vanilla client 1137, NeoForge 92, other mod 631, own mod 65, unknown 134, JDK or none 0. 108 mixin configs name a mixin plugin class. 174 mods ship an access transformer (4718 lines in total).

Annotations inside mixin classes (methods): Inject 3102, Accessor 818, Unique 711, Shadow 529, WrapOperation 288, Redirect 273, Invoker 242, ModifyExpressionValue 163, ModifyReturnValue 134, ModifyVariable 122, ModifyArg 108, Overwrite 102, WrapWithCondition 69, Mutable 57, WrapMethod 55, ModifyConstant 34.

Mods by mixin count: 0: 155, 1-5: 141, 6-20: 77, 21-50: 27, 51+: 14.

Most targeted vanilla server-side classes (mods that target the class, mixin classes):

| # | Vanilla class | Mods | Mixin classes |
|--:|:--|--:|--:|
| 1 | `world.entity.Entity` | 43 | 63 |
| 2 | `world.entity.LivingEntity` | 42 | 66 |
| 3 | `world.entity.player.Player` | 30 | 38 |
| 4 | `world.item.ItemStack` | 23 | 26 |
| 5 | `server.MinecraftServer` | 17 | 20 |
| 6 | `server.level.ServerLevel` | 16 | 18 |
| 7 | `world.item.crafting.RecipeManager` | 15 | 18 |
| 8 | `world.level.Level` | 15 | 17 |
| 9 | `world.level.block.state.BlockBehaviour$BlockStateBase` | 15 | 17 |
| 10 | `world.entity.item.ItemEntity` | 13 | 16 |
| 11 | `server.level.ServerPlayer` | 13 | 15 |
| 12 | `server.network.ServerGamePacketListenerImpl` | 13 | 14 |
| 13 | `world.entity.Mob` | 12 | 14 |
| 14 | `world.item.Item` | 12 | 13 |
| 15 | `server.players.PlayerList` | 11 | 14 |
| 16 | `world.level.block.state.BlockBehaviour` | 11 | 12 |
| 17 | `world.level.block.Block` | 11 | 11 |
| 18 | `world.level.levelgen.structure.templatesystem.StructureTemplate` | 10 | 14 |
| 19 | `world.level.chunk.LevelChunk` | 10 | 10 |
| 20 | `world.level.block.entity.BlockEntity` | 9 | 12 |
| 21 | `server.ReloadableServerResources` | 9 | 9 |
| 22 | `world.inventory.AbstractContainerMenu` | 9 | 9 |
| 23 | `world.entity.npc.Villager` | 8 | 10 |
| 24 | `server.level.ChunkMap` | 8 | 9 |
| 25 | `world.item.BlockItem` | 7 | 8 |
| 26 | `world.item.crafting.Ingredient` | 6 | 9 |
| 27 | `world.item.enchantment.EnchantmentHelper` | 6 | 7 |
| 28 | `world.level.Explosion` | 6 | 7 |
| 29 | `world.entity.EntityType` | 6 | 6 |
| 30 | `world.level.biome.Biome` | 6 | 6 |
| 31 | `world.level.block.DispenserBlock` | 6 | 6 |
| 32 | `world.level.block.entity.BlockEntityType` | 6 | 6 |
| 33 | `world.level.chunk.ChunkGenerator` | 5 | 8 |
| 34 | `world.level.block.FireBlock` | 5 | 7 |
| 35 | `world.level.material.FlowingFluid` | 5 | 7 |
| 36 | `world.level.storage.loot.LootTable` | 5 | 6 |
| 37 | `core.MappedRegistry` | 5 | 5 |
| 38 | `server.level.ServerPlayerGameMode` | 5 | 5 |
| 39 | `tags.TagLoader` | 5 | 5 |
| 40 | `world.entity.LightningBolt` | 5 | 5 |
| 41 | `world.entity.item.FallingBlockEntity` | 5 | 5 |
| 42 | `world.entity.monster.Zombie` | 5 | 5 |
| 43 | `world.level.BaseSpawner` | 5 | 5 |
| 44 | `world.level.block.Blocks` | 5 | 5 |
| 45 | `world.level.block.FarmBlock` | 5 | 5 |
| 46 | `world.level.storage.loot.functions.EnchantedCountIncreaseFunction` | 5 | 5 |
| 47 | `world.level.storage.loot.LootPool` | 4 | 5 |
| 48 | `network.chat.TextColor` | 4 | 4 |
| 49 | `util.datafix.schemas.V1460` | 4 | 4 |
| 50 | `world.entity.decoration.ItemFrame` | 4 | 4 |
| 51 | `world.entity.npc.WanderingTrader` | 4 | 4 |
| 52 | `world.entity.player.Inventory` | 4 | 4 |
| 53 | `world.inventory.AnvilMenu` | 4 | 4 |
| 54 | `world.item.CreativeModeTab` | 4 | 4 |
| 55 | `world.item.alchemy.PotionBrewing` | 4 | 4 |
| 56 | `world.level.biome.BiomeManager` | 4 | 4 |
| 57 | `world.level.block.BushBlock` | 4 | 4 |
| 58 | `world.level.block.CropBlock` | 4 | 4 |
| 59 | `world.level.lighting.LightEngine` | 4 | 4 |
| 60 | `world.level.saveddata.maps.MapItemSavedData` | 4 | 4 |

NeoForge targets: `neoforge.client.ClientHooks`(6), `neoforge.registries.GameData`(4), `neoforge.client.loading.NeoForgeLoadingOverlay`(3), `neoforge.data.loading.DatagenModLoader`(3), `neoforge.common.BasicItemListing`(2), `neoforge.common.loot.CanItemPerformAbility`(2), `neoforge.resource.ContextAwareReloadListener`(2), `neoforge.server.ServerLifecycleHooks`(2), `neoforge.common.extensions.IItemExtension`(2), `neoforge.items.ItemStackHandler`(2), `neoforge.capabilities.CapabilityHooks`(2), `neoforge.capabilities.RegisterCapabilitiesEvent`(2), `neoforge.resource.ResourcePackLoader`(2), `neoforge.common.extensions.IBlockStateExtension`(2), `neoforge.common.loot.AddTableLootModifier`(1), `neoforge.common.loot.LootModifier`(1), `neoforge.common.loot.LootTableIdCondition`(1), `neoforge.common.NeoForgeEventHandler`(1), `neoforge.common.world.chunk.ForcedChunkManager$TicketOwner`(1), `neoforge.network.handlers.ClientPayloadHandler`(1), `neoforge.common.util.FakePlayerFactory`(1), `neoforge.common.util.FakePlayer`(1), `neoforge.registries.RegistryManager`(1), `neoforge.registries.holdersets.AnyHolderSet`(1), `neoforge.common.data.GlobalLootModifierProvider`(1).

Other-mod targets (the 25 most targeted classes of other mods): `com.hollingsworth.arsnouveau.setup.registry.Documentation`(4), `com.hollingsworth.arsnouveau.api.spell.SpellSchool`(4), `com.hollingsworth.arsnouveau.common.block.tile.BasicSpellTurretTile`(4), `com.hollingsworth.arsnouveau.common.entity.EntityOrbitProjectile`(4), `com.simibubi.create.content.contraptions.Contraption`(4), `dev.latvian.mods.kubejs.server.ServerScriptManager`(3), `com.hollingsworth.arsnouveau.common.block.tile.RelayTile`(3), `fr.frinn.custommachinery.common.integration.kubejs.CustomCraftRecipeBuilderJS`(3), `fr.frinn.custommachinery.common.integration.kubejs.CustomMachineRecipeBuilderJS`(3), `appeng.hooks.BuiltInModelHooks`(2), `appeng.helpers.patternprovider.PatternProviderLogic`(2), `dev.latvian.mods.rhino.ContextFactory`(2), `dev.latvian.mods.kubejs.recipe.CachedTagLookup$1`(2), `software.bernie.geckolib.renderer.GeoArmorRenderer`(2), `dev.latvian.mods.kubejs.recipe.component.RecipeComponentBuilder$1`(2), `dev.latvian.mods.kubejs.recipe.component.RecipeComponentBuilder$Value`(2), `dev.latvian.mods.kubejs.recipe.component.RecipeComponentBuilder`(2), `dev.latvian.mods.kubejs.recipe.component.RegistryComponent`(2), `com.hollingsworth.arsnouveau.api.spell.SpellResolver`(2), `com.hollingsworth.arsnouveau.common.spell.effect.EffectCut`(2), `com.hollingsworth.arsnouveau.common.block.tile.SourcelinkTile`(2), `com.hollingsworth.arsnouveau.common.block.tile.SourceJarTile`(2), `com.hollingsworth.arsnouveau.common.block.tile.RelaySplitterTile`(2), `com.hollingsworth.arsnouveau.common.block.tile.ImbuementTile`(2), `com.hollingsworth.arsnouveau.common.block.tile.WixieCauldronTile`(2).

Per mod: only mods with at least one mixin class (259 rows). Server mixins are the mixin classes that are not client.

| Mod | Mixins | Server | Client | Vanilla | Vanilla client | NeoForge | Other mod | Own | Unknown | Configs | Injectors and accessors |
|:--|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| iris | 179 | 5 | 174 | 13 | 136 | 0 | 26 | 5 | 0 | 17 | 423 |
| modernfix | 156 | 88 | 68 | 90 | 53 | 9 | 3 | 0 | 5 | 1 | 265 |
| alltheleaks | 123 | 62 | 61 | 30 | 6 | 5 | 42 | 0 | 42 | 2 | 168 |
| sodium | 112 | 5 | 107 | 12 | 93 | 5 | 0 | 2 | 0 | 8 | 198 |
| supplementaries | 112 | 78 | 34 | 69 | 28 | 0 | 3 | 11 | 3 | 3 | 189 |
| fancymenu | 102 | 3 | 99 | 12 | 88 | 1 | 0 | 0 | 1 | 2 | 544 |
| create | 101 | 46 | 55 | 55 | 42 | 2 | 0 | 0 | 2 | 6 | 164 |
| kubejs | 89 | 70 | 19 | 57 | 16 | 15 | 0 | 0 | 2 | 2 | 224 |
| collective | 80 | 56 | 24 | 68 | 12 | 0 | 0 | 0 | 0 | 4 | 132 |
| moonlight | 77 | 53 | 24 | 49 | 18 | 3 | 0 | 7 | 0 | 4 | 164 |
| kubejstweaks | 69 | 60 | 9 | 2 | 0 | 0 | 61 | 0 | 6 | 4 | 107 |
| servercore | 60 | 60 | 0 | 59 | 0 | 0 | 0 | 0 | 1 | 1 | 95 |
| amendments | 56 | 43 | 13 | 40 | 10 | 0 | 7 | 1 | 0 | 2 | 86 |
| create_connected | 54 | 51 | 3 | 12 | 0 | 0 | 40 | 0 | 4 | 1 | 89 |
| mekanism_ponders | 49 | 13 | 36 | 12 | 26 | 0 | 9 | 2 | 0 | 6 | 67 |
| lychee | 48 | 40 | 8 | 39 | 7 | 0 | 1 | 0 | 1 | 2 | 74 |
| ars_nouveau | 46 | 33 | 13 | 34 | 12 | 0 | 0 | 0 | 0 | 3 | 98 |
| immersiveengineering | 46 | 31 | 15 | 31 | 15 | 0 | 0 | 0 | 0 | 1 | 68 |
| artifacts | 42 | 36 | 6 | 36 | 4 | 0 | 0 | 1 | 1 | 2 | 67 |
| fusion | 40 | 5 | 35 | 5 | 25 | 5 | 3 | 0 | 2 | 1 | 59 |
| kiwi | 39 | 22 | 17 | 20 | 16 | 3 | 0 | 0 | 1 | 1 | 69 |
| immediatelyfast | 37 | 0 | 37 | 1 | 27 | 3 | 3 | 0 | 3 | 2 | 72 |
| create_dragons_plus | 36 | 35 | 1 | 5 | 0 | 1 | 20 | 4 | 21 | 5 | 46 |
| productivemetalworks | 36 | 4 | 32 | 12 | 24 | 0 | 0 | 0 | 0 | 5 | 49 |
| bookshelf | 35 | 27 | 8 | 27 | 8 | 0 | 0 | 0 | 0 | 2 | 50 |
| structurify | 35 | 25 | 10 | 21 | 2 | 0 | 9 | 0 | 3 | 2 | 114 |
| irons_spellbooks | 33 | 22 | 11 | 20 | 10 | 1 | 1 | 1 | 0 | 1 | 57 |
| simplyswords | 32 | 18 | 14 | 18 | 11 | 0 | 0 | 0 | 3 | 2 | 102 |
| ars_elemental | 31 | 23 | 8 | 7 | 1 | 0 | 23 | 0 | 0 | 2 | 41 |
| particular | 30 | 0 | 30 | 16 | 11 | 0 | 0 | 0 | 3 | 4 | 43 |
| create_central_kitchen | 26 | 21 | 5 | 1 | 0 | 0 | 19 | 3 | 3 | 6 | 48 |
| morejs | 26 | 24 | 2 | 22 | 2 | 2 | 0 | 0 | 0 | 1 | 30 |
| irregular_implements | 25 | 18 | 7 | 18 | 6 | 0 | 1 | 0 | 0 | 1 | 48 |
| malum | 25 | 14 | 11 | 14 | 10 | 1 | 0 | 0 | 0 | 1 | 43 |
| create_enchantment_industry | 24 | 24 | 0 | 4 | 0 | 0 | 14 | 5 | 1 | 5 | 37 |
| iceberg | 23 | 3 | 20 | 4 | 11 | 1 | 1 | 0 | 6 | 2 | 46 |
| cataclysm | 23 | 20 | 3 | 20 | 3 | 0 | 0 | 0 | 0 | 1 | 37 |
| apotheosis | 22 | 17 | 5 | 14 | 5 | 2 | 0 | 0 | 1 | 1 | 45 |
| balm | 21 | 10 | 11 | 10 | 11 | 0 | 0 | 0 | 0 | 4 | 35 |
| euphoria_patcher | 21 | 8 | 13 | 0 | 8 | 0 | 13 | 0 | 0 | 1 | 115 |
| nbtac | 21 | 0 | 21 | 14 | 7 | 0 | 0 | 0 | 0 | 1 | 45 |
| apothic_enchanting | 20 | 17 | 3 | 14 | 3 | 1 | 1 | 0 | 1 | 1 | 24 |
| architectury | 20 | 17 | 3 | 16 | 2 | 1 | 0 | 1 | 0 | 2 | 13 |
| ferritecore | 20 | 11 | 9 | 11 | 9 | 0 | 0 | 0 | 0 | 10 | 55 |
| lootjs | 20 | 20 | 0 | 18 | 0 | 2 | 0 | 0 | 0 | 1 | 20 |
| puzzleslib | 20 | 15 | 5 | 12 | 5 | 2 | 0 | 1 | 0 | 2 | 46 |
| create_pattern_schematics | 19 | 19 | 0 | 1 | 0 | 0 | 18 | 0 | 0 | 1 | 56 |
| psi | 19 | 7 | 12 | 7 | 12 | 0 | 0 | 0 | 0 | 2 | 27 |
| relics | 19 | 12 | 7 | 9 | 7 | 1 | 2 | 0 | 0 | 1 | 36 |
| cabletiers | 18 | 14 | 4 | 0 | 0 | 0 | 18 | 0 | 0 | 1 | 40 |
| drippyloadingscreen | 18 | 0 | 18 | 0 | 3 | 1 | 14 | 0 | 0 | 2 | 66 |
| structureessentials | 18 | 18 | 0 | 17 | 0 | 1 | 0 | 0 | 0 | 1 | 29 |
| watut | 18 | 4 | 14 | 4 | 14 | 0 | 0 | 0 | 0 | 2 | 24 |
| playeranimator | 17 | 0 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 1 | 35 |
| starbunclemania | 17 | 10 | 7 | 2 | 0 | 0 | 15 | 0 | 0 | 2 | 26 |
| advanced_ae | 16 | 12 | 4 | 1 | 1 | 0 | 9 | 5 | 0 | 1 | 50 |
| ae2 | 16 | 8 | 8 | 8 | 7 | 0 | 1 | 0 | 0 | 1 | 22 |
| ftbpc | 16 | 10 | 6 | 9 | 5 | 0 | 1 | 0 | 1 | 1 | 23 |
| createenchantablemachinery | 15 | 15 | 0 | 3 | 0 | 0 | 12 | 0 | 0 | 1 | 43 |
| lodestone | 15 | 3 | 12 | 3 | 12 | 0 | 0 | 0 | 0 | 1 | 33 |
| supermartijn642corelib | 15 | 7 | 8 | 7 | 4 | 4 | 0 | 0 | 0 | 1 | 22 |
| villagerconfig | 15 | 14 | 1 | 14 | 1 | 0 | 0 | 0 | 0 | 2 | 31 |
| ars_affinity | 14 | 9 | 5 | 5 | 3 | 0 | 5 | 1 | 0 | 4 | 17 |
| clavis | 14 | 12 | 2 | 12 | 2 | 0 | 0 | 0 | 0 | 2 | 26 |
| entityjs | 14 | 11 | 3 | 6 | 3 | 1 | 4 | 0 | 0 | 1 | 186 |
| extendedae | 14 | 6 | 8 | 0 | 4 | 0 | 10 | 0 | 0 | 1 | 16 |
| farmersdelight | 14 | 12 | 2 | 12 | 2 | 0 | 0 | 0 | 0 | 1 | 19 |
| respawningstructures | 14 | 14 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 1 | 15 |
| stepcrafter | 14 | 11 | 3 | 0 | 0 | 0 | 14 | 0 | 0 | 1 | 27 |
| create_hypertube | 13 | 6 | 7 | 6 | 5 | 1 | 0 | 0 | 1 | 3 | 25 |
| curios | 13 | 13 | 0 | 9 | 0 | 0 | 0 | 4 | 0 | 2 | 36 |
| fdlib | 13 | 0 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 1 | 23 |
| tooltipoverhaul | 13 | 0 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 2 | 19 |
| ali | 12 | 11 | 1 | 2 | 1 | 6 | 2 | 0 | 1 | 4 | 18 |
| almostunified | 12 | 11 | 1 | 9 | 1 | 2 | 0 | 0 | 0 | 2 | 12 |
| framedblocks | 12 | 8 | 4 | 8 | 4 | 0 | 0 | 0 | 0 | 1 | 21 |
| ftbchunks | 12 | 9 | 3 | 10 | 2 | 0 | 0 | 0 | 0 | 1 | 14 |
| konkrete | 12 | 0 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 2 | 27 |
| mekanismcurios | 11 | 11 | 0 | 1 | 0 | 0 | 10 | 0 | 0 | 1 | 15 |
| octolib | 11 | 0 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 1 | 24 |
| rarcompat | 11 | 8 | 3 | 4 | 2 | 0 | 5 | 0 | 0 | 1 | 19 |
| underlay | 11 | 6 | 5 | 2 | 2 | 0 | 4 | 0 | 3 | 4 | 26 |
| fdbosses | 10 | 6 | 4 | 6 | 3 | 0 | 1 | 0 | 0 | 1 | 11 |
| mekanism_unleashed | 10 | 10 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 1 | 23 |
| patchouli | 10 | 2 | 8 | 3 | 7 | 0 | 0 | 0 | 0 | 1 | 16 |
| reliquified_ars_nouveau | 10 | 9 | 1 | 5 | 1 | 0 | 4 | 0 | 0 | 1 | 13 |
| universalgrid | 10 | 7 | 3 | 0 | 0 | 0 | 10 | 0 | 0 | 1 | 17 |
| entityculling | 9 | 0 | 9 | 3 | 7 | 0 | 0 | 0 | 0 | 1 | 9 |
| ftbteambases | 9 | 8 | 1 | 8 | 1 | 0 | 0 | 0 | 0 | 1 | 12 |
| pipezretriever | 9 | 8 | 1 | 0 | 0 | 0 | 9 | 0 | 0 | 1 | 16 |
| rechiseled | 9 | 5 | 4 | 5 | 2 | 0 | 2 | 0 | 0 | 1 | 13 |
| apothic_attributes | 8 | 7 | 1 | 6 | 1 | 1 | 0 | 0 | 0 | 1 | 19 |
| appflux | 8 | 6 | 2 | 0 | 0 | 0 | 7 | 0 | 1 | 1 | 16 |
| baguettelib | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 1 | 13 |
| custommachinerycreate | 8 | 8 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 1 | 9 |
| irons_lib | 8 | 2 | 6 | 5 | 3 | 0 | 0 | 0 | 0 | 1 | 9 |
| megacells | 8 | 6 | 2 | 1 | 0 | 0 | 7 | 0 | 0 | 1 | 13 |
| reliquified_twilight_forest | 8 | 5 | 3 | 2 | 2 | 0 | 4 | 0 | 0 | 1 | 9 |
| xycraft_core | 8 | 6 | 2 | 6 | 2 | 0 | 0 | 0 | 0 | 1 | 16 |
| authme | 7 | 0 | 7 | 2 | 5 | 0 | 0 | 0 | 0 | 2 | 25 |
| cable_facades | 7 | 3 | 4 | 4 | 2 | 0 | 1 | 0 | 0 | 3 | 20 |
| createaddition | 7 | 7 | 0 | 4 | 0 | 0 | 2 | 0 | 1 | 1 | 12 |
| geckolib | 7 | 3 | 4 | 3 | 4 | 0 | 0 | 0 | 0 | 1 | 13 |
| omnitools | 7 | 4 | 3 | 1 | 0 | 0 | 5 | 1 | 0 | 1 | 7 |
| oritechthings | 7 | 6 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 1 | 34 |
| rep_ae2_bridge | 7 | 5 | 2 | 2 | 1 | 0 | 4 | 0 | 0 | 1 | 15 |
| rgp_client | 7 | 2 | 5 | 1 | 5 | 0 | 1 | 0 | 0 | 1 | 10 |
| yet_another_config_lib_v3 | 7 | 0 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 2 | 10 |
| ars_hex | 6 | 6 | 0 | 2 | 0 | 0 | 4 | 0 | 0 | 2 | 9 |
| computercraft | 6 | 4 | 2 | 4 | 2 | 0 | 0 | 0 | 0 | 3 | 9 |
| create_shimmer | 6 | 6 | 0 | 1 | 0 | 0 | 5 | 0 | 0 | 1 | 8 |
| dummmmmmy | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 2 | 10 |
| inventoryessentials | 6 | 0 | 6 | 1 | 5 | 0 | 0 | 0 | 0 | 2 | 11 |
| jade | 6 | 3 | 3 | 2 | 3 | 1 | 0 | 0 | 0 | 1 | 8 |
| kubejs_curios | 6 | 6 | 0 | 1 | 0 | 0 | 5 | 0 | 0 | 1 | 4 |
| nerb | 6 | 3 | 3 | 3 | 3 | 0 | 0 | 0 | 0 | 1 | 9 |
| oritech | 6 | 3 | 3 | 3 | 2 | 0 | 0 | 5 | 0 | 2 | 7 |
| placebo | 6 | 4 | 2 | 4 | 2 | 0 | 0 | 0 | 0 | 1 | 4 |
| ae2wtlib | 5 | 3 | 2 | 2 | 2 | 0 | 1 | 0 | 0 | 1 | 6 |
| ars_unification | 5 | 5 | 0 | 1 | 0 | 0 | 3 | 0 | 1 | 1 | 5 |
| bcc | 5 | 2 | 3 | 2 | 3 | 0 | 0 | 0 | 0 | 1 | 4 |
| brandonscore | 5 | 3 | 2 | 3 | 2 | 0 | 0 | 0 | 0 | 1 | 5 |
| classicperipherals | 5 | 5 | 0 | 2 | 0 | 0 | 3 | 0 | 0 | 2 | 7 |
| codechickenlib | 5 | 0 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 1 | 6 |
| collapsible_groups | 5 | 0 | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 2 | 14 |
| controlling | 5 | 0 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 2 | 15 |
| cupboard | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 1 | 15 |
| derenderpatcher | 5 | 0 | 5 | 0 | 0 | 0 | 5 | 0 | 0 | 1 | 15 |
| draconicevolution | 5 | 3 | 2 | 3 | 2 | 0 | 0 | 0 | 0 | 1 | 7 |
| enderio | 5 | 3 | 2 | 3 | 2 | 0 | 0 | 0 | 0 | 1 | 8 |
| ftblibrary | 5 | 4 | 1 | 4 | 1 | 0 | 0 | 0 | 0 | 1 | 9 |
| gag | 5 | 3 | 2 | 3 | 2 | 0 | 0 | 0 | 0 | 2 | 3 |
| idlecinematics | 5 | 0 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 1 | 8 |
| justdirefuels | 5 | 5 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 1 | 12 |
| justzoom | 5 | 0 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 2 | 9 |
| kubejs_enderio | 5 | 5 | 0 | 3 | 0 | 0 | 2 | 0 | 0 | 1 | 6 |
| modernworldcreation | 5 | 0 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 2 | 16 |
| replication_rs2_bridge | 5 | 3 | 2 | 0 | 1 | 0 | 4 | 0 | 0 | 1 | 13 |
| statuseffectbars | 5 | 3 | 2 | 2 | 2 | 0 | 0 | 0 | 1 | 1 | 12 |
| structures_tweaker | 5 | 5 | 0 | 3 | 0 | 1 | 1 | 0 | 0 | 2 | 5 |
| utilitarian | 5 | 4 | 1 | 4 | 1 | 0 | 0 | 0 | 0 | 1 | 11 |
| avaritia | 4 | 2 | 2 | 1 | 2 | 0 | 0 | 1 | 0 | 2 | 4 |
| caelus | 4 | 2 | 2 | 1 | 2 | 0 | 0 | 1 | 0 | 1 | 5 |
| chisel | 4 | 3 | 1 | 1 | 1 | 0 | 2 | 0 | 0 | 1 | 4 |
| compactmachines | 4 | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 3 | 5 |
| crash_assistant | 4 | 1 | 3 | 1 | 2 | 1 | 0 | 0 | 0 | 2 | 7 |
| cucumber | 4 | 3 | 1 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 6 |
| custommachineryars | 4 | 4 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 1 | 10 |
| custommachinerymekanism | 4 | 4 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 1 | 1 |
| emojiful | 4 | 0 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 1 | 10 |
| enderdrives | 4 | 4 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 1 | 18 |
| fadingnightvision | 4 | 0 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 2 | 4 |
| fastitemframes | 4 | 3 | 1 | 3 | 1 | 0 | 0 | 0 | 0 | 2 | 6 |
| irons_jewelry | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 4 |
| jei_mekanism_multiblocks | 4 | 0 | 4 | 0 | 1 | 0 | 2 | 0 | 1 | 2 | 5 |
| mffs | 4 | 3 | 1 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 4 |
| occultengineering | 4 | 4 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 1 | 4 |
| resourcefulconfig | 4 | 3 | 1 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 5 |
| shield_api | 4 | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 1 | 7 |
| sophisticatedcore | 4 | 3 | 1 | 2 | 0 | 1 | 1 | 0 | 0 | 2 | 6 |
| xycraft_machines | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 1 | 8 |
| actuallyadditions | 3 | 2 | 1 | 2 | 1 | 0 | 0 | 0 | 0 | 1 | 3 |
| apothic_compats | 3 | 3 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 1 | 0 |
| arseng | 3 | 3 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 1 | 5 |
| athena | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 2 | 3 |
| colorfulhearts | 3 | 0 | 3 | 0 | 1 | 0 | 2 | 0 | 0 | 2 | 4 |
| entangled | 3 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 1 | 3 |
| extremesoundmuffler | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 16 |
| ftbpromoter | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 11 |
| industrialforegoing | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 1 | 3 |
| irons_apothic | 3 | 3 | 0 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 10 |
| justdirethings | 3 | 2 | 1 | 2 | 1 | 0 | 0 | 0 | 0 | 1 | 5 |
| kubejs_actuallyadditions | 3 | 3 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 1 | 4 |
| lmft | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 2 | 5 |
| melody | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 2 | 4 |
| merequester | 3 | 0 | 3 | 1 | 1 | 0 | 1 | 0 | 0 | 1 | 6 |
| polylib | 3 | 1 | 2 | 1 | 1 | 0 | 0 | 1 | 0 | 2 | 3 |
| refinedstorage | 3 | 1 | 2 | 1 | 2 | 0 | 0 | 0 | 0 | 1 | 5 |
| renderjs | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 1 | 3 |
| structureexpansion | 3 | 2 | 1 | 2 | 1 | 0 | 0 | 0 | 0 | 1 | 4 |
| synesthesia | 3 | 0 | 3 | 0 | 2 | 0 | 1 | 0 | 0 | 1 | 7 |
| tipsmod | 3 | 0 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 2 | 3 |
| trade_cycling | 3 | 2 | 1 | 2 | 1 | 0 | 0 | 0 | 0 | 1 | 5 |
| yeetusexperimentus | 3 | 0 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 2 | 4 |
| ae2ct | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 4 |
| apothic_spawners | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 3 |
| appmek | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 2 |
| ars_creo | 2 | 1 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 3 |
| beer | 2 | 2 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 4 |
| betteradvancedtooltips | 2 | 0 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| bhmenu | 2 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | 8 |
| busy_villagers | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 6 |
| chipped | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 2 |
| cmpreviewfixer | 2 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 3 |
| colorfulallays | 2 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 11 |
| enchdesc | 2 | 0 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 4 |
| ftbmaterials | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 2 |
| ftbobsidian | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 2 |
| ftbquests | 2 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| ftbstuff | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 1 | 2 |
| ftbxmodcompat | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 2 |
| glassential | 2 | 0 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 1 | 2 |
| lionfishapi | 2 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| morered | 2 | 0 | 2 | 0 | 1 | 1 | 0 | 0 | 0 | 1 | 7 |
| oracle_index | 2 | 0 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| pickupnotifier | 2 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 2 | 2 |
| rechiseled_chipped | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 3 |
| routers | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 1 | 2 |
| schematicenergistics | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 13 |
| simplytooltips | 2 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 1 | 12 |
| sophisticateditemactions | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | 2 |
| tempad | 2 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 2 | 4 |
| torchmaster | 2 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 2 | 2 |
| aaron | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| ae2things | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 2 |
| ae2netanalyser | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 2 |
| appex | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 3 |
| attributefix | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 4 |
| bonsaitrees4 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| bowinfinityfix | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| ctm | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| dsp | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 1 |
| deus_ex_machina | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| disconnect_packet_fix | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| energymeter | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| entityguardian | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| farmingforblockheads | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 2 |
| ftbauxilium | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 |
| ftbessentials | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| ftbpmapi | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| ftbranks | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| ftbultimine | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| ftbunearthed | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| ftboceanmobs | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| gateways | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| guideme | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 1 |
| hostilenetworks | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 2 |
| ibicf | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| justhammers | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 2 |
| justdynathings | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 |
| leaderboards | 1 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 |
| magiccoinstweak | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 1 | 1 |
| mekanisticrouters | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 2 |
| mob_grinding_utils | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| modularrouters | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| moofluids | 1 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 1 | 1 |
| moreoverlays | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| mousetweaks | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 2 | 6 |
| refinedtypes | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 |
| reliquified_lenders_cataclysm | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | 1 |
| repeatable_trial_vaults | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 1 |
| resourcefullib | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 |
| rsinfinitybooster | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | 1 |
| searchables | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 2 | 2 |
| sophisticatedbackpacks | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| squatgrow | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| sfm | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 1 |
| toofast | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 |
| woodenshears | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 3 |

## 9. Mixin targets by method

This section sizes the two primitives of the native mod channel (the NeoForge-shaped API and compile-time source patches) by the members of the mixin classes, not by the classes. Section 8 counts the mixin classes; this section reads every member of every mixin class and its target member.

### 9.1 Method

The scanner is `tools/modpack-scan/mixin_scan.py` (its README gives the command). It reads the 414 jars of the folder and their Jar-in-Jar jars with `zipfile` and its own class file parser, and it does not load or run code from the jars. A class is a mixin class when it carries `@Mixin`. The scanner reads `RuntimeInvisibleAnnotations` and `RuntimeVisibleAnnotations` of the class, its fields and its methods, with all element values. The mixin configs are the JSON files at the root of a jar that have a `package` and a `mixins`, `client` or `server` list. 0 class files with the `@Mixin` descriptor did not parse.

Read errors: 0 jars or entries were skipped because they are larger than the size limit or do not decompress.

For each member the scanner records the annotation, the target class (from `value` and `targets` of `@Mixin`), the target member, the `@At` value and target, `cancellable`, and the mixin plugin class of the config. The target member comes from `method` of the injector (a selector with or without descriptor; overloads merge by name), from `value` of `@Accessor` and `@Invoker` or from the method name without the `get`, `set`, `is`, `call` or `invoke` prefix, and from the member name for `@Shadow` and `@Overwrite`. Names are Mojang names: NeoForge runs Mojang mappings in production.

Scope: members of mixin classes listed in the `mixins` (common) or `server` list of a config that the jar declares (in `[[mixins]]` of its `mods.toml` or in `MixinConfigs` of its manifest), with a vanilla target (`net.minecraft` or `com.mojang`) outside `net.minecraft.client` and `com.mojang.blaze3d`. `@Unique` members add a new member to the target and target no member: they are counted apart. `@Mutable` is a modifier of `@Shadow` and is not a member. A member with two target classes or two selectors counts once, and it needs all its target members.

Intent of a member:

- **pre**: `@Inject` at `HEAD`, not cancellable.
- **post**: `@Inject` at `RETURN` or `TAIL`, not cancellable.
- **mid**: `@Inject` at a point inside the method (`INVOKE`, `FIELD`, `NEW` and others), not cancellable.
- **cancel**: `@Inject` with `cancellable = true` at any point, and `@WrapMethod` (it can skip the original method).
- **value**: a value modifier: `@Redirect`, `@WrapOperation`, `@ModifyArg`, `@ModifyArgs`, `@ModifyConstant`, `@ModifyExpressionValue`, `@ModifyVariable`, `@ModifyReturnValue`, `@ModifyReceiver`, `@WrapWithCondition`.
- **overwrite**: `@Overwrite`, a whole-method override.
- **accessor**: `@Accessor`, `@Invoker`, `@Shadow`.

The analysis groups the intents in four classes: accessor (accessor), hook (pre, post, cancel), call site (mid, value) and override (overwrite). The classes describe what the mixin does. The two primitives of the native channel serve them as follows:

- **accessor**: a source patch, or the NeoForge-shaped API when it exposes the field or method.
- **hook**: a NeoForge event when one fires on that method, otherwise a source patch.
- **call site**: a source patch.
- **override**: a source patch.

### 9.2 Totals

| Measure | Value |
|:--|--:|
| Mixin members, all sides and targets (`@Unique` included) | 9183 |
| Members of common or server mixin classes of declared configs (`@Unique` included) | 4490 |
| Server-side members with a vanilla target (the scope) | 2674 |
| Mods with at least one member in the scope | 157 of 414 |
| Distinct target members in the scope (methods, and fields for accessors) | 1931 |
| Vanilla target classes in the scope | 519 |
| Members with more than one target member | 27 |
| `@Unique` members in common or server mixin classes with a vanilla target | 443 |
| Mods in the scope whose config names a mixin plugin | 33 |
| Configs with mixin members that the jar does not declare | 8 |
| Common or server members with a vanilla target in those configs (left out) | 92 |

The mixins section counts 165 mods with a common or server mixin class into vanilla server code; this scope counts 157, because the other 8 mods have only mixin classes with no members in the scope or mixin classes in configs that the jar does not declare. A config that the jar does not declare does not load through the jar metadata. 4 of the 8 have `fabric` in the file name: Fabric configs that multi-loader jars carry, some with Fabric intermediary names (`net.minecraft.class_1309`). 92 of the 92 members left out are in them. A mixin plugin or mod code can still add a config at run time; the scan does not follow code.

Members of common or server mixin classes of declared configs by target kind (`@Unique` left out): vanilla 2674, vanilla-client 11, neoforge 103, other 881, own 66, unknown 117.

Members in the scope by intent: pre 157 (6%), post 282 (11%), mid 123 (5%), cancel 560 (21%), value 445 (17%), overwrite 21 (1%), accessor 1086 (41%).

Cancel members by point: at `HEAD` 331, at `RETURN` or `TAIL` 144, at another point 59, `@WrapMethod` 26.

Members in the scope by annotation: Inject 1096, Shadow 681, Accessor 304, ModifyReturnValue 102, Invoker 101, WrapOperation 90, Redirect 83, ModifyExpressionValue 63, ModifyVariable 39, ModifyArg 29, WrapMethod 26, Overwrite 21, ModifyConstant 20, WrapWithCondition 15, ModifyReceiver 4.

### 9.3 Coverage curve

A set of target members covers a member when the set holds all target members of that member. It covers a mod when it covers every server-side member of the mod with a vanilla target (157 mods). Mixins into NeoForge and into other mods are out of this count (section 9.6). Three orders of the target members:

- **by members**: target members ordered by the number of mixin members, highest first.
- **by mods**: the order of the table in section 9.4.
- **cheapest mod first**: at each step the set adds all target members of the mod that needs the fewest new ones. This order covers the most mods for a number of target members.

The rows `all` count all members. The rows `not accessors` count the members of the hook, call site and override classes only. A mod with accessor members only is covered at 0 in those rows.

Target members needed to cover a share of the members or of the 157 mods:

| Members counted | Order | Covers | Total | 50% | 80% | 90% | 100% |
|:--|:--|:--|--:|--:|--:|--:|--:|
| all | by members | members | 2674 | 570 | 1385 | 1662 | 1931 |
| all | by mods | members | 2674 | 570 | 1385 | 1662 | 1931 |
| all | cheapest mod first | members | 2674 | 801 | 1440 | 1689 | 1931 |
| all | by members | mods | 157 | 1302 | 1717 | 1848 | 1931 |
| all | by mods | mods | 157 | 1302 | 1717 | 1848 | 1931 |
| all | cheapest mod first | mods | 157 | 162 | 526 | 910 | 1931 |
| not accessors | by members | members | 1588 | 300 | 786 | 959 | 1120 |
| not accessors | by mods | members | 1588 | 301 | 786 | 959 | 1120 |
| not accessors | cheapest mod first | members | 1588 | 464 | 826 | 961 | 1120 |
| not accessors | by members | mods | 157 | 762 | 1014 | 1094 | 1120 |
| not accessors | by mods | mods | 157 | 763 | 1016 | 1094 | 1120 |
| not accessors | cheapest mod first | mods | 157 | 97 | 329 | 526 | 1120 |

The 200 target members of the table in section 9.4 cover 700 of 2674 members (26%) and 17 of 157 mods (11%).

With accessors left out, the same 200 target members cover 475 of 1588 members that are not accessors (30%) and 33 of 157 mods (21%).

By class: the vanilla classes ordered by members, highest first, cover 50%, 80%, 90% and 100% of the members with 42, 179, 291, 519 classes.

### 9.4 Target members by number of mods

The 200 most targeted vanilla server-side members, by mods, then by mixin members. Class names drop the `net.minecraft.` prefix. **Desc** is the number of distinct descriptors that selectors name (0: the selectors give the name only). The intent columns count members (section 9.1). **Call targets** are the most common `@At` targets of `@Redirect` and `@WrapOperation`, with their count.

| # | Target member | Mods | Members | Desc | pre | post | mid | cancel | value | overwrite | accessor | Call targets |
|--:|:--|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|:--|
| 1 | `world.item.crafting.RecipeManager.apply` | 11 | 17 | 1 | 7 | 4 | 3 | 0 | 3 | 0 | 0 | `Logger.error` 1 |
| 2 | `world.entity.LivingEntity.travel` | 9 | 10 | 1 | 0 | 0 | 1 | 1 | 8 | 0 | 0 | `BlockState.getFriction` 2, `LivingEntity.isInFluidType` 1 |
| 3 | `world.entity.LivingEntity.hurt` | 6 | 9 | 1 | 2 | 2 | 2 | 2 | 1 | 0 | 0 | - |
| 4 | `server.network.ServerGamePacketListenerImpl.player (field)` | 6 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 7 | - |
| 5 | `world.entity.Entity.isAlliedTo` | 6 | 6 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 1 | - |
| 6 | `world.entity.Entity.move` | 6 | 6 | 1 | 0 | 0 | 2 | 1 | 3 | 0 | 0 | - |
| 7 | `world.entity.LivingEntity.canAttack` | 6 | 6 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 0 | - |
| 8 | `world.level.block.state.BlockBehaviour$BlockStateBase.asState` | 6 | 6 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 6 | - |
| 9 | `world.entity.Entity.load` | 5 | 7 | 1 | 1 | 4 | 1 | 1 | 0 | 0 | 0 | - |
| 10 | `server.MinecraftServer.reloadResources` | 5 | 6 | 1 | 2 | 2 | 0 | 1 | 1 | 0 | 0 | - |
| 11 | `world.entity.Entity.playStepSound` | 5 | 6 | 1 | 1 | 0 | 1 | 2 | 1 | 0 | 1 | - |
| 12 | `world.inventory.AbstractContainerMenu.doClick` | 5 | 6 | 0 | 1 | 1 | 0 | 3 | 1 | 0 | 0 | `ItemStack.copyWithCount` 1 |
| 13 | `world.level.levelgen.structure.templatesystem.StructureTemplate.palettes (field)` | 5 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 6 | - |
| 14 | `world.level.storage.loot.functions.EnchantedCountIncreaseFunction.run` | 5 | 6 | 1 | 1 | 1 | 0 | 0 | 4 | 0 | 0 | `EnchantmentHelper.getEnchantmentLevel` 1 |
| 15 | `world.entity.player.Player.tick` | 5 | 5 | 1 | 2 | 2 | 1 | 0 | 0 | 0 | 0 | - |
| 16 | `world.level.levelgen.structure.templatesystem.StructureTemplate.placeInWorld` | 5 | 5 | 1 | 1 | 3 | 0 | 0 | 1 | 0 | 0 | `ServerLevelAccessor.setBlock` 1 |
| 17 | `world.level.Explosion.finalizeExplosion` | 4 | 7 | 0 | 2 | 0 | 1 | 0 | 4 | 0 | 0 | `Block.popResource` 1 |
| 18 | `world.entity.LivingEntity.updateFallFlying` | 4 | 6 | 0 | 0 | 0 | 0 | 1 | 5 | 0 | 0 | - |
| 19 | `world.entity.player.Player.attack` | 4 | 6 | 1 | 0 | 0 | 0 | 1 | 5 | 0 | 0 | `Entity.hurt` 1, `LivingEntity.hurt` 1 |
| 20 | `server.network.ServerGamePacketListenerImpl.handleMovePlayer` | 4 | 5 | 1 | 0 | 0 | 1 | 1 | 3 | 0 | 0 | `ServerPlayer.isChangingDimension` 1 |
| 21 | `world.entity.Entity.level (field)` | 4 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 5 | - |
| 22 | `world.entity.LivingEntity.die` | 4 | 5 | 1 | 1 | 2 | 0 | 2 | 0 | 0 | 0 | - |
| 23 | `world.level.block.FireBlock.tick` | 4 | 5 | 1 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | `FireBlock.getIgniteOdds` 1, `ServerLevel.setBlock` 1 |
| 24 | `world.level.levelgen.structure.templatesystem.StructureTemplate.fillFromWorld` | 4 | 5 | 0 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 25 | `world.level.storage.loot.LootPool.entries (field)` | 4 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 5 | - |
| 26 | `server.MinecraftServer.storageSource (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 27 | `server.ReloadableServerResources.loadResources` | 4 | 4 | 1 | 1 | 0 | 0 | 2 | 1 | 0 | 0 | - |
| 28 | `server.ReloadableServerResources.recipes (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 29 | `server.ReloadableServerResources.updateRegistryTags` | 4 | 4 | 1 | 0 | 2 | 1 | 0 | 1 | 0 | 0 | `Blocks.rebuildCache` 1 |
| 30 | `server.players.PlayerList.placeNewPlayer` | 4 | 4 | 1 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | - |
| 31 | `world.entity.Entity.setLevel` | 4 | 4 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 32 | `world.entity.LivingEntity.onEffectAdded` | 4 | 4 | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 0 | - |
| 33 | `world.entity.LivingEntity.onEffectRemoved` | 4 | 4 | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 0 | - |
| 34 | `world.entity.LivingEntity.onEffectUpdated` | 4 | 4 | 1 | 1 | 2 | 0 | 0 | 0 | 0 | 1 | - |
| 35 | `world.entity.Mob.goalSelector (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 36 | `world.entity.item.FallingBlockEntity.tick` | 4 | 4 | 1 | 0 | 0 | 2 | 0 | 2 | 0 | 0 | `FallingBlock.isFree` 1 |
| 37 | `world.entity.item.ItemEntity.hurt` | 4 | 4 | 0 | 0 | 0 | 1 | 2 | 1 | 0 | 0 | `ItemEntity.isInvulnerableTo` 1 |
| 38 | `world.entity.item.ItemEntity.tick` | 4 | 4 | 0 | 0 | 3 | 0 | 0 | 1 | 0 | 0 | - |
| 39 | `world.entity.player.Inventory.player (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 40 | `world.entity.player.Player.tryToStartFallFlying` | 4 | 4 | 0 | 0 | 0 | 0 | 2 | 2 | 0 | 0 | - |
| 41 | `world.item.BlockItem.getPlacementState` | 4 | 4 | 1 | 0 | 0 | 0 | 2 | 1 | 0 | 1 | `Block.getStateForPlacement` 1 |
| 42 | `world.item.crafting.Ingredient.values (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 43 | `world.level.BaseSpawner.serverTick` | 4 | 4 | 1 | 0 | 0 | 3 | 1 | 0 | 0 | 0 | - |
| 44 | `world.level.biome.BiomeManager.biomeZoomSeed (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 45 | `world.level.block.Blocks.<clinit>` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | `(Lnet/minecraft/world/level/biome/Biome$Precipitation;Lnet/minecraft/core/cauldron/CauldronInteraction$InteractionMap;Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;)Lnet/minecraft/world/level/block/LayeredCauldronBlock;` 1, `BlockState.initCache` 1, `block.EnchantingTableBlock` 1 |
| 46 | `world.level.block.DispenserBlock.getDispenseMethod` | 4 | 4 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 3 | - |
| 47 | `world.level.storage.loot.LootTable.pools (field)` | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 48 | `world.item.MapItem.update` | 3 | 8 | 1 | 0 | 0 | 0 | 2 | 6 | 0 | 0 | `BlockState.getMapColor` 1, `Level.getChunk` 1, `LevelChunk.isEmpty` 1 |
| 49 | `server.WorldLoader.load` | 3 | 7 | 0 | 2 | 0 | 3 | 0 | 2 | 0 | 0 | - |
| 50 | `world.level.saveddata.maps.MapItemSavedData.tickCarriedBy` | 3 | 6 | 1 | 0 | 1 | 1 | 0 | 4 | 0 | 0 | `Component.getString` 1, `Inventory.contains` 1 |
| 51 | `server.level.ServerLevel.tick` | 3 | 5 | 1 | 2 | 1 | 2 | 0 | 0 | 0 | 0 | - |
| 52 | `server.level.ServerPlayerGameMode.destroyBlock` | 3 | 5 | 0 | 1 | 0 | 4 | 0 | 0 | 0 | 0 | - |
| 53 | `world.level.levelgen.structure.pools.StructureTemplatePool.rawTemplates (field)` | 3 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 5 | - |
| 54 | `server.level.ChunkMap.level (field)` | 3 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 55 | `server.level.ServerPlayer.<init>` | 3 | 4 | 1 | 0 | 1 | 0 | 0 | 3 | 0 | 0 | `ServerPlayer.adjustSpawnLocation` 2 |
| 56 | `server.level.ServerPlayerGameMode.useItemOn` | 3 | 4 | 1 | 1 | 2 | 0 | 1 | 0 | 0 | 0 | - |
| 57 | `server.network.ServerGamePacketListenerImpl.aboveGroundTickCount (field)` | 3 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 58 | `world.entity.npc.WanderingTrader.updateTrades` | 3 | 4 | 0 | 0 | 2 | 0 | 1 | 1 | 0 | 0 | `MerchantOffers.add` 1 |
| 59 | `world.entity.npc.WanderingTraderSpawner.spawn` | 3 | 4 | 0 | 0 | 1 | 0 | 1 | 2 | 0 | 0 | - |
| 60 | `world.item.BlockItem.place` | 3 | 4 | 0 | 0 | 0 | 1 | 2 | 1 | 0 | 0 | - |
| 61 | `world.item.crafting.RecipeManager.byName (field)` | 3 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 62 | `world.item.crafting.RecipeManager.byType (field)` | 3 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | - |
| 63 | `world.level.block.FireBlock.checkBurnOut` | 3 | 4 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 1 | `Level.removeBlock` 1, `Level.setBlock` 1 |
| 64 | `world.level.block.state.BlockBehaviour$BlockStateBase.initCache` | 3 | 4 | 1 | 1 | 1 | 0 | 0 | 1 | 0 | 1 | `Block.isRandomlyTicking` 1 |
| 65 | `core.component.PatchedDataComponentMap.patch (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 66 | `server.PlayerAdvancements.player (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 67 | `server.ReloadableServerResources.<init>` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 68 | `server.level.ServerPlayer.tick` | 3 | 3 | 1 | 1 | 1 | 0 | 0 | 1 | 0 | 0 | - |
| 69 | `tags.TagLoader.build` | 3 | 3 | 2 | 1 | 1 | 0 | 0 | 1 | 0 | 0 | `List.isEmpty` 1 |
| 70 | `util.datafix.fixes.ItemStackComponentizationFix.fixItemStack` | 3 | 3 | 0 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | - |
| 71 | `util.datafix.schemas.V1460.registerBlockEntities` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 72 | `world.entity.Entity.getXRot` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | - |
| 73 | `world.entity.Entity.getYRot` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | - |
| 74 | `world.entity.Entity.isCurrentlyGlowing` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | - |
| 75 | `world.entity.Entity.isInvulnerableTo` | 3 | 3 | 1 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | - |
| 76 | `world.entity.Entity.push` | 3 | 3 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | - |
| 77 | `world.entity.LightningBolt.tick` | 3 | 3 | 1 | 0 | 0 | 2 | 0 | 1 | 0 | 0 | - |
| 78 | `world.entity.LivingEntity.actuallyHurt` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 1 | `LivingEntity.setHealth` 1 |
| 79 | `world.entity.LivingEntity.checkTotemDeathProtection` | 3 | 3 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | - |
| 80 | `world.entity.LivingEntity.eat` | 3 | 3 | 1 | 2 | 0 | 0 | 1 | 0 | 0 | 0 | - |
| 81 | `world.entity.LivingEntity.getJumpBoostPower` | 3 | 3 | 0 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | - |
| 82 | `world.entity.LivingEntity.getSoundVolume` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | - |
| 83 | `world.entity.LivingEntity.hasEffect` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 84 | `world.entity.LivingEntity.isCurrentlyGlowing` | 3 | 3 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | - |
| 85 | `world.entity.Mob.getAmbientSound` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | - |
| 86 | `world.entity.item.FallingBlockEntity.blockState (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 87 | `world.entity.item.ItemEntity.getItem` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 88 | `world.entity.npc.Villager.updateSpecialPrices` | 3 | 3 | 1 | 0 | 1 | 0 | 0 | 1 | 0 | 1 | - |
| 89 | `world.entity.player.Player.aiStep` | 3 | 3 | 1 | 0 | 1 | 0 | 0 | 2 | 0 | 0 | - |
| 90 | `world.entity.player.Player.canEat` | 3 | 3 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | - |
| 91 | `world.inventory.AbstractContainerMenu.slots (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 92 | `world.inventory.AnvilMenu.createResult` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 2 | 0 | 0 | - |
| 93 | `world.item.ItemStack.inventoryTick` | 3 | 3 | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | - |
| 94 | `world.item.crafting.Ingredient.getItems` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | - |
| 95 | `world.item.crafting.ShapedRecipe.pattern (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 96 | `world.item.enchantment.EnchantmentHelper.doPostAttackEffectsWithItemSource` | 3 | 3 | 1 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | - |
| 97 | `world.level.block.BushBlock.mayPlaceOn` | 3 | 3 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 1 | - |
| 98 | `world.level.block.ConcretePowderBlock.concrete (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 99 | `world.level.block.FenceBlock.connectsTo` | 3 | 3 | 1 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | - |
| 100 | `world.level.block.state.BlockBehaviour$BlockStateBase.getBlock` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 101 | `world.level.block.state.BlockBehaviour$BlockStateBase.getCollisionShape` | 3 | 3 | 1 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | `Block.getCollisionShape` 1 |
| 102 | `world.level.block.state.BlockBehaviour$BlockStateBase.onRemove` | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | `Block.onRemove` 1 |
| 103 | `world.level.levelgen.structure.pools.JigsawPlacement.addPieces` | 3 | 3 | 1 | 0 | 0 | 2 | 1 | 0 | 0 | 0 | - |
| 104 | `world.level.levelgen.structure.templatesystem.StructureTemplate.load` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 105 | `world.level.levelgen.structure.templatesystem.StructureTemplate.save` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 106 | `world.level.levelgen.structure.templatesystem.StructureTemplate.size (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 107 | `world.level.saveddata.maps.MapItemSavedData.load` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 108 | `world.level.saveddata.maps.MapItemSavedData.save` | 3 | 3 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 109 | `world.level.storage.loot.LootPool.conditions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 110 | `world.level.storage.loot.LootPool.functions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 111 | `world.level.storage.loot.LootTable.functions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 112 | `world.level.storage.loot.LootTable.randomSequence (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 113 | `world.level.storage.loot.entries.CompositeEntryBase.children (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 114 | `world.level.storage.loot.entries.LootItem.item (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 115 | `world.level.storage.loot.functions.ApplyBonusCount.enchantment (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 116 | `world.level.storage.loot.functions.ApplyBonusCount.run` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | - |
| 117 | `world.level.storage.loot.functions.EnchantedCountIncreaseFunction.enchantment (field)` | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 118 | `world.level.storage.loot.predicates.LootItemRandomChanceWithEnchantedBonusCondition.test` | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | `EnchantmentHelper.getEnchantmentLevel` 1 |
| 119 | `world.level.block.piston.PistonStructureResolver.addBlockLine` | 2 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | `BlockState.canStickTo` 2, `BlockPos.equals` 1, `BlockState.isStickyBlock` 1 |
| 120 | `world.entity.LivingEntity.getDamageAfterMagicAbsorb` | 2 | 5 | 1 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | `CombatRules.getDamageAfterMagicAbsorb` 1, `LivingEntity.hasEffect` 1, `Math.max` 1 |
| 121 | `world.level.block.piston.PistonBaseBlock.moveBlocks` | 2 | 5 | 0 | 0 | 0 | 1 | 1 | 3 | 0 | 0 | `Level.setBlockEntity` 1, `MovingPistonBlock.newMovingBlockEntity` 1, `PistonStructureResolver.resolve` 1 |
| 122 | `server.commands.LocateCommand.locateStructure` | 2 | 4 | 0 | 1 | 1 | 0 | 1 | 1 | 0 | 0 | - |
| 123 | `world.food.FoodData.tick` | 2 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | `Player.heal` 1 |
| 124 | `world.item.ItemStack.forEachModifier` | 2 | 4 | 2 | 0 | 2 | 0 | 2 | 0 | 0 | 0 | - |
| 125 | `world.level.Explosion.explode` | 2 | 4 | 1 | 0 | 1 | 1 | 0 | 2 | 0 | 0 | `Entity.hurt` 1 |
| 126 | `world.level.block.piston.PistonMovingBlockEntity.tick` | 2 | 4 | 0 | 0 | 1 | 3 | 0 | 0 | 0 | 0 | - |
| 127 | `world.level.levelgen.structure.Structure.generate` | 2 | 4 | 0 | 1 | 1 | 0 | 2 | 0 | 0 | 0 | - |
| 128 | `server.level.ServerLevel.<init>` | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | `(Ljava/lang/Class;Lnet/minecraft/world/level/entity/LevelCallback;Lnet/minecraft/world/level/entity/EntityPersistentStorage;)Lnet/minecraft/world/level/entity/PersistentEntitySectionManager;` 1, `(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/level/storage/LevelStorageSource$LevelStorageAccess;Lcom/mojang/datafixers/DataFixer;Lnet/minecraft/world/level/levelgen/structure/templatesystem/StructureTemplateManager;Ljava/util/concurrent/Executor;Lnet/minecraft/world/level/chunk/ChunkGenerator;IIZLnet/minecraft/server/level/progress/ChunkProgressListener;Lnet/minecraft/world/level/entity/ChunkStatusUpdateListener;Ljava/util/function/Supplier;)Lnet/minecraft/server/level/ServerChunkCache;` 1, `ChunkGeneratorStructureState.ensureStructuresGenerated` 1 |
| 129 | `server.level.ServerLevel.addEntity` | 2 | 3 | 1 | 0 | 1 | 0 | 1 | 0 | 0 | 1 | - |
| 130 | `server.level.ServerLevel.tickChunk` | 2 | 3 | 1 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | `RandomSource.nextInt` 2 |
| 131 | `server.level.ServerLevel.tickNonPassenger` | 2 | 3 | 1 | 0 | 1 | 0 | 0 | 2 | 0 | 0 | `Entity.tickCount` 1 |
| 132 | `server.level.ServerPlayer.restoreFrom` | 2 | 3 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 133 | `world.entity.Entity.saveWithoutId` | 2 | 3 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 134 | `world.entity.Entity.setDeltaMovement` | 2 | 3 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 1 | - |
| 135 | `world.entity.Entity.spawnAtLocation` | 2 | 3 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | - |
| 136 | `world.entity.LivingEntity.aiStep` | 2 | 3 | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | - |
| 137 | `world.entity.Mob.doHurtTarget` | 2 | 3 | 1 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | - |
| 138 | `world.entity.npc.AbstractVillager.addOffersFromItemListings` | 2 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | `AbstractVillager.random` 1, `MerchantOffers.add` 1 |
| 139 | `world.entity.projectile.ThrownTrident.onHitEntity` | 2 | 3 | 1 | 0 | 0 | 0 | 2 | 1 | 0 | 0 | - |
| 140 | `world.inventory.AnvilMenu.onTake` | 2 | 3 | 0 | 1 | 0 | 0 | 1 | 1 | 0 | 0 | `Player.giveExperienceLevels` 1 |
| 141 | `world.item.ItemStack.getTooltipLines` | 2 | 3 | 1 | 0 | 0 | 2 | 1 | 0 | 0 | 0 | - |
| 142 | `world.item.ItemStack.hurtAndBreak` | 2 | 3 | 2 | 0 | 0 | 1 | 2 | 0 | 0 | 0 | - |
| 143 | `world.item.crafting.Ingredient.<init>` | 2 | 3 | 3 | 0 | 0 | 0 | 0 | 2 | 0 | 1 | - |
| 144 | `world.level.Explosion.level (field)` | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 145 | `world.level.block.DispenserBlock.dispenseFrom` | 2 | 3 | 0 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | - |
| 146 | `world.level.block.entity.BlockEntity.setLevel` | 2 | 3 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | - |
| 147 | `world.level.chunk.ChunkGenerator.findNearestMapStructure` | 2 | 3 | 0 | 1 | 0 | 0 | 1 | 1 | 0 | 0 | - |
| 148 | `world.level.chunk.ChunkGeneratorStructureState.generateRingPositions` | 2 | 3 | 0 | 0 | 0 | 1 | 1 | 1 | 0 | 0 | `Util.backgroundExecutor` 1 |
| 149 | `world.level.levelgen.structure.StructureStart.placeInChunk` | 2 | 3 | 0 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | - |
| 150 | `world.level.levelgen.structure.pools.StructureTemplatePool.templates (field)` | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 151 | `world.level.levelgen.structure.templatesystem.StructureTemplate.entityInfoList (field)` | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | - |
| 152 | `world.level.storage.loot.parameters.LootContextParamSets.register` | 2 | 3 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 2 | `ResourceLocation.withDefaultNamespace` 1 |
| 153 | `core.MappedRegistry.byValue (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 154 | `core.MappedRegistry.toId (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 155 | `core.component.DataComponents.lambda$static$1` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | - |
| 156 | `core.component.PatchedDataComponentMap.prototype (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 157 | `data.models.ItemModelGenerators.GENERATED_TRIM_MODELS (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 158 | `server.MinecraftServer.createLevels` | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | - |
| 159 | `server.MinecraftServer.lambda$reloadResources$29` | 2 | 2 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | `(Lnet/minecraft/server/packs/PackType;Ljava/util/List;)Lnet/minecraft/server/packs/resources/MultiPackResourceManager;` 1 |
| 160 | `server.MinecraftServer.stopServer` | 2 | 2 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 1 | - |
| 161 | `server.PlayerAdvancements.award` | 2 | 2 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | - |
| 162 | `server.level.ChunkMap.anyPlayerCloseEnoughForSpawning` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 163 | `server.level.ChunkMap.lambda$scheduleUnload$12` | 2 | 2 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 1 | - |
| 164 | `server.level.ChunkMap.pendingUnloads (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 165 | `server.level.ChunkMap.processUnloads` | 2 | 2 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | - |
| 166 | `server.level.ServerLevel.findLightningTargetAround` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | - |
| 167 | `server.level.ServerLevel.getEntity` | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 168 | `server.level.ServerLevel.levelEvent` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 169 | `server.level.ServerPlayerGameMode.level (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 170 | `server.level.ServerPlayerGameMode.player (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 171 | `server.network.ServerGamePacketListenerImpl.clientIsFloating (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 172 | `server.network.ServerGamePacketListenerImpl.handleMoveVehicle` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | - |
| 173 | `server.network.ServerGamePacketListenerImpl.handlePlayerAbilities` | 2 | 2 | 0 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | - |
| 174 | `server.players.PlayerList.reloadResources` | 2 | 2 | 1 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | - |
| 175 | `server.players.PlayerList.stats (field)` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 176 | `tags.TagLoader.load` | 2 | 2 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | - |
| 177 | `tags.TagManager.createLoader` | 2 | 2 | 1 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | - |
| 178 | `util.datafix.schemas.V1460.registerInventory` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 179 | `util.datafix.schemas.V3818_3.lambda$registerTypes$0` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | - |
| 180 | `world.damagesource.DamageSources.source` | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 181 | `world.entity.Entity.<init>` | 2 | 2 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | - |
| 182 | `world.entity.Entity.canAddPassenger` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | - |
| 183 | `world.entity.Entity.canRide` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 184 | `world.entity.Entity.extinguishFire` | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | - |
| 185 | `world.entity.Entity.getBlockSpeedFactor` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 186 | `world.entity.Entity.getDeltaMovement` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 187 | `world.entity.Entity.getGravity` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 188 | `world.entity.Entity.getSwimSplashSound` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | - |
| 189 | `world.entity.Entity.getTeamColor` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 190 | `world.entity.Entity.getType` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 191 | `world.entity.Entity.getX` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 192 | `world.entity.Entity.getY` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 193 | `world.entity.Entity.getZ` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 194 | `world.entity.Entity.hurt` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | - |
| 195 | `world.entity.Entity.ignoreExplosion` | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 196 | `world.entity.Entity.isInWaterOrRain` | 2 | 2 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | - |
| 197 | `world.entity.Entity.lavaHurt` | 2 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | - |
| 198 | `world.entity.Entity.level` | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | - |
| 199 | `world.entity.Entity.playerTouch` | 2 | 2 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | - |
| 200 | `world.entity.Entity.remove` | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | - |

Long tail: 1731 more target members in 511 classes, with 1974 members. The classes of the long tail that 2 or more mods target, by mods (target members, members, mods, intents):

| Class | Target members | Members | Mods | Intents |
|:--|--:|--:|--:|:--|
| `world.entity.LivingEntity` | 101 | 125 | 28 | pre 5, post 10, mid 4, cancel 63, value 9, accessor 34 |
| `world.entity.Entity` | 100 | 110 | 28 | pre 5, post 2, mid 2, cancel 46, value 12, accessor 43 |
| `world.item.ItemStack` | 29 | 35 | 17 | pre 1, post 2, mid 1, cancel 15, value 6, accessor 10 |
| `world.entity.player.Player` | 26 | 34 | 15 | pre 5, post 2, mid 1, cancel 7, value 6, accessor 13 |
| `world.item.Item` | 25 | 32 | 11 | pre 4, post 3, cancel 17, value 1, overwrite 1, accessor 6 |
| `world.level.block.state.BlockBehaviour$BlockStateBase` | 24 | 30 | 11 | pre 4, cancel 12, value 7, accessor 7 |
| `world.level.block.state.BlockBehaviour` | 28 | 29 | 10 | post 2, cancel 13, value 1, accessor 13 |
| `world.level.Level` | 15 | 16 | 10 | pre 1, post 3, cancel 2, value 4, accessor 8 |
| `world.entity.Mob` | 29 | 34 | 9 | pre 1, post 6, cancel 19, value 3, accessor 5 |
| `world.level.block.Block` | 11 | 12 | 8 | pre 2, post 2, cancel 4, value 2, accessor 2 |
| `server.level.ServerPlayer` | 13 | 16 | 7 | pre 1, post 7, cancel 5, accessor 3 |
| `world.item.crafting.RecipeManager` | 6 | 9 | 7 | pre 3, post 2, cancel 1, value 2, accessor 1 |
| `server.level.ChunkMap` | 13 | 17 | 6 | pre 2, mid 2, cancel 1, value 3, accessor 9 |
| `world.entity.item.ItemEntity` | 9 | 12 | 6 | mid 1, cancel 2, value 2, accessor 7 |
| `server.level.ServerLevel` | 8 | 10 | 6 | post 2, mid 1, value 2, accessor 5 |
| `world.inventory.AbstractContainerMenu` | 10 | 9 | 6 | cancel 1, value 5, accessor 5 |
| `world.entity.npc.Villager` | 15 | 18 | 5 | post 2, cancel 1, value 3, overwrite 3, accessor 9 |
| `server.MinecraftServer` | 17 | 17 | 5 | post 3, mid 1, value 4, accessor 9 |
| `world.level.chunk.ChunkGenerator` | 7 | 10 | 5 | cancel 7, value 2, accessor 1 |
| `world.item.enchantment.EnchantmentHelper` | 9 | 9 | 5 | cancel 4, value 5 |
| `world.level.material.FlowingFluid` | 6 | 8 | 5 | post 1, cancel 4, value 1, accessor 2 |
| `world.level.chunk.LevelChunk` | 7 | 7 | 5 | post 2, mid 1, cancel 1, value 1, accessor 2 |
| `world.entity.monster.Zombie` | 4 | 6 | 5 | pre 1, cancel 2, value 2, accessor 1 |
| `world.level.block.FarmBlock` | 4 | 6 | 5 | cancel 4, accessor 2 |
| `world.level.Explosion` | 12 | 14 | 4 | post 1, cancel 1, value 1, accessor 11 |
| `world.entity.player.Inventory` | 6 | 9 | 4 | pre 1, post 1, mid 2, cancel 4, accessor 1 |
| `core.MappedRegistry` | 7 | 8 | 4 | pre 1, post 3, value 1, accessor 3 |
| `server.players.PlayerList` | 7 | 8 | 4 | pre 2, post 1, cancel 1, value 2, accessor 2 |
| `server.network.ServerGamePacketListenerImpl` | 7 | 7 | 4 | pre 1, mid 2, cancel 3, accessor 1 |
| `world.item.BlockItem` | 6 | 6 | 4 | post 1, cancel 1, accessor 4 |
| `world.entity.LightningBolt` | 3 | 5 | 4 | pre 1, post 1, accessor 3 |
| `world.level.block.CropBlock` | 3 | 4 | 4 | cancel 1, accessor 3 |
| `world.level.saveddata.maps.MapItemSavedData` | 17 | 23 | 3 | post 5, value 1, accessor 17 |
| `server.level.ServerChunkCache` | 6 | 13 | 3 | post 1, mid 2, value 7, accessor 3 |
| `world.entity.animal.allay.Allay` | 8 | 11 | 3 | pre 1, post 4, cancel 4, accessor 2 |
| `world.item.crafting.Ingredient` | 9 | 11 | 3 | cancel 3, value 1, accessor 7 |
| `world.level.block.piston.PistonMovingBlockEntity` | 10 | 11 | 3 | post 3, mid 2, accessor 6 |
| `world.level.storage.loot.LootTable` | 8 | 11 | 3 | post 1, cancel 2, value 1, accessor 7 |
| `world.level.storage.loot.LootPool` | 7 | 9 | 3 | post 1, accessor 8 |
| `world.effect.MobEffectInstance` | 7 | 8 | 3 | pre 1, post 2, mid 1, cancel 1, value 1, accessor 2 |
| `world.entity.monster.Creeper` | 5 | 8 | 3 | pre 1, post 4, cancel 1, value 2 |
| `world.entity.projectile.AbstractArrow` | 7 | 8 | 3 | cancel 1, accessor 7 |
| `world.item.CreativeModeTab` | 6 | 7 | 3 | post 1, cancel 1, accessor 5 |
| `world.level.block.state.StateHolder` | 6 | 7 | 3 | post 1, value 2, overwrite 1, accessor 4 |
| `world.damagesource.DamageSource` | 6 | 6 | 3 | cancel 2, accessor 4 |
| `world.entity.projectile.Projectile` | 6 | 6 | 3 | pre 1, post 1, mid 1, cancel 3 |
| `world.item.enchantment.Enchantment` | 11 | 6 | 3 | cancel 2, value 8, accessor 1 |
| `world.entity.item.FallingBlockEntity` | 4 | 5 | 3 | pre 1, cancel 1, value 1, accessor 2 |
| `world.item.alchemy.PotionBrewing` | 4 | 5 | 3 | post 1, accessor 4 |
| `world.level.block.WallBlock` | 3 | 5 | 3 | post 1, cancel 4 |
| `world.level.levelgen.structure.templatesystem.StructureTemplateManager` | 5 | 5 | 3 | post 3, cancel 1, accessor 1 |
| `world.entity.decoration.ItemFrame` | 3 | 4 | 3 | cancel 1, value 2, accessor 1 |
| `world.entity.monster.piglin.PiglinAi` | 3 | 4 | 3 | cancel 2, value 1, accessor 1 |
| `world.level.levelgen.Beardifier` | 5 | 4 | 3 | mid 1, cancel 2, accessor 2 |
| `world.level.levelgen.structure.StructureStart` | 4 | 4 | 3 | accessor 4 |
| `world.inventory.TransientCraftingContainer` | 2 | 3 | 3 | accessor 3 |
| `world.level.BaseSpawner` | 2 | 3 | 3 | accessor 3 |
| `world.level.block.BaseFireBlock` | 3 | 3 | 3 | cancel 2, value 1 |
| `world.level.block.PowderSnowBlock` | 2 | 3 | 3 | cancel 2, value 1 |
| `world.item.trading.MerchantOffer` | 15 | 19 | 2 | post 2, cancel 4, value 1, accessor 12 |
| `world.inventory.EnchantmentMenu` | 12 | 14 | 2 | post 1, mid 1, cancel 3, value 2, accessor 7 |
| `world.level.block.entity.LecternBlockEntity` | 11 | 13 | 2 | pre 1, cancel 3, accessor 9 |
| `world.level.storage.loot.functions.ExplorationMapFunction` | 11 | 12 | 2 | post 2, cancel 1, accessor 9 |
| `world.item.alchemy.PotionBrewing$Builder` | 7 | 10 | 2 | pre 3, accessor 7 |
| `world.level.levelgen.structure.placement.RandomSpreadStructurePlacement` | 6 | 10 | 2 | post 1, value 4, accessor 5 |
| `world.level.levelgen.structure.structures.JigsawStructure` | 7 | 10 | 2 | value 5, accessor 5 |
| `world.level.block.entity.BlockEntity` | 6 | 9 | 2 | pre 1, post 2, accessor 6 |
| `server.level.ChunkHolder` | 8 | 8 | 2 | post 2, mid 2, accessor 4 |
| `world.entity.animal.horse.AbstractHorse` | 8 | 8 | 2 | pre 1, cancel 2, accessor 5 |
| `world.entity.npc.AbstractVillager` | 7 | 8 | 2 | pre 2, accessor 6 |
| `world.entity.npc.WanderingTraderSpawner` | 5 | 8 | 2 | pre 1, cancel 1, value 1, accessor 5 |
| `world.level.chunk.PalettedContainer` | 7 | 8 | 2 | post 1, mid 1, overwrite 2, accessor 4 |
| `world.level.block.piston.PistonStructureResolver` | 5 | 7 | 2 | value 4, accessor 3 |
| `world.level.chunk.ChunkAccess` | 7 | 7 | 2 | post 2, overwrite 1, accessor 4 |
| `world.entity.animal.horse.SkeletonHorse` | 6 | 6 | 2 | post 2, cancel 1, accessor 3 |
| `world.entity.monster.AbstractSkeleton` | 5 | 6 | 2 | post 1, mid 1, cancel 2, accessor 2 |
| `world.level.levelgen.structure.TemplateStructurePiece` | 4 | 6 | 2 | pre 1, post 1, accessor 4 |
| `world.level.storage.loot.functions.EnchantedCountIncreaseFunction` | 3 | 6 | 2 | accessor 6 |
| `core.Holder$Reference` | 4 | 5 | 2 | value 1, accessor 4 |
| `world.entity.projectile.FishingHook` | 5 | 5 | 2 | post 1, value 1, accessor 3 |
| `world.level.NaturalSpawner` | 5 | 5 | 2 | mid 1, value 5 |
| `world.level.block.state.BlockBehaviour$BlockStateBase$Cache` | 4 | 5 | 2 | value 2, accessor 3 |
| `world.level.levelgen.NoiseChunk` | 5 | 5 | 2 | post 1, cancel 1, overwrite 1, accessor 2 |
| `world.level.levelgen.structure.Structure` | 5 | 5 | 2 | cancel 3, accessor 2 |
| `world.entity.ai.goal.target.NearestAttackableTargetGoal` | 4 | 4 | 2 | pre 1, post 1, accessor 2 |
| `world.entity.monster.Skeleton` | 3 | 4 | 2 | post 3, cancel 1 |
| `world.entity.monster.Slime` | 4 | 4 | 2 | mid 1, cancel 1, accessor 2 |
| `world.inventory.AnvilMenu` | 4 | 4 | 2 | cancel 1, accessor 3 |
| `world.inventory.CraftingMenu` | 3 | 4 | 2 | value 1, accessor 3 |
| `world.inventory.GrindstoneMenu` | 4 | 4 | 2 | cancel 1, value 1, accessor 2 |
| `world.inventory.SmithingMenu` | 3 | 4 | 2 | pre 1, mid 1, value 1, accessor 1 |
| `world.item.crafting.ShapedRecipePattern` | 3 | 4 | 2 | accessor 4 |
| `world.item.crafting.SmithingTransformRecipe` | 4 | 4 | 2 | cancel 1, accessor 3 |
| `world.level.StructureManager` | 3 | 4 | 2 | cancel 1, accessor 3 |
| `world.level.block.FireBlock` | 3 | 4 | 2 | value 2, accessor 2 |
| `world.level.block.NetherPortalBlock` | 3 | 4 | 2 | cancel 1, value 2, accessor 1 |
| `world.level.block.entity.AbstractFurnaceBlockEntity` | 4 | 4 | 2 | accessor 4 |
| `world.level.block.entity.BlockEntityType` | 3 | 4 | 2 | value 1, accessor 3 |
| `world.level.block.piston.PistonBaseBlock` | 2 | 4 | 2 | mid 1, cancel 1, value 2 |
| `world.level.levelgen.structure.pools.JigsawPlacement$Placer` | 3 | 4 | 2 | mid 1, value 1, accessor 2 |
| `world.level.levelgen.structure.templatesystem.StructureTemplate$StructureBlockInfo` | 4 | 4 | 2 | post 1, accessor 3 |
| `Util` | 3 | 3 | 2 | post 1, value 1, accessor 1 |
| `commands.CommandSourceStack` | 3 | 3 | 2 | cancel 1, accessor 2 |
| `network.chat.TextColor` | 3 | 3 | 2 | cancel 1, accessor 2 |
| `server.level.ChunkMap$TrackedEntity` | 3 | 3 | 2 | value 1, accessor 2 |
| `tags.TagLoader` | 4 | 3 | 2 | pre 2, value 1, accessor 1 |
| `util.datafix.schemas.V1460` | 3 | 3 | 2 | pre 1, post 1, value 1 |
| `world.entity.ExperienceOrb` | 3 | 3 | 2 | cancel 1, value 2 |
| `world.entity.decoration.Painting` | 2 | 3 | 2 | cancel 2, accessor 1 |
| `world.inventory.ItemCombinerMenu` | 3 | 3 | 2 | cancel 1, accessor 2 |
| `world.inventory.MerchantMenu` | 3 | 3 | 2 | post 1, accessor 2 |
| `world.item.BucketItem` | 3 | 3 | 2 | cancel 2, accessor 1 |
| `world.item.Items` | 2 | 3 | 2 | cancel 1, value 1, accessor 1 |
| `world.level.biome.Biome` | 3 | 3 | 2 | cancel 1, overwrite 1, accessor 1 |
| `world.level.block.FenceBlock` | 2 | 3 | 2 | cancel 3 |
| `world.level.block.state.BlockBehaviour$Properties` | 2 | 3 | 2 | value 1, accessor 2 |
| `world.level.block.state.StateDefinition$Builder` | 3 | 3 | 2 | post 1, accessor 2 |
| `world.level.chunk.status.ChunkStatusTasks` | 3 | 3 | 2 | mid 1, value 2 |
| `world.level.levelgen.structure.templatesystem.StructureTemplate` | 2 | 3 | 2 | pre 1, post 1, accessor 1 |
| `world.level.storage.loot.LootDataType` | 2 | 3 | 2 | post 1, cancel 1, accessor 1 |
| `world.level.storage.loot.entries.LootPoolSingletonContainer` | 3 | 3 | 2 | accessor 3 |
| `commands.Commands` | 2 | 2 | 2 | post 1, value 1 |
| `core.component.DataComponents` | 2 | 2 | 2 | value 1, accessor 1 |
| `util.datafix.DataFixers` | 2 | 2 | 2 | value 2 |
| `world.entity.decoration.ArmorStand` | 1 | 2 | 2 | cancel 2 |
| `world.inventory.GrindstoneMenu$4` | 2 | 2 | 2 | pre 1, cancel 1 |
| `world.item.BrushItem` | 1 | 2 | 2 | cancel 1, value 1 |
| `world.item.DiggerItem` | 2 | 2 | 2 | post 1, cancel 1 |
| `world.item.StandingAndWallBlockItem` | 2 | 2 | 2 | value 1, accessor 1 |
| `world.item.crafting.Ingredient$TagValue` | 2 | 2 | 2 | accessor 2 |
| `world.level.block.AnvilBlock` | 1 | 2 | 2 | post 2 |
| `world.level.block.Blocks` | 1 | 2 | 2 | post 1, value 1 |
| `world.level.block.CampfireBlock` | 1 | 2 | 2 | cancel 2 |
| `world.level.block.LanternBlock` | 1 | 2 | 2 | value 2 |
| `world.level.block.LiquidBlock` | 2 | 2 | 2 | value 1, accessor 1 |
| `world.level.block.StairBlock` | 2 | 2 | 2 | cancel 1, value 1 |
| `world.level.block.entity.BaseContainerBlockEntity` | 1 | 2 | 2 | accessor 2 |
| `world.level.entity.PersistentEntitySectionManager` | 3 | 2 | 2 | value 2, accessor 1 |
| `world.level.levelgen.NoiseBasedChunkGenerator` | 2 | 2 | 2 | post 1, mid 1 |
| `world.level.levelgen.feature.Feature` | 2 | 2 | 2 | cancel 2 |
| `world.level.levelgen.structure.pools.SinglePoolElement` | 2 | 2 | 2 | accessor 2 |
| `world.level.levelgen.structure.templatesystem.StructureProcessor` | 1 | 2 | 2 | accessor 2 |
| `world.level.pathfinder.WalkNodeEvaluator` | 2 | 2 | 2 | cancel 1, value 1 |
| `world.level.storage.loot.parameters.LootContextParamSets` | 2 | 2 | 2 | post 1, accessor 1 |
| `world.level.storage.loot.predicates.LootItemRandomChanceWithEnchantedBonusCondition` | 1 | 2 | 2 | accessor 2 |

The other 366 classes of the long tail have one mod each (735 members). By package (classes, members, mods): `world.level` 133/272/46, `world.entity` 75/131/31, `server` 35/63/15, `world.item` 33/59/23, `com` 4/32/1, `core` 11/29/7, `world.phys` 6/29/1, `network` 9/26/7, `util` 9/14/5, `world.inventory` 8/11/4, `data` 6/8/5, `nbt` 4/8/2, `advancements` 6/7/3, `resources` 4/7/4, `tags` 3/6/3, `gametest` 3/5/2, `world.damagesource` 3/5/2, `stats` 2/4/1, `SystemReport` 1/3/1, `world.food` 1/3/1, `commands` 2/2/2, `world.CompoundContainer` 1/2/1, `world.Containers` 1/2/1, `world.RandomizableContainer` 1/2/1, `world.effect` 2/2/2, `ChatFormatting` 1/1/1, `recipebook` 1/1/1, `world.Container` 1/1/1.

### 9.5 Intent classes per target member

For each target member of section 9.4: the members in each intent class (section 9.1), and the strongest class. **Accessor**: `@Accessor`, `@Invoker`, `@Shadow`; a source patch serves it, or the NeoForge-shaped API when it exposes the field or method. **Hook**: pre, post and cancel, at the start or the end of the method; a NeoForge event serves it when one fires on that method, otherwise a source patch. **Call site**: value modifiers and mid-method injections, at an identified call, field access or constant inside the method; a source patch serves it. **Sites** is the number of distinct `@At` points. **Override**: `@Overwrite`, a whole-method override; a source patch serves it. **Strongest** is the strongest class of the members of the target member, in the order accessor, hook, call site, override. A target member whose strongest class is call site or override needs a source patch.

| # | Target member | Mods | Accessor | Hook | Call site | Sites | Override | Strongest |
|--:|:--|--:|--:|--:|--:|--:|--:|:--|
| 1 | `world.item.crafting.RecipeManager.apply` | 11 | 0 | 11 | 6 | 4 | 0 | call site |
| 2 | `world.entity.LivingEntity.travel` | 9 | 0 | 1 | 9 | 7 | 0 | call site |
| 3 | `world.entity.LivingEntity.hurt` | 6 | 0 | 6 | 3 | 3 | 0 | call site |
| 4 | `server.network.ServerGamePacketListenerImpl.player (field)` | 6 | 7 | 0 | 0 | 0 | 0 | accessor |
| 5 | `world.entity.Entity.isAlliedTo` | 6 | 1 | 5 | 0 | 0 | 0 | hook |
| 6 | `world.entity.Entity.move` | 6 | 0 | 1 | 5 | 4 | 0 | call site |
| 7 | `world.entity.LivingEntity.canAttack` | 6 | 0 | 6 | 0 | 0 | 0 | hook |
| 8 | `world.level.block.state.BlockBehaviour$BlockStateBase.asState` | 6 | 6 | 0 | 0 | 0 | 0 | accessor |
| 9 | `world.entity.Entity.load` | 5 | 0 | 6 | 1 | 1 | 0 | call site |
| 10 | `server.MinecraftServer.reloadResources` | 5 | 0 | 5 | 1 | 1 | 0 | call site |
| 11 | `world.entity.Entity.playStepSound` | 5 | 1 | 3 | 2 | 2 | 0 | call site |
| 12 | `world.inventory.AbstractContainerMenu.doClick` | 5 | 0 | 5 | 1 | 1 | 0 | call site |
| 13 | `world.level.levelgen.structure.templatesystem.StructureTemplate.palettes (field)` | 5 | 6 | 0 | 0 | 0 | 0 | accessor |
| 14 | `world.level.storage.loot.functions.EnchantedCountIncreaseFunction.run` | 5 | 0 | 2 | 4 | 3 | 0 | call site |
| 15 | `world.entity.player.Player.tick` | 5 | 0 | 4 | 1 | 1 | 0 | call site |
| 16 | `world.level.levelgen.structure.templatesystem.StructureTemplate.placeInWorld` | 5 | 0 | 4 | 1 | 1 | 0 | call site |
| 17 | `world.level.Explosion.finalizeExplosion` | 4 | 0 | 2 | 5 | 3 | 0 | call site |
| 18 | `world.entity.LivingEntity.updateFallFlying` | 4 | 0 | 1 | 5 | 3 | 0 | call site |
| 19 | `world.entity.player.Player.attack` | 4 | 0 | 1 | 5 | 4 | 0 | call site |
| 20 | `server.network.ServerGamePacketListenerImpl.handleMovePlayer` | 4 | 0 | 1 | 4 | 4 | 0 | call site |
| 21 | `world.entity.Entity.level (field)` | 4 | 5 | 0 | 0 | 0 | 0 | accessor |
| 22 | `world.entity.LivingEntity.die` | 4 | 0 | 5 | 0 | 0 | 0 | hook |
| 23 | `world.level.block.FireBlock.tick` | 4 | 0 | 0 | 5 | 4 | 0 | call site |
| 24 | `world.level.levelgen.structure.templatesystem.StructureTemplate.fillFromWorld` | 4 | 0 | 5 | 0 | 0 | 0 | hook |
| 25 | `world.level.storage.loot.LootPool.entries (field)` | 4 | 5 | 0 | 0 | 0 | 0 | accessor |
| 26 | `server.MinecraftServer.storageSource (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 27 | `server.ReloadableServerResources.loadResources` | 4 | 0 | 3 | 1 | 1 | 0 | call site |
| 28 | `server.ReloadableServerResources.recipes (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 29 | `server.ReloadableServerResources.updateRegistryTags` | 4 | 0 | 2 | 2 | 1 | 0 | call site |
| 30 | `server.players.PlayerList.placeNewPlayer` | 4 | 0 | 0 | 4 | 4 | 0 | call site |
| 31 | `world.entity.Entity.setLevel` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 32 | `world.entity.LivingEntity.onEffectAdded` | 4 | 0 | 4 | 0 | 0 | 0 | hook |
| 33 | `world.entity.LivingEntity.onEffectRemoved` | 4 | 0 | 4 | 0 | 0 | 0 | hook |
| 34 | `world.entity.LivingEntity.onEffectUpdated` | 4 | 1 | 3 | 0 | 0 | 0 | hook |
| 35 | `world.entity.Mob.goalSelector (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 36 | `world.entity.item.FallingBlockEntity.tick` | 4 | 0 | 0 | 4 | 4 | 0 | call site |
| 37 | `world.entity.item.ItemEntity.hurt` | 4 | 0 | 2 | 2 | 2 | 0 | call site |
| 38 | `world.entity.item.ItemEntity.tick` | 4 | 0 | 3 | 1 | 1 | 0 | call site |
| 39 | `world.entity.player.Inventory.player (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 40 | `world.entity.player.Player.tryToStartFallFlying` | 4 | 0 | 2 | 2 | 1 | 0 | call site |
| 41 | `world.item.BlockItem.getPlacementState` | 4 | 1 | 2 | 1 | 1 | 0 | call site |
| 42 | `world.item.crafting.Ingredient.values (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 43 | `world.level.BaseSpawner.serverTick` | 4 | 0 | 1 | 3 | 3 | 0 | call site |
| 44 | `world.level.biome.BiomeManager.biomeZoomSeed (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 45 | `world.level.block.Blocks.<clinit>` | 4 | 0 | 0 | 4 | 4 | 0 | call site |
| 46 | `world.level.block.DispenserBlock.getDispenseMethod` | 4 | 3 | 1 | 0 | 0 | 0 | hook |
| 47 | `world.level.storage.loot.LootTable.pools (field)` | 4 | 4 | 0 | 0 | 0 | 0 | accessor |
| 48 | `world.item.MapItem.update` | 3 | 0 | 2 | 6 | 6 | 0 | call site |
| 49 | `server.WorldLoader.load` | 3 | 0 | 2 | 5 | 4 | 0 | call site |
| 50 | `world.level.saveddata.maps.MapItemSavedData.tickCarriedBy` | 3 | 0 | 1 | 5 | 4 | 0 | call site |
| 51 | `server.level.ServerLevel.tick` | 3 | 0 | 3 | 2 | 2 | 0 | call site |
| 52 | `server.level.ServerPlayerGameMode.destroyBlock` | 3 | 0 | 1 | 4 | 3 | 0 | call site |
| 53 | `world.level.levelgen.structure.pools.StructureTemplatePool.rawTemplates (field)` | 3 | 5 | 0 | 0 | 0 | 0 | accessor |
| 54 | `server.level.ChunkMap.level (field)` | 3 | 4 | 0 | 0 | 0 | 0 | accessor |
| 55 | `server.level.ServerPlayer.<init>` | 3 | 0 | 1 | 3 | 2 | 0 | call site |
| 56 | `server.level.ServerPlayerGameMode.useItemOn` | 3 | 0 | 4 | 0 | 0 | 0 | hook |
| 57 | `server.network.ServerGamePacketListenerImpl.aboveGroundTickCount (field)` | 3 | 4 | 0 | 0 | 0 | 0 | accessor |
| 58 | `world.entity.npc.WanderingTrader.updateTrades` | 3 | 0 | 3 | 1 | 1 | 0 | call site |
| 59 | `world.entity.npc.WanderingTraderSpawner.spawn` | 3 | 0 | 2 | 2 | 2 | 0 | call site |
| 60 | `world.item.BlockItem.place` | 3 | 0 | 2 | 2 | 2 | 0 | call site |
| 61 | `world.item.crafting.RecipeManager.byName (field)` | 3 | 4 | 0 | 0 | 0 | 0 | accessor |
| 62 | `world.item.crafting.RecipeManager.byType (field)` | 3 | 4 | 0 | 0 | 0 | 0 | accessor |
| 63 | `world.level.block.FireBlock.checkBurnOut` | 3 | 1 | 0 | 3 | 3 | 0 | call site |
| 64 | `world.level.block.state.BlockBehaviour$BlockStateBase.initCache` | 3 | 1 | 2 | 1 | 1 | 0 | call site |
| 65 | `core.component.PatchedDataComponentMap.patch (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 66 | `server.PlayerAdvancements.player (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 67 | `server.ReloadableServerResources.<init>` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 68 | `server.level.ServerPlayer.tick` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 69 | `tags.TagLoader.build` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 70 | `util.datafix.fixes.ItemStackComponentizationFix.fixItemStack` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 71 | `util.datafix.schemas.V1460.registerBlockEntities` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 72 | `world.entity.Entity.getXRot` | 3 | 2 | 0 | 1 | 1 | 0 | call site |
| 73 | `world.entity.Entity.getYRot` | 3 | 2 | 0 | 1 | 1 | 0 | call site |
| 74 | `world.entity.Entity.isCurrentlyGlowing` | 3 | 2 | 1 | 0 | 0 | 0 | hook |
| 75 | `world.entity.Entity.isInvulnerableTo` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 76 | `world.entity.Entity.push` | 3 | 1 | 2 | 0 | 0 | 0 | hook |
| 77 | `world.entity.LightningBolt.tick` | 3 | 0 | 0 | 3 | 3 | 0 | call site |
| 78 | `world.entity.LivingEntity.actuallyHurt` | 3 | 1 | 1 | 1 | 1 | 0 | call site |
| 79 | `world.entity.LivingEntity.checkTotemDeathProtection` | 3 | 1 | 2 | 0 | 0 | 0 | hook |
| 80 | `world.entity.LivingEntity.eat` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 81 | `world.entity.LivingEntity.getJumpBoostPower` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 82 | `world.entity.LivingEntity.getSoundVolume` | 3 | 2 | 1 | 0 | 0 | 0 | hook |
| 83 | `world.entity.LivingEntity.hasEffect` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 84 | `world.entity.LivingEntity.isCurrentlyGlowing` | 3 | 1 | 2 | 0 | 0 | 0 | hook |
| 85 | `world.entity.Mob.getAmbientSound` | 3 | 2 | 1 | 0 | 0 | 0 | hook |
| 86 | `world.entity.item.FallingBlockEntity.blockState (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 87 | `world.entity.item.ItemEntity.getItem` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 88 | `world.entity.npc.Villager.updateSpecialPrices` | 3 | 1 | 1 | 1 | 1 | 0 | call site |
| 89 | `world.entity.player.Player.aiStep` | 3 | 0 | 1 | 2 | 2 | 0 | call site |
| 90 | `world.entity.player.Player.canEat` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 91 | `world.inventory.AbstractContainerMenu.slots (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 92 | `world.inventory.AnvilMenu.createResult` | 3 | 0 | 1 | 2 | 2 | 0 | call site |
| 93 | `world.item.ItemStack.inventoryTick` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 94 | `world.item.crafting.Ingredient.getItems` | 3 | 1 | 0 | 1 | 1 | 1 | override |
| 95 | `world.item.crafting.ShapedRecipe.pattern (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 96 | `world.item.enchantment.EnchantmentHelper.doPostAttackEffectsWithItemSource` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 97 | `world.level.block.BushBlock.mayPlaceOn` | 3 | 1 | 1 | 1 | 1 | 0 | call site |
| 98 | `world.level.block.ConcretePowderBlock.concrete (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 99 | `world.level.block.FenceBlock.connectsTo` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 100 | `world.level.block.state.BlockBehaviour$BlockStateBase.getBlock` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 101 | `world.level.block.state.BlockBehaviour$BlockStateBase.getCollisionShape` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 102 | `world.level.block.state.BlockBehaviour$BlockStateBase.onRemove` | 3 | 0 | 2 | 1 | 1 | 0 | call site |
| 103 | `world.level.levelgen.structure.pools.JigsawPlacement.addPieces` | 3 | 0 | 1 | 2 | 2 | 0 | call site |
| 104 | `world.level.levelgen.structure.templatesystem.StructureTemplate.load` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 105 | `world.level.levelgen.structure.templatesystem.StructureTemplate.save` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 106 | `world.level.levelgen.structure.templatesystem.StructureTemplate.size (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 107 | `world.level.saveddata.maps.MapItemSavedData.load` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 108 | `world.level.saveddata.maps.MapItemSavedData.save` | 3 | 0 | 3 | 0 | 0 | 0 | hook |
| 109 | `world.level.storage.loot.LootPool.conditions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 110 | `world.level.storage.loot.LootPool.functions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 111 | `world.level.storage.loot.LootTable.functions (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 112 | `world.level.storage.loot.LootTable.randomSequence (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 113 | `world.level.storage.loot.entries.CompositeEntryBase.children (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 114 | `world.level.storage.loot.entries.LootItem.item (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 115 | `world.level.storage.loot.functions.ApplyBonusCount.enchantment (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 116 | `world.level.storage.loot.functions.ApplyBonusCount.run` | 3 | 0 | 0 | 3 | 2 | 0 | call site |
| 117 | `world.level.storage.loot.functions.EnchantedCountIncreaseFunction.enchantment (field)` | 3 | 3 | 0 | 0 | 0 | 0 | accessor |
| 118 | `world.level.storage.loot.predicates.LootItemRandomChanceWithEnchantedBonusCondition.test` | 3 | 0 | 0 | 3 | 2 | 0 | call site |
| 119 | `world.level.block.piston.PistonStructureResolver.addBlockLine` | 2 | 0 | 0 | 7 | 5 | 0 | call site |
| 120 | `world.entity.LivingEntity.getDamageAfterMagicAbsorb` | 2 | 0 | 0 | 5 | 5 | 0 | call site |
| 121 | `world.level.block.piston.PistonBaseBlock.moveBlocks` | 2 | 0 | 1 | 4 | 4 | 0 | call site |
| 122 | `server.commands.LocateCommand.locateStructure` | 2 | 0 | 3 | 1 | 1 | 0 | call site |
| 123 | `world.food.FoodData.tick` | 2 | 0 | 2 | 2 | 2 | 0 | call site |
| 124 | `world.item.ItemStack.forEachModifier` | 2 | 0 | 4 | 0 | 0 | 0 | hook |
| 125 | `world.level.Explosion.explode` | 2 | 0 | 1 | 3 | 3 | 0 | call site |
| 126 | `world.level.block.piston.PistonMovingBlockEntity.tick` | 2 | 0 | 1 | 3 | 2 | 0 | call site |
| 127 | `world.level.levelgen.structure.Structure.generate` | 2 | 0 | 4 | 0 | 0 | 0 | hook |
| 128 | `server.level.ServerLevel.<init>` | 2 | 0 | 0 | 3 | 3 | 0 | call site |
| 129 | `server.level.ServerLevel.addEntity` | 2 | 1 | 2 | 0 | 0 | 0 | hook |
| 130 | `server.level.ServerLevel.tickChunk` | 2 | 0 | 1 | 2 | 1 | 0 | call site |
| 131 | `server.level.ServerLevel.tickNonPassenger` | 2 | 0 | 1 | 2 | 2 | 0 | call site |
| 132 | `server.level.ServerPlayer.restoreFrom` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 133 | `world.entity.Entity.saveWithoutId` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 134 | `world.entity.Entity.setDeltaMovement` | 2 | 1 | 2 | 0 | 0 | 0 | hook |
| 135 | `world.entity.Entity.spawnAtLocation` | 2 | 2 | 1 | 0 | 0 | 0 | hook |
| 136 | `world.entity.LivingEntity.aiStep` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 137 | `world.entity.Mob.doHurtTarget` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 138 | `world.entity.npc.AbstractVillager.addOffersFromItemListings` | 2 | 0 | 1 | 2 | 2 | 0 | call site |
| 139 | `world.entity.projectile.ThrownTrident.onHitEntity` | 2 | 0 | 2 | 1 | 1 | 0 | call site |
| 140 | `world.inventory.AnvilMenu.onTake` | 2 | 0 | 2 | 1 | 1 | 0 | call site |
| 141 | `world.item.ItemStack.getTooltipLines` | 2 | 0 | 1 | 2 | 2 | 0 | call site |
| 142 | `world.item.ItemStack.hurtAndBreak` | 2 | 0 | 2 | 1 | 1 | 0 | call site |
| 143 | `world.item.crafting.Ingredient.<init>` | 2 | 1 | 0 | 2 | 1 | 0 | call site |
| 144 | `world.level.Explosion.level (field)` | 2 | 3 | 0 | 0 | 0 | 0 | accessor |
| 145 | `world.level.block.DispenserBlock.dispenseFrom` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 146 | `world.level.block.entity.BlockEntity.setLevel` | 2 | 0 | 3 | 0 | 0 | 0 | hook |
| 147 | `world.level.chunk.ChunkGenerator.findNearestMapStructure` | 2 | 0 | 2 | 1 | 1 | 0 | call site |
| 148 | `world.level.chunk.ChunkGeneratorStructureState.generateRingPositions` | 2 | 0 | 1 | 2 | 2 | 0 | call site |
| 149 | `world.level.levelgen.structure.StructureStart.placeInChunk` | 2 | 0 | 2 | 1 | 1 | 0 | call site |
| 150 | `world.level.levelgen.structure.pools.StructureTemplatePool.templates (field)` | 2 | 3 | 0 | 0 | 0 | 0 | accessor |
| 151 | `world.level.levelgen.structure.templatesystem.StructureTemplate.entityInfoList (field)` | 2 | 3 | 0 | 0 | 0 | 0 | accessor |
| 152 | `world.level.storage.loot.parameters.LootContextParamSets.register` | 2 | 2 | 0 | 1 | 1 | 0 | call site |
| 153 | `core.MappedRegistry.byValue (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 154 | `core.MappedRegistry.toId (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 155 | `core.component.DataComponents.lambda$static$1` | 2 | 0 | 0 | 2 | 2 | 0 | call site |
| 156 | `core.component.PatchedDataComponentMap.prototype (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 157 | `data.models.ItemModelGenerators.GENERATED_TRIM_MODELS (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 158 | `server.MinecraftServer.createLevels` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 159 | `server.MinecraftServer.lambda$reloadResources$29` | 2 | 0 | 0 | 2 | 2 | 0 | call site |
| 160 | `server.MinecraftServer.stopServer` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 161 | `server.PlayerAdvancements.award` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 162 | `server.level.ChunkMap.anyPlayerCloseEnoughForSpawning` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 163 | `server.level.ChunkMap.lambda$scheduleUnload$12` | 2 | 1 | 0 | 1 | 1 | 0 | call site |
| 164 | `server.level.ChunkMap.pendingUnloads (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 165 | `server.level.ChunkMap.processUnloads` | 2 | 0 | 1 | 1 | 1 | 0 | call site |
| 166 | `server.level.ServerLevel.findLightningTargetAround` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 167 | `server.level.ServerLevel.getEntity` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 168 | `server.level.ServerLevel.levelEvent` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 169 | `server.level.ServerPlayerGameMode.level (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 170 | `server.level.ServerPlayerGameMode.player (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 171 | `server.network.ServerGamePacketListenerImpl.clientIsFloating (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 172 | `server.network.ServerGamePacketListenerImpl.handleMoveVehicle` | 2 | 0 | 1 | 1 | 1 | 0 | call site |
| 173 | `server.network.ServerGamePacketListenerImpl.handlePlayerAbilities` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 174 | `server.players.PlayerList.reloadResources` | 2 | 0 | 0 | 2 | 2 | 0 | call site |
| 175 | `server.players.PlayerList.stats (field)` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 176 | `tags.TagLoader.load` | 2 | 0 | 1 | 1 | 1 | 0 | call site |
| 177 | `tags.TagManager.createLoader` | 2 | 0 | 1 | 1 | 1 | 0 | call site |
| 178 | `util.datafix.schemas.V1460.registerInventory` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 179 | `util.datafix.schemas.V3818_3.lambda$registerTypes$0` | 2 | 0 | 0 | 2 | 2 | 0 | call site |
| 180 | `world.damagesource.DamageSources.source` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 181 | `world.entity.Entity.<init>` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 182 | `world.entity.Entity.canAddPassenger` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 183 | `world.entity.Entity.canRide` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 184 | `world.entity.Entity.extinguishFire` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 185 | `world.entity.Entity.getBlockSpeedFactor` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 186 | `world.entity.Entity.getDeltaMovement` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 187 | `world.entity.Entity.getGravity` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 188 | `world.entity.Entity.getSwimSplashSound` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 189 | `world.entity.Entity.getTeamColor` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 190 | `world.entity.Entity.getType` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 191 | `world.entity.Entity.getX` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 192 | `world.entity.Entity.getY` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 193 | `world.entity.Entity.getZ` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 194 | `world.entity.Entity.hurt` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 195 | `world.entity.Entity.ignoreExplosion` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 196 | `world.entity.Entity.isInWaterOrRain` | 2 | 0 | 1 | 1 | 1 | 0 | call site |
| 197 | `world.entity.Entity.lavaHurt` | 2 | 0 | 2 | 0 | 0 | 0 | hook |
| 198 | `world.entity.Entity.level` | 2 | 2 | 0 | 0 | 0 | 0 | accessor |
| 199 | `world.entity.Entity.playerTouch` | 2 | 1 | 1 | 0 | 0 | 0 | hook |
| 200 | `world.entity.Entity.remove` | 2 | 1 | 1 | 0 | 0 | 0 | hook |

Strongest class over the 200 target members: accessor 58, hook 56, call site 85, override 1.

Call-site points over all target members: 554 distinct points (target member, `@At` value and target); 520 of them have one member.

#### Classes that mods replace

These are the classes whose logic mods replace instead of hooking it: the classes that #51 lists, and every other class in the scope whose members are mostly `@Overwrite` or `@Redirect` (half or more of its members that are not accessors, with at least 2 mods). Their members become a source patch, or a NeoForge API row where the inventory has one. Columns: mods (all members), accessor members, members that are not accessors, `@Overwrite`, `@Redirect`, `@WrapOperation`, the share of `@Overwrite` and `@Redirect`, and the most targeted members.

| Class | Mods | Accessors | Members | Overwrite | Redirect | WrapOperation | Share | Top members |
|:--|--:|--:|--:|--:|--:|--:|--:|:--|
| `world.item.crafting.RecipeManager` | 15 | 9 | 25 | 0 | 1 | 0 | 4% | `apply` 17, `fromJson` 2, `lambda$apply$0` 2 |
| `server.ReloadableServerResources` | 9 | 5 | 11 | 0 | 1 | 0 | 9% | `loadResources` 4, `updateRegistryTags` 4, `<init>` 3 |
| `server.players.PlayerList` | 8 | 4 | 12 | 0 | 0 | 0 | 0% | `placeNewPlayer` 4, `getPlayerAdvancements` 2, `reloadResources` 2 |
| `world.item.enchantment.EnchantmentHelper` | 6 | 0 | 12 | 0 | 0 | 0 | 0% | `doPostAttackEffectsWithItemSource` 3, `getComponentType` 1, `getDamageProtection` 1 |
| `world.level.Explosion` | 6 | 14 | 14 | 0 | 1 | 2 | 7% | `finalizeExplosion` 7, `explode` 4, `<init>` 1 |
| `tags.TagLoader` | 5 | 1 | 6 | 0 | 0 | 1 | 0% | `build` 3, `load` 2, `lambda$build$6` 1 |
| `world.level.BaseSpawner` | 5 | 3 | 4 | 0 | 0 | 0 | 0% | `serverTick` 4 |
| `world.level.block.Blocks` | 5 | 0 | 6 | 0 | 4 | 0 | 67% | `<clinit>` 4, `rebuildCache` 2 |
| `world.level.chunk.ChunkGenerator` | 5 | 1 | 12 | 0 | 1 | 0 | 8% | `findNearestMapStructure` 3, `getNearestGeneratedStructure` 3, `getMobsAt` 2 |
| `world.level.storage.loot.LootTable` | 5 | 17 | 4 | 0 | 0 | 0 | 0% | `getRandomItemsRaw` 2, `fill` 1, `getRandomItems` 1 |
| `world.level.storage.loot.LootPool` | 4 | 19 | 1 | 0 | 0 | 0 | 0% | `<clinit>` 1 |
| `server.level.ServerChunkCache` | 3 | 3 | 10 | 0 | 5 | 0 | 50% | `tickChunks` 8, `getChunkFutureMainThread` 1, `save` 1 |
| `world.item.alchemy.PotionBrewing` | 3 | 4 | 1 | 0 | 0 | 0 | 0% | `addVanillaMixes` 1 |
| `world.level.block.state.StateHolder` | 3 | 4 | 3 | 1 | 1 | 0 | 67% | `populateNeighbours` 2, `setValue` 1 |
| `commands.Commands` | 2 | 0 | 2 | 0 | 1 | 0 | 50% | `<init>` 1, `performCommand` 1 |
| `world.level.biome.Biome` | 2 | 1 | 2 | 1 | 0 | 0 | 50% | `getTemperature` 1, `shouldSnow` 1 |
| `world.level.block.LiquidBlock` | 2 | 1 | 1 | 0 | 1 | 0 | 100% | `isRandomlyTicking` 1 |
| `world.level.block.state.BlockBehaviour$BlockStateBase$Cache` | 2 | 3 | 2 | 0 | 2 | 0 | 100% | `<init>` 2 |
| `world.level.chunk.PalettedContainer` | 2 | 4 | 4 | 2 | 0 | 0 | 50% | `<init>` 1, `acquire` 1, `read` 1 |
| `world.level.entity.PersistentEntitySectionManager` | 2 | 1 | 1 | 0 | 1 | 0 | 100% | `lambda$updateChunkStatus$6` 1 |
| `world.level.levelgen.structure.pools.JigsawPlacement$Placer` | 2 | 2 | 2 | 0 | 1 | 0 | 50% | `tryPlacingChildren` 2 |

### 9.6 Mixins into other mods

Members of common or server mixin classes whose target class is in another jar of the pack: 881 members in 73 mods, into 339 classes of 46 mods. Most targeted mods by members: `create` 284, `ae2` 114, `kubejs` 94, `refinedstorage` 65, `ars_nouveau` 62, `mekanism` 58, `oritech` 32, `iris` 18.

The 25 most targeted classes of other mods, by mods, then by members:

| # | Class | Owner | Mods | Members | Intents | Top members |
|--:|:--|:--|--:|--:|:--|:--|
| 1 | `com.hollingsworth.arsnouveau.common.entity.EntityOrbitProjectile` | ars_nouveau | 4 | 5 | pre 2, post 3 | `<init>` 3, `onHit` 1, `tick` 1 |
| 2 | `com.hollingsworth.arsnouveau.api.spell.SpellSchool` | ars_nouveau | 4 | 4 | accessor 4 | `docIcon` 4 |
| 3 | `com.hollingsworth.arsnouveau.common.block.tile.BasicSpellTurretTile` | ars_nouveau | 4 | 4 | accessor 4 | `uuid` 4 |
| 4 | `com.hollingsworth.arsnouveau.setup.registry.Documentation` | ars_nouveau | 4 | 4 | post 4 | `getRecipePages` 4 |
| 5 | `com.simibubi.create.content.contraptions.Contraption` | create | 3 | 12 | pre 1, post 5, mid 1, value 1, accessor 4 | `addBlocksToWorld` 2, `blocks` 2, `readNBT` 2 |
| 6 | `dev.latvian.mods.kubejs.server.ServerScriptManager` | kubejs | 3 | 6 | pre 1, mid 3, cancel 1, value 1 | `createPackResources` 3, `loadAdditional` 1, `reload` 1 |
| 7 | `com.simibubi.create.content.schematics.cannon.SchematicannonBlockEntity` | create | 2 | 14 | pre 3, post 2, cancel 1, value 1, accessor 7 | `finishedPrinting` 2, `inventory` 2, `updateChecklist` 2 |
| 8 | `com.simibubi.create.content.schematics.ServerSchematicLoader` | create | 2 | 13 | mid 2, cancel 2, value 9 | `handleNewUpload` 5, `handleInstantSchematic` 3, `handleFinishedUpload` 2 |
| 9 | `com.simibubi.create.content.kinetics.fan.AirCurrent` | create | 2 | 12 | pre 1, value 4, accessor 7 | `rebuild` 3, `segments` 2, `source` 2 |
| 10 | `appeng.helpers.patternprovider.PatternProviderLogic` | ae2 | 2 | 10 | post 5, cancel 1, accessor 4 | `mainNode` 2, `<init>` 1, `actionSource` 1 |
| 11 | `com.simibubi.create.foundation.blockEntity.SmartBlockEntity` | create | 2 | 9 | pre 1, accessor 8 | `initialized` 2, `lazyTickCounter` 2, `lazyTickRate` 2 |
| 12 | `com.simibubi.create.content.kinetics.base.KineticBlockEntity` | create | 2 | 8 | value 2, accessor 6 | `flickerTally` 2, `validateKinetics` 2, `validationCountdown` 2 |
| 13 | `com.simibubi.create.content.kinetics.RotationPropagator` | create | 2 | 7 | post 1, cancel 2, value 3, accessor 1 | `findConnectedNeighbour` 2, `getAxisModifier` 1, `getPotentialNeighbourLocations` 1 |
| 14 | `com.refinedmods.refinedstorage.api.network.impl.autocrafting.AutocraftingNetworkComponentImpl` | refinedstorage | 2 | 6 | cancel 1, accessor 5 | `addTask` 1, `ensureTaskForCraftableAmount` 1, `patternRepository` 1 |
| 15 | `com.simibubi.create.content.kinetics.saw.SawBlockEntity` | create | 2 | 6 | value 1, accessor 5 | `recipeIndex` 2, `cuttingRecipesKey` 1, `filtering` 1 |
| 16 | `com.hollingsworth.arsnouveau.api.spell.SpellResolver` | ars_nouveau | 2 | 4 | pre 1, cancel 1, accessor 2 | `expendMana` 1, `getCastStats` 1, `onResolveEffect` 1 |
| 17 | `com.refinedmods.refinedstorage.api.autocrafting.task.TaskImpl` | refinedstorage | 2 | 4 | pre 1, accessor 3 | `completedPatterns` 1, `internalStorage` 1, `patterns` 1 |
| 18 | `com.simibubi.create.content.contraptions.AbstractContraptionEntity` | create | 2 | 4 | pre 1, cancel 1, accessor 2 | `contraption` 2, `handlePlayerInteraction` 1, `remove` 1 |
| 19 | `com.simibubi.create.content.fluids.spout.FillingBySpout` | create | 2 | 4 | cancel 4 | `getRequiredAmountForItem` 2, `canItemBeFilled` 1, `fillItem` 1 |
| 20 | `com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlockEntity` | create | 2 | 4 | mid 1, accessor 3 | `entityUUID` 2, `processingEntity` 1, `tick` 1 |
| 21 | `com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour` | create | 2 | 4 | cancel 1, accessor 3 | `lazyTickCounter` 2, `get` 1, `lazyTickRate` 1 |
| 22 | `dev.latvian.mods.kubejs.recipe.component.RecipeComponentBuilder` | kubejs | 2 | 4 | value 2, accessor 2 | `buildUniqueId` 2, `keys` 1, `mapCodec` 1 |
| 23 | `com.simibubi.create.content.kinetics.KineticNetwork` | create | 2 | 3 | value 2, accessor 1 | `calculateCapacity` 1, `calculateStress` 1, `unloadedStress` 1 |
| 24 | `com.simibubi.create.content.kinetics.mixer.MechanicalMixerBlockEntity` | create | 2 | 3 | post 2, accessor 1 | `getMatchingRecipes` 2, `shapelessOrMixingRecipesKey` 1 |
| 25 | `dev.latvian.mods.kubejs.recipe.component.RegistryComponent` | kubejs | 2 | 3 | value 2, accessor 1 | `lambda$static$0` 2, `registry` 1 |

### 9.7 Findings

- **Demand is flat.** The most targeted member, `RecipeManager.apply`, has 11 mods. 16 target members have 5 or more mods (section 9.4). 2674 members in 157 mods target 1931 members of 519 vanilla classes. The 200 most targeted members cover 26% of the members and 11% of the mods, and 21% of the mods when accessors are left out (section 9.3). No small set of patched methods ports the pack: with the best order, half of the mods need 97 target members of members that are not accessors, 80% need 329, and all mods need 1120.
- **Accessors are the largest intent class.** 1086 of the 2674 members (41%) are `@Shadow`, `@Accessor` or `@Invoker`. A source patch that opens the field or method serves them, or the NeoForge-shaped API when it exposes the field or method.
- **Most hooks sit at the start or the end of a method.** Of the 1588 members that are not accessors, 1042 (66%) act at the start or the end of the target method: pre 157, post 282, cancel at `HEAD` 331, cancel at `RETURN` or `TAIL` 144, `@WrapMethod` 26, `@ModifyReturnValue` 102. A NeoForge event that fires at that point, with cancel and a replaceable result, serves such a member. Where no event fires, a source patch serves it.
- **Call-site members need source patches.** 525 members that are not accessors (33%) act inside the method body: value modifiers other than `@ModifyReturnValue` 343, mid-method injections 123, cancellable injections at an inner point 59. Each needs a source patch at an identified call, field access, constant or local variable in the server source. The points seldom repeat: 520 of the 554 distinct call-site points have one member (section 9.5).
- **Whole-method overrides are rare.** 21 members (1%) are `@Overwrite`, 13 of them in the performance mods `modernfix` (10) and `ferritecore` (3). The classes that #51 lists have no `@Overwrite` and at most one `@Redirect` each: `RecipeManager` has 25 members that are not accessors, with 0 `@Overwrite` and 1 `@Redirect`; `LootTable`, `LootPool` and `PotionBrewing` are mostly accessors; `BaseSpawner` has 4 hooks at inner points of `serverTick`. NeoForge events and source patches serve their members. The classes whose members are mostly `@Overwrite` or `@Redirect` (`ServerChunkCache`, `StateHolder`, `PalettedContainer`, `BlockBehaviour$BlockStateBase$Cache`, `Biome`) are engine internals that `servercore`, `ferritecore` and `modernfix` replace for speed. They have no counterpart in a Rust server. `Blocks` is the exception: `amendments`, `apothic_enchanting` and `apothic_spawners` redirect its static initializer to construct their own subclass in place of a vanilla block. Those members become a source patch, or a NeoForge API row where the inventory has one.
- **Mods hook other mods.** 881 members in 73 mods target 339 classes of 46 other mods: `create` 284 members, `ae2` 114, `kubejs` 94, `refinedstorage` 65, `ars_nouveau` 62, `mekanism` 58. These are addons of the target mod. The most targeted class of another mod has 4 mods (section 9.6). A ported mod with addons fires its own events and exposes its own API to them. Source patches apply to the IronPumpkin tree only.
- **Mixins add state.** 443 `@Unique` members (293 fields, 150 methods) in 61 mods add state or methods to vanilla classes. Data attachments and extension traits cover them.
- **Conditional mixins.** 33 mods in the scope name a mixin plugin in their config. A plugin can turn mixins on or off at run time, for example when another mod is present. The port of such a mod needs a build-time or start-time condition on its event handlers and source patches.

## 10. What this means for 26.3

### 10.1 Method

The check follows #35. Manual renames from the primers of #35, used when the simple-name search finds nothing (6): `ResourceLocation` -> `Identifier`, `InteractionResultHolder` -> `InteractionResult`, `ItemInteractionResult` -> `InteractionResult`, `Tier` -> `ToolMaterial`, `UseAnim` -> `ItemUseAnimation`, `MobSpawnType` -> `EntitySpawnReason`.

Class names: a class is `same` when `n263` holds it. `n263` is the union of the NeoForge-patched 26.3 server jar (`minecraft-server-patched-26.3.0.52-beta`), the NeoForge universal jar `26.3.0.52-beta`, FML loader 12.0.8, EventBus 8.0.5, the mergetool API jar (holds `Dist`), and Brigadier 1.3.11, DataFixerUpper 10.0.21, logging 1.7.12 and authlib 10.0.77. The unpatched 26.3 server jar `server-26.3.jar` is indexed too, to tell vanilla members from members that NeoForge adds. Members: owner, name and erased descriptor are looked up in the owner and its super types (JDK supertypes through `javap`), as in #35. Results: `exact` (found in the unpatched jar), `exact-neo` (found only through NeoForge patches or the universal jar), `sig` (name found, other descriptor), `none` (no member of that name), `owner gone` (owner class not in 26.3), `unknown` (inherited from a library class outside the indexes). Client packages (`net.minecraft.client`, `com.mojang.blaze3d`) are not in the server artifacts and are not checked. A changed descriptor can still compile from source. Behaviour changes behind an unchanged signature are not detected, so every number here is a lower bound for breakage.

### 10.2 Totals

| Set | Classes | same | moved | GONE |
|:--|--:|--:|--:|--:|
| NeoForge classes, server side | 644 | 556 | 3 | 85 |
| Minecraft classes, server side (without client packages) | 2980 | 2095 | 227 | 658 |
| Mojang classes, server side | 165 | 157 | 1 | 7 |

Members referenced by server-side or mixed classes (27584 distinct, client owners excluded): exact 14831, exact through NeoForge 2759, signature changed 1830, no member of that name 2068, owner class gone 6057, unknown 39. That is 9955 members (36%) that no longer resolve as compiled.

381 of 414 mods reference at least one server-side member that does not resolve in 26.3.

### 10.3 NeoForge classes that are GONE in 26.3 (server side, by mods)

| # | Class | Mods (srv) | Refs |
|--:|:--|--:|--:|
| 1 | `neoforge.items.IItemHandler` | 139 | 1514 |
| 2 | `neoforge.capabilities.Capabilities$ItemHandler` | 112 | 434 |
| 3 | `neoforge.fluids.capability.IFluidHandler` | 89 | 649 |
| 4 | `neoforge.capabilities.Capabilities$FluidHandler` | 78 | 302 |
| 5 | `neoforge.fluids.capability.IFluidHandler$FluidAction` | 77 | 398 |
| 6 | `neoforge.energy.IEnergyStorage` | 75 | 465 |
| 7 | `neoforge.capabilities.Capabilities$EnergyStorage` | 73 | 267 |
| 8 | `fml.common.EventBusSubscriber$Bus` | 69 | 189 |
| 9 | `neoforge.items.ItemHandlerHelper` | 69 | 247 |
| 10 | `neoforge.items.ItemStackHandler` | 69 | 504 |
| 11 | `neoforge.items.IItemHandlerModifiable` | 65 | 341 |
| 12 | `neoforge.items.SlotItemHandler` | 55 | 186 |
| 13 | `neoforge.event.level.BlockEvent$BreakEvent` | 52 | 97 |
| 14 | `neoforge.fluids.capability.IFluidHandlerItem` | 45 | 148 |
| 15 | `neoforge.event.AddReloadListenerEvent` | 41 | 59 |
| 16 | `neoforge.common.util.TriState` | 40 | 89 |
| 17 | `neoforge.common.util.INBTSerializable` | 39 | 174 |
| 18 | `neoforge.items.wrapper.InvWrapper` | 39 | 80 |
| 19 | `neoforge.fluids.capability.templates.FluidTank` | 37 | 237 |
| 20 | `neoforge.energy.EnergyStorage` | 26 | 69 |
| 21 | `neoforge.common.DeferredSpawnEggItem` | 19 | 27 |
| 22 | `neoforge.common.EffectCure` | 17 | 35 |
| 23 | `neoforge.fluids.FluidActionResult` | 16 | 18 |
| 24 | `neoforge.items.wrapper.CombinedInvWrapper` | 15 | 50 |
| 25 | `neoforge.registries.DataPackRegistryEvent` | 15 | 19 |
| 26 | `neoforge.registries.DataPackRegistryEvent$NewRegistry` | 15 | 19 |
| 27 | `neoforge.fluids.IFluidTank` | 13 | 58 |
| 28 | `neoforge.event.village.VillagerTradesEvent` | 12 | 17 |
| 29 | `neoforge.items.wrapper.PlayerMainInvWrapper` | 12 | 20 |
| 30 | `neoforge.common.EffectCures` | 11 | 13 |
| 31 | `neoforge.items.ComponentItemHandler` | 11 | 38 |
| 32 | `neoforge.event.entity.player.PlayerInteractEvent$EntityInteractSpecific` | 10 | 15 |
| 33 | `neoforge.items.wrapper.RecipeWrapper` | 10 | 25 |
| 34 | `neoforge.network.handling.DirectionalPayloadHandler` | 10 | 10 |
| 35 | `neoforge.event.TagsUpdatedEvent$UpdateCause` | 9 | 9 |
| 36 | `neoforge.event.village.WandererTradesEvent` | 9 | 11 |
| 37 | `neoforge.event.brewing.RegisterBrewingRecipesEvent` | 8 | 8 |
| 38 | `neoforge.items.wrapper.SidedInvWrapper` | 8 | 9 |
| 39 | `neoforge.event.entity.EntityTeleportEvent$ChorusFruit` | 7 | 7 |
| 40 | `neoforge.fluids.capability.templates.FluidHandlerItemStack` | 7 | 14 |
| 41 | `neoforge.items.ItemHandlerCopySlot` | 7 | 9 |
| 42 | `neoforge.items.wrapper.RangedWrapper` | 7 | 12 |
| 43 | `neoforge.common.brewing.BrewingRecipe` | 6 | 12 |
| 44 | `neoforge.common.brewing.IBrewingRecipe` | 6 | 8 |
| 45 | `neoforge.common.util.LogicalSidedProvider` | 6 | 6 |
| 46 | `neoforge.fluids.capability.wrappers.FluidBucketWrapper` | 6 | 6 |
| 47 | `neoforge.items.wrapper.EmptyItemHandler` | 6 | 10 |
| 48 | `neoforge.common.BasicItemListing` | 5 | 5 |
| 49 | `neoforge.common.SimpleTier` | 5 | 6 |
| 50 | `neoforge.energy.ComponentEnergyStorage` | 4 | 8 |
| 51 | `neoforge.fluids.crafting.TagFluidIngredient` | 4 | 4 |
| 52 | `neoforge.items.wrapper.PlayerArmorInvWrapper` | 4 | 4 |
| 53 | `neoforge.items.wrapper.PlayerOffhandInvWrapper` | 4 | 4 |
| 54 | `neoforge.common.NeoForgeConfig` | 3 | 3 |
| 55 | `neoforge.common.NeoForgeConfig$Client` | 3 | 3 |
| 56 | `neoforge.energy.EmptyEnergyStorage` | 3 | 3 |
| 57 | `neoforge.event.entity.living.EnderManAngerEvent` | 3 | 3 |
| 58 | `neoforge.event.furnace.FurnaceFuelBurnTimeEvent` | 3 | 3 |
| 59 | `neoforge.fluids.capability.templates.EmptyFluidHandler` | 3 | 4 |
| 60 | `neoforge.fluids.capability.templates.VoidFluidHandler` | 3 | 3 |
| 61 | `neoforge.items.wrapper.PlayerInvWrapper` | 3 | 3 |
| 62 | `neoforge.registries.datamaps.builtin.Compostable` | 3 | 4 |
| 63 | `neoforge.registries.datamaps.builtin.FurnaceFuel` | 3 | 4 |
| 64 | `coremod.api.ASMAPI` | 2 | 35 |
| 65 | `neoforge.common.conditions.FalseCondition` | 2 | 3 |
| 66 | `neoforge.common.extensions.IAbstractMinecartExtension` | 2 | 2 |
| 67 | `neoforge.common.util.DataComponentUtil` | 2 | 2 |
| 68 | `neoforge.event.entity.player.AnvilRepairEvent` | 2 | 3 |
| 69 | `neoforge.fluids.capability.wrappers.BucketPickupHandlerWrapper` | 2 | 2 |
| 70 | `neoforge.fluids.crafting.SingleFluidIngredient` | 2 | 2 |
| 71 | `fml.loading.targets.CommonLaunchHandler` | 1 | 1 |
| 72 | `neoforge.common.WorldWorkerManager` | 1 | 1 |
| 73 | `neoforge.common.WorldWorkerManager$IWorker` | 1 | 1 |
| 74 | `neoforge.common.conditions.ItemExistsCondition` | 1 | 4 |
| 75 | `neoforge.common.conditions.TrueCondition` | 1 | 2 |
| 76 | `neoforge.common.crafting.CraftingHelper` | 1 | 4 |
| 77 | `neoforge.fluids.capability.templates.FluidHandlerItemStack$Consumable` | 1 | 1 |
| 78 | `neoforge.fluids.capability.templates.FluidHandlerItemStack$SwapEmpty` | 1 | 1 |
| 79 | `neoforge.fluids.capability.templates.FluidHandlerItemStackSimple` | 1 | 1 |
| 80 | `neoforge.items.wrapper.ForwardingItemHandler` | 1 | 1 |

5 more GONE NeoForge classes with fewer mods are not listed.

### 10.4 Classes that moved (same simple name in another package, or manual rename)

| # | Class (21.1) | Mods (srv) | Class in 26.3 |
|--:|:--|--:|:--|
| 1 | `resources.ResourceLocation` | 367 | `resources.Identifier` |
| 2 | `world.InteractionResultHolder` | 131 | `world.InteractionResult` |
| 3 | `Util` | 129 | `util.Util` |
| 4 | `world.level.GameRules` | 53 | `world.level.gamerules.GameRules` |
| 5 | `world.entity.MobSpawnType` | 46 | `world.entity.EntitySpawnReason` |
| 6 | `world.item.Tier` | 44 | `world.item.ToolMaterial` |
| 7 | `neoforge.fluids.FluidUtil` | 41 | `neoforge.transfer.fluid.FluidUtil` |
| 8 | `advancements.CriteriaTriggers` | 38 | `advancements.triggers.CriteriaTriggers` |
| 9 | `world.item.ArmorMaterial` | 37 | `world.item.equipment.ArmorMaterial` |
| 10 | `world.item.UseAnim` | 34 | `world.item.ItemUseAnimation` |
| 11 | `advancements.critereon.EntityPredicate` | 28 | `advancements.predicates.entity.EntityPredicate` |
| 12 | `advancements.CriterionTrigger` | 27 | `advancements.triggers.CriterionTrigger` |
| 13 | `advancements.critereon.ItemUsedOnLocationTrigger` | 27 | `advancements.triggers.ItemUsedOnLocationTrigger` |
| 14 | `world.entity.projectile.AbstractArrow` | 27 | `world.entity.projectile.arrow.AbstractArrow` |
| 15 | `world.entity.npc.VillagerProfession` | 26 | `world.entity.npc.villager.VillagerProfession` |
| 16 | `world.entity.npc.Villager` | 24 | `world.entity.npc.villager.Villager` |
| 17 | `advancements.critereon.MinMaxBounds` | 22 | `advancements.predicates.MinMaxBounds` |
| 18 | `advancements.critereon.SimpleCriterionTrigger` | 21 | `advancements.triggers.SimpleCriterionTrigger` |
| 19 | `advancements.critereon.SimpleCriterionTrigger$SimpleInstance` | 21 | `advancements.triggers.SimpleCriterionTrigger$SimpleInstance` |
| 20 | `world.entity.npc.VillagerTrades` | 21 | `world.item.trading.VillagerTrades` |
| 21 | `world.level.storage.loot.providers.number.ConstantValue` | 21 | `world.level.storage.loot.providers.number.floats.ConstantValue (+1)` |
| 22 | `advancements.critereon.ItemPredicate` | 20 | `advancements.predicates.ItemPredicate` |
| 23 | `advancements.critereon.MinMaxBounds$Ints` | 20 | `advancements.predicates.MinMaxBounds$Ints` |
| 24 | `world.entity.projectile.ThrowableItemProjectile` | 19 | `world.entity.projectile.throwableitemprojectile.ThrowableItemProjectile` |
| 25 | `Util$OS` | 18 | `util.Util$OS` |
| 26 | `world.entity.animal.horse.AbstractHorse` | 18 | `world.entity.animal.equine.AbstractHorse` |
| 27 | `world.entity.animal.Sheep` | 17 | `world.entity.animal.sheep.Sheep` |
| 28 | `world.entity.animal.Wolf` | 17 | `world.entity.animal.wolf.Wolf` |
| 29 | `world.entity.animal.horse.Horse` | 15 | `world.entity.animal.equine.Horse` |
| 30 | `advancements.Criterion` | 14 | `advancements.triggers.Criterion` |
| 31 | `world.entity.monster.Zombie` | 14 | `world.entity.monster.zombie.Zombie` |
| 32 | `world.entity.npc.VillagerData` | 14 | `world.entity.npc.villager.VillagerData` |
| 33 | `world.entity.monster.Slime` | 13 | `world.entity.monster.cubemob.Slime` |
| 34 | `world.entity.projectile.AbstractArrow$Pickup` | 13 | `world.entity.projectile.arrow.AbstractArrow$Pickup` |
| 35 | `advancements.critereon.StatePropertiesPredicate` | 12 | `advancements.predicates.StatePropertiesPredicate` |
| 36 | `core.RegistryCodecs` | 12 | `core.registries.codec.RegistryCodecs` |
| 37 | `world.entity.animal.Bee` | 12 | `world.entity.animal.bee.Bee` |
| 38 | `world.entity.animal.WaterAnimal` | 12 | `world.entity.animal.fish.WaterAnimal` |
| 39 | `world.entity.npc.AbstractVillager` | 12 | `world.entity.npc.villager.AbstractVillager` |
| 40 | `world.entity.vehicle.AbstractMinecart` | 12 | `world.entity.vehicle.minecart.AbstractMinecart` |
| 41 | `world.item.ArmorMaterials` | 12 | `world.item.equipment.ArmorMaterials` |
| 42 | `core.component.DataComponentPredicate` | 11 | `core.component.predicates.DataComponentPredicate` |
| 43 | `world.entity.projectile.Arrow` | 11 | `world.entity.projectile.arrow.Arrow` |
| 44 | `world.entity.vehicle.Boat` | 11 | `world.entity.vehicle.boat.Boat` |
| 45 | `advancements.critereon.ItemPredicate$Builder` | 10 | `advancements.predicates.ItemPredicate$Builder` |
| 46 | `world.entity.animal.Cat` | 10 | `world.entity.animal.feline.Cat` |
| 47 | `world.entity.animal.Parrot` | 10 | `world.entity.animal.parrot.Parrot` |
| 48 | `world.entity.npc.VillagerType` | 10 | `world.entity.npc.villager.VillagerType` |
| 49 | `advancements.critereon.MinMaxBounds$Doubles` | 9 | `advancements.predicates.MinMaxBounds$Doubles` |
| 50 | `world.entity.animal.Chicken` | 9 | `world.entity.animal.chicken.Chicken` |
| 51 | `world.entity.monster.Skeleton` | 9 | `world.entity.monster.skeleton.Skeleton` |
| 52 | `world.entity.npc.WanderingTrader` | 9 | `world.entity.npc.wanderingtrader.WanderingTrader` |
| 53 | `world.entity.projectile.AbstractHurtingProjectile` | 9 | `world.entity.projectile.hurtingprojectile.AbstractHurtingProjectile` |
| 54 | `world.ContainerListener` | 8 | `world.inventory.ContainerListener` |
| 55 | `world.entity.animal.Fox` | 8 | `world.entity.animal.fox.Fox` |
| 56 | `world.entity.animal.IronGolem` | 8 | `world.entity.animal.golem.IronGolem` |
| 57 | `world.entity.monster.Spider` | 8 | `world.entity.monster.spider.Spider` |
| 58 | `world.level.storage.loot.providers.number.UniformGenerator` | 8 | `world.level.storage.loot.providers.number.floats.UniformGenerator (+1)` |
| 59 | `advancements.critereon.ConsumeItemTrigger` | 7 | `advancements.triggers.ConsumeItemTrigger` |
| 60 | `advancements.critereon.EntityTypePredicate` | 7 | `advancements.predicates.entity.EntityTypePredicate` |
| 61 | `advancements.critereon.PlayerTrigger` | 7 | `advancements.triggers.PlayerTrigger` |
| 62 | `advancements.critereon.StatePropertiesPredicate$Builder` | 7 | `advancements.predicates.StatePropertiesPredicate$Builder` |
| 63 | `resources.RegistryFixedCodec` | 7 | `core.registries.codec.RegistryFixedCodec` |
| 64 | `world.entity.decoration.PaintingVariant` | 7 | `world.entity.decoration.painting.PaintingVariant` |
| 65 | `advancements.critereon.BlockPredicate` | 6 | `advancements.predicates.BlockPredicate (+2)` |
| 66 | `advancements.critereon.NbtPredicate` | 6 | `advancements.predicates.NbtPredicate` |
| 67 | `world.entity.animal.Cow` | 6 | `world.entity.animal.cow.Cow` |
| 68 | `world.entity.decoration.Painting` | 6 | `world.entity.decoration.painting.Painting` |
| 69 | `world.entity.monster.AbstractSkeleton` | 6 | `world.entity.monster.skeleton.AbstractSkeleton` |
| 70 | `world.entity.monster.ZombieVillager` | 6 | `world.entity.monster.zombie.ZombieVillager` |

160 more moved classes are not listed.

### 10.5 Minecraft classes that are GONE in 26.3 (server side, top 70 by mods)

| # | Class | Mods (srv) | Refs |
|--:|:--|--:|--:|
| 1 | `world.level.block.state.properties.DirectionProperty` | 135 | 1956 |
| 2 | `world.level.block.entity.BlockEntityType$Builder` | 112 | 124 |
| 3 | `world.ItemInteractionResult` | 110 | 678 |
| 4 | `world.level.storage.loot.parameters.LootContextParam` | 77 | 268 |
| 5 | `world.level.BlockAndTintGetter` | 71 | 422 |
| 6 | `MethodsReturnNonnullByDefault` | 53 | 1288 |
| 7 | `world.level.GameRules$Key` | 52 | 119 |
| 8 | `world.level.saveddata.SavedData$Factory` | 52 | 93 |
| 9 | `world.level.storage.DimensionDataStorage` | 52 | 92 |
| 10 | `world.inventory.ClickType` | 46 | 121 |
| 11 | `world.item.ArmorItem` | 46 | 222 |
| 12 | `util.FastColor` | 44 | 119 |
| 13 | `util.FastColor$ARGB32` | 42 | 106 |
| 14 | `world.level.storage.loot.parameters.LootContextParamSet` | 42 | 111 |
| 15 | `world.level.block.entity.BlockEntity$DataComponentInput` | 40 | 178 |
| 16 | `network.chat.Component$Serializer` | 38 | 79 |
| 17 | `world.item.SwordItem` | 35 | 73 |
| 18 | `world.item.Tiers` | 32 | 50 |
| 19 | `world.level.storage.loot.functions.LootItemFunctionType` | 32 | 81 |
| 20 | `world.level.storage.loot.providers.number.NumberProvider` | 32 | 86 |
| 21 | `world.item.ArmorItem$Type` | 31 | 178 |
| 22 | `world.level.storage.loot.entries.LootPoolSingletonContainer` | 28 | 54 |
| 23 | `commands.arguments.ResourceLocationArgument` | 27 | 42 |
| 24 | `world.item.AxeItem` | 27 | 48 |
| 25 | `ResourceLocationException` | 26 | 52 |
| 26 | `world.item.crafting.SimpleCraftingRecipeSerializer` | 26 | 28 |
| 27 | `world.item.crafting.SimpleCraftingRecipeSerializer$Factory` | 26 | 28 |
| 28 | `util.Tuple` | 25 | 161 |
| 29 | `world.level.portal.DimensionTransition` | 25 | 47 |
| 30 | `world.item.ArmorMaterial$Layer` | 24 | 40 |
| 31 | `world.item.alchemy.PotionBrewing` | 24 | 49 |
| 32 | `advancements.critereon.ContextAwarePredicate` | 21 | 48 |
| 33 | `world.entity.npc.VillagerTrades$ItemListing` | 21 | 79 |
| 34 | `world.item.PickaxeItem` | 21 | 40 |
| 35 | `world.item.ShovelItem` | 21 | 29 |
| 36 | `world.level.storage.loot.entries.LootPoolSingletonContainer$Builder` | 21 | 33 |
| 37 | `world.level.portal.DimensionTransition$PostDimensionTransition` | 20 | 33 |
| 38 | `FieldsAreNonnullByDefault` | 19 | 458 |
| 39 | `world.entity.monster.EnderMan` | 19 | 39 |
| 40 | `world.item.crafting.Ingredient$Value` | 19 | 33 |
| 41 | `world.level.levelgen.feature.FeaturePlaceContext` | 18 | 70 |
| 42 | `world.level.levelgen.feature.configurations.FeatureConfiguration` | 18 | 78 |
| 43 | `world.item.HoeItem` | 17 | 29 |
| 44 | `world.level.GameRules$Value` | 17 | 25 |
| 45 | `world.level.storage.loot.predicates.LootItemConditionType` | 17 | 58 |
| 46 | `server.packs.metadata.MetadataSectionSerializer` | 16 | 32 |
| 47 | `util.random.WeightedEntry` | 16 | 43 |
| 48 | `world.item.component.Unbreakable` | 16 | 20 |
| 49 | `world.level.GameRules$BooleanValue` | 16 | 23 |
| 50 | `world.level.block.FarmBlock` | 16 | 25 |
| 51 | `world.level.block.RedStoneWireBlock` | 16 | 27 |
| 52 | `world.item.Equipable` | 15 | 25 |
| 53 | `advancements.critereon.ItemSubPredicate` | 14 | 36 |
| 54 | `server.players.GameProfileCache` | 14 | 28 |
| 55 | `util.random.Weight` | 14 | 31 |
| 56 | `world.item.DiggerItem` | 14 | 22 |
| 57 | `world.item.crafting.Ingredient$TagValue` | 14 | 21 |
| 58 | `world.level.levelgen.placement.PlacementModifierType` | 14 | 37 |
| 59 | `world.level.storage.loot.entries.LootPoolEntryType` | 14 | 41 |
| 60 | `advancements.critereon.ItemSubPredicate$Type` | 13 | 25 |
| 61 | `world.entity.RelativeMovement` | 13 | 19 |
| 62 | `world.item.TieredItem` | 13 | 16 |
| 63 | `world.level.levelgen.feature.ConfiguredFeature` | 13 | 22 |
| 64 | `world.item.EnchantedBookItem` | 12 | 13 |
| 65 | `core.cauldron.CauldronInteraction$InteractionMap` | 11 | 17 |
| 66 | `util.random.WeightedEntry$Wrapper` | 10 | 28 |
| 67 | `world.item.ItemNameBlockItem` | 10 | 15 |
| 68 | `world.item.alchemy.PotionBrewing$Builder` | 10 | 14 |
| 69 | `world.level.GameRules$Type` | 10 | 14 |
| 70 | `world.level.storage.loot.providers.number.LootNumberProviderType` | 10 | 17 |

588 more GONE Minecraft classes are not listed. Many are datagen helpers or old names that a rename note in the primers explains (`ArmorItem`, `SwordItem` and the tool items became item properties in 1.21.5; `BlockEntityType$Builder` is removed in 1.21.2).

### 10.6 Members that do not resolve (top 60 by mods)

| Member | Result | Mods (srv) | Descriptor in 21.1 |
|:--|:--|--:|:--|
| `network.protocol.common.custom.CustomPacketPayload$Type.<init>` | sig | 193 | (Lnet/minecraft/resources/ResourceLocation;)V |
| `nbt.CompoundTag.getInt` | sig | 175 | (Ljava/lang/String;)I |
| `resources.ResourceKey.location` | none | 170 | ()Lnet/minecraft/resources/ResourceLocation; |
| `core.DefaultedRegistry.getKey` | sig | 160 | (Ljava/lang/Object;)Lnet/minecraft/resources/ResourceLocatio |
| `nbt.CompoundTag.getCompound` | sig | 159 | (Ljava/lang/String;)Lnet/minecraft/nbt/CompoundTag; |
| `tags.TagKey.create` | sig | 151 | (Lnet/minecraft/resources/ResourceKey;Lnet/minecraft/resourc |
| `nbt.CompoundTag.getString` | sig | 149 | (Ljava/lang/String;)Ljava/lang/String; |
| `nbt.CompoundTag.getBoolean` | sig | 137 | (Ljava/lang/String;)Z |
| `world.InteractionResult.SUCCESS` | sig | 137 | Lnet/minecraft/world/InteractionResult; |
| `fml.loading.FMLEnvironment.dist` | none | 126 | Lnet/neoforged/api/distmarker/Dist; |
| `world.level.block.state.BlockState.is` | sig | 123 | (Lnet/minecraft/world/level/block/Block;)Z |
| `world.InteractionResult.PASS` | sig | 122 | Lnet/minecraft/world/InteractionResult; |
| `resources.ResourceKey.create` | sig | 121 | (Lnet/minecraft/resources/ResourceKey;Lnet/minecraft/resourc |
| `world.item.ItemStack.is` | sig | 120 | (Lnet/minecraft/world/item/Item;)Z |
| `nbt.CompoundTag.getList` | sig | 116 | (Ljava/lang/String;I)Lnet/minecraft/nbt/ListTag; |
| `core.DefaultedRegistry.get` | sig | 113 | (Lnet/minecraft/resources/ResourceLocation;)Ljava/lang/Objec |
| `commands.CommandSourceStack.hasPermission` | none | 110 | (I)Z |
| `world.entity.player.Player.displayClientMessage` | none | 109 | (Lnet/minecraft/network/chat/Component;Z)V |
| `core.Registry.getKey` | sig | 106 | (Ljava/lang/Object;)Lnet/minecraft/resources/ResourceLocatio |
| `world.level.Level.playSound` | sig | 103 | (Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/co |
| `neoforge.network.PacketDistributor.sendToServer` | none | 103 | (Lnet/minecraft/network/protocol/common/custom/CustomPacketP |
| `fml.config.ModConfig$Type.COMMON` | none | 102 | Lnet/neoforged/fml/config/ModConfig$Type; |
| `nbt.CompoundTag.contains` | sig | 96 | (Ljava/lang/String;I)Z |
| `world.InteractionResult.FAIL` | sig | 93 | Lnet/minecraft/world/InteractionResult; |
| `tags.TagKey.location` | sig | 89 | ()Lnet/minecraft/resources/ResourceLocation; |
| `world.level.block.entity.BlockEntity.loadAdditional` | sig | 87 | (Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLo |
| `world.level.block.entity.BlockEntity.saveAdditional` | sig | 86 | (Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLo |
| `core.Registry.get` | sig | 81 | (Lnet/minecraft/resources/ResourceLocation;)Ljava/lang/Objec |
| `nbt.CompoundTag.getLong` | sig | 81 | (Ljava/lang/String;)J |
| `nbt.ListTag.getCompound` | sig | 81 | (I)Lnet/minecraft/nbt/CompoundTag; |
| `world.InteractionResult.sidedSuccess` | none | 79 | (Z)Lnet/minecraft/world/InteractionResult; |
| `world.level.Level.getRecipeManager` | none | 78 | ()Lnet/minecraft/world/item/crafting/RecipeManager; |
| `core.RegistryAccess.registryOrThrow` | none | 77 | (Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/R |
| `world.item.crafting.Ingredient.getItems` | none | 75 | ()[Lnet/minecraft/world/item/ItemStack; |
| `fml.config.ModConfig$Type.SERVER` | none | 75 | Lnet/neoforged/fml/config/ModConfig$Type; |
| `nbt.CompoundTag.remove` | sig | 74 | (Ljava/lang/String;)V |
| `sounds.SoundEvent.createVariableRangeEvent` | sig | 71 | (Lnet/minecraft/resources/ResourceLocation;)Lnet/minecraft/s |
| `nbt.CompoundTag.getUUID` | none | 70 | (Ljava/lang/String;)Ljava/util/UUID; |
| `nbt.CompoundTag.putUUID` | none | 69 | (Ljava/lang/String;Ljava/util/UUID;)V |
| `world.item.ItemStack.parseOptional` | none | 69 | (Lnet/minecraft/core/HolderLookup$Provider;Lnet/minecraft/nb |
| `world.item.crafting.RecipeManager.getAllRecipesFor` | none | 69 | (Lnet/minecraft/world/item/crafting/RecipeType;)Ljava/util/L |
| `nbt.CompoundTag.getAllKeys` | none | 67 | ()Ljava/util/Set; |
| `world.item.Item.appendHoverText` | sig | 67 | (Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/it |
| `world.level.Level.playSound` | sig | 67 | (Lnet/minecraft/world/entity/player/Player;DDDLnet/minecraft |
| `world.level.ChunkPos.<init>` | sig | 66 | (Lnet/minecraft/core/BlockPos;)V |
| `world.InteractionResult.CONSUME` | sig | 65 | Lnet/minecraft/world/InteractionResult; |
| `world.item.crafting.RecipeHolder.id` | sig | 65 | ()Lnet/minecraft/resources/ResourceLocation; |
| `world.entity.player.Player.drop` | sig | 64 | (Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/ |
| `world.entity.EntityType.create` | sig | 62 | (Lnet/minecraft/world/level/Level;)Lnet/minecraft/world/enti |
| `world.level.block.state.properties.BlockStateProperties.FACING` | sig | 62 | Lnet/minecraft/world/level/block/state/properties/DirectionP |
| `nbt.CompoundTag.getFloat` | sig | 60 | (Ljava/lang/String;)F |
| `world.level.block.state.properties.BlockStateProperties.HORIZONTAL_FACING` | sig | 60 | Lnet/minecraft/world/level/block/state/properties/DirectionP |
| `util.Mth.sin` | sig | 59 | (F)F |
| `world.entity.EntityType$Builder.build` | sig | 59 | (Ljava/lang/String;)Lnet/minecraft/world/entity/EntityType; |
| `core.Direction.getNormal` | none | 56 | ()Lnet/minecraft/core/Vec3i; |
| `world.entity.ai.attributes.AttributeModifier.<init>` | sig | 56 | (Lnet/minecraft/resources/ResourceLocation;DLnet/minecraft/w |
| `world.item.crafting.Ingredient.EMPTY` | none | 54 | Lnet/minecraft/world/item/crafting/Ingredient; |
| `world.entity.EntityType.is` | none | 53 | (Lnet/minecraft/tags/TagKey;)Z |
| `nbt.CompoundTag.getByte` | sig | 52 | (Ljava/lang/String;)B |
| `server.level.ServerLevel.getDataStorage` | sig | 52 | ()Lnet/minecraft/world/level/storage/DimensionDataStorage; |

## 11. Coverage against the `ironpumpkin-neo` contract

The contract is `mapping-table.md`: 1299 rows, 883 from the inventory of design.md (#34) and 416 from the classes of the single-mod scan (#35). Each row is matched against the scan, so the **mods** column says how many of the 414 mods need the row. Rows name 26.3 items, the scan sees 21.1 names, so the matching is by simple class name and member name. The rules:

- Event and class rows: mods that reference a class with this simple name (nested classes as `Outer$Inner`). NeoForge classes win over Minecraft classes of the same simple name.
- `Class#member` rows: mods with a member reference on an owner with this simple name and this member name. For an interface, mods that call a method with a declared name and descriptor on any owner count too (extension methods are called on `Entity`, `Level`, ...).
- Extension interface rows: mods that reference the interface plus mods that call one of its methods (exact name and descriptor). This is an upper bound for the methods whose descriptor is also a vanilla one.
- Annotation rows (`@Mod(value)`): mods whose annotation sets that element. Registry rows: mods that register into the registry (`DeferredRegister`, `Registry.register`, `RegisterEvent`); the touch count is in section 6. Package rows: mods that reference a class in exactly that package (`transfer.*` rows also count the 21.1 `items`, `fluids` and `energy` packages). `mods.toml` key rows: mods whose file has the key.
- Renames between 21.1 and 26.3 handled by an alias: `Capabilities.Energy/Fluid/Item` (21.1: `EnergyStorage`, `FluidHandler`, `ItemHandler`), `ModConfig.Type.LOCAL` (21.1 `COMMON`), `SYNCED` (21.1 `SERVER`). A row without a 21.1 counterpart shows `n/a` and counts 0.
- The 25 platform payload ids (`neoforge:network`, ...) are internal to NeoForge. A mod never references them, so they have no count.

### 11.1 Coverage by section

Status: **covered** = the row has a WIT counterpart (supported or planned), **partial** = calls work through a counterpart but overriding does not, **contract only** = the WIT type exists and the host rejects it until a phase backs it, **gap** = no counterpart (not supported). **Used rows** = rows needed by at least one mod. **Gap mods** = distinct mods that need at least one gap row of the section. Rows that do not exist in 21.1 count as unused.

| Part | Section | Rows | Used | >=10 mods | >=50 mods | Covered | Partial | Contract only | Gap | Gap mods |
|:--|:--|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| inv | Events | 282 | 218 | 91 | 12 | 249 | 0 | 0 | 33 | 115 |
| inv | Registries | 150 | 87 | 43 | 20 | 40 | 0 | 9 | 101 | 145 |
| inv | Capabilities | 35 | 31 | 21 | 12 | 33 | 0 | 0 | 2 | 0 |
| inv | Attachments | 26 | 21 | 13 | 7 | 23 | 0 | 0 | 3 | 0 |
| inv | Data maps | 24 | 19 | 2 | 0 | 24 | 0 | 0 | 0 | 0 |
| inv | Config | 36 | 33 | 25 | 17 | 33 | 0 | 0 | 3 | 4 |
| inv | Networking | 63 | 36 | 21 | 8 | 40 | 0 | 0 | 23 | 48 |
| inv | Lifecycle, entry points and metadata | 72 | 59 | 44 | 33 | 46 | 0 | 0 | 26 | 314 |
| inv | Loot modifiers, loot conditions and data conditions | 25 | 17 | 5 | 0 | 6 | 0 | 0 | 19 | 54 |
| inv | Extension interfaces NeoForge adds to `net.minecraft` | 58 | 34 | 22 | 12 | 0 | 43 | 0 | 15 | 11 |
| inv | Other packages | 87 | 47 | 30 | 11 | 29 | 0 | 0 | 58 | 213 |
| mod | NeoForge registries | 9 | 9 | 9 | 8 | 8 | 0 | 1 | 0 | 0 |
| mod | NeoForge events and event bus | 50 | 50 | 40 | 13 | 47 | 0 | 0 | 3 | 81 |
| mod | NeoForge mod loading and config | 10 | 10 | 10 | 10 | 10 | 0 | 0 | 0 | 0 |
| mod | NeoForge attachments | 2 | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 |
| mod | NeoForge networking | 6 | 6 | 5 | 5 | 6 | 0 | 0 | 0 | 0 |
| mod | NeoForge fluids | 6 | 6 | 6 | 1 | 6 | 0 | 0 | 0 | 0 |
| mod | NeoForge common utilities | 14 | 14 | 11 | 4 | 11 | 0 | 0 | 3 | 84 |
| mod | NeoForge loot and world modifiers | 6 | 4 | 3 | 0 | 2 | 0 | 0 | 4 | 14 |
| mod | Minecraft core, registries and tags | 29 | 29 | 28 | 25 | 29 | 0 | 0 | 0 | 0 |
| mod | Minecraft data components | 5 | 5 | 5 | 4 | 4 | 0 | 1 | 0 | 0 |
| mod | Minecraft utilities | 10 | 10 | 7 | 5 | 3 | 0 | 0 | 7 | 312 |
| mod | Minecraft sounds | 3 | 3 | 3 | 3 | 3 | 0 | 0 | 0 | 0 |
| mod | Minecraft items | 36 | 36 | 35 | 14 | 34 | 0 | 0 | 2 | 43 |
| mod | Minecraft enchantments | 6 | 6 | 6 | 4 | 6 | 0 | 0 | 0 | 0 |
| mod | Minecraft recipes and trading | 5 | 5 | 4 | 1 | 2 | 0 | 0 | 3 | 146 |
| mod | Minecraft blocks and block entities | 27 | 27 | 25 | 15 | 25 | 0 | 0 | 2 | 88 |
| mod | Minecraft entities | 44 | 44 | 31 | 12 | 43 | 0 | 0 | 1 | 22 |
| mod | Minecraft attributes | 9 | 9 | 9 | 5 | 9 | 0 | 0 | 0 | 0 |
| mod | Minecraft effects and damage | 9 | 8 | 8 | 5 | 8 | 0 | 0 | 1 | 0 |
| mod | Minecraft menus and containers | 11 | 11 | 10 | 9 | 7 | 0 | 0 | 4 | 176 |
| mod | Minecraft level and world | 28 | 28 | 26 | 20 | 23 | 0 | 0 | 5 | 186 |
| mod | Minecraft fluids and materials | 4 | 4 | 4 | 4 | 4 | 0 | 0 | 0 | 0 |
| mod | Minecraft world generation | 28 | 26 | 12 | 2 | 0 | 0 | 0 | 28 | 71 |
| mod | Minecraft loot | 4 | 4 | 4 | 3 | 4 | 0 | 0 | 0 | 0 |
| mod | Minecraft server and commands | 17 | 17 | 14 | 8 | 17 | 0 | 0 | 0 | 0 |
| mod | Minecraft network and chat | 8 | 8 | 7 | 7 | 5 | 0 | 0 | 3 | 263 |
| mod | Minecraft NBT and codecs | 2 | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 |
| mod | Minecraft data generation | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| mod | Mojang Brigadier | 10 | 10 | 10 | 9 | 10 | 0 | 0 | 0 | 0 |
| mod | Mojang DataFixerUpper codecs | 14 | 14 | 14 | 14 | 0 | 0 | 0 | 14 | 278 |
| mod | Mojang logging and auth | 2 | 2 | 2 | 2 | 2 | 0 | 0 | 0 | 0 |
|  | **All sections** | 1274 | 1011 | 669 | 348 | 855 | 43 | 11 | 365 | - |

405 of 414 mods need at least one gap row or contract-only row (the `mods.toml` metadata keys are left out; mixin and access transformer rows are in). 

### 11.2 Top 30 gaps by mods affected

A gap is a contract row without a WIT counterpart (gap) or whose registry the host rejects (contract only). Rows that share a cause are grouped, because they have one fix: for the classes of the single-mod scan the group is the reason text of `mapping-table.md`, for inventory rows it is the row, except that all static registries without an API, all `neoforge:` registries without an API, the mixin rows and the access transformer rows each form one group. The `mods.toml` metadata keys `modLoader`, `loaderVersion`, `license` and similar are not API and are left out. **Mods** is the number of distinct mods that need at least one row of the group. **Rows** shows the largest rows of the group and their mods.

| # | Gap | Mods | Rows | Largest rows (mods) | What the gap is |
|:--|:--|--:|--:|:--|:--|
| 1 | **Java helpers** | 312 | 5 | ChatFormatting(248), RandomSource(200), Mth(191), StringRepresentable(127) | Java helpers `ChatFormatting`, `RandomSource`, `Mth`, `StringRepresentable`: no WIT type, plugins use Rust std. Every port rewrites these calls; the host needs no work. |
| 2 | **Mixins** | 291 | 5 | [[mixins]] config(291), file mixins(278), [[mixins]] requiredMods(2), [[mixins]] behaviorVersion(0) | Mixins (`[[mixins]]` config, `@Mixin` classes): not supported. They patch the bytecode of the Java server, which Pumpkin does not have. A mod that needs one cannot be ported as it is. |
| 3 | **DFU codecs** | 278 | 14 | Codec(244), MapCodec(220), PrimitiveCodec(196), Products(195) | DataFixerUpper codecs (`Codec`, `MapCodec`, `RecordCodecBuilder`, `Products`): no codec types at the Wasm boundary. Data crosses as JSON, NBT or bytes. |
| 4 | **Stream codecs and byte buffers** | 263 | 3 | StreamCodec(252), RegistryFriendlyByteBuf(226), ByteBufCodecs(204) | `StreamCodec`, `RegistryFriendlyByteBuf`, `ByteBufCodecs`: a payload is the bytes of its encoding. The plugin encodes and decodes the bytes itself. |
| 5 | **Registries the host rejects** | 247 | 11 | DataComponentType(232), data_component_type(113), creative_mode_tab(112), sound_event(55) | Types with a WIT shape that the host rejects until a phase backs them: `data_component_type` (and `DeferredRegister.DataComponents`), `sound_event`, `attribute`, `mob_effect`, `fluid`, `neoforge:fluid_type`. `creative_mode_tab` entries are accepted and ignored. |
| 6 | **Access transformers** | 183 | 2 | file accessTransformers(55), [[accessTransformers]] file(55) | Access transformers (`accesstransformer.cfg`, `[[accessTransformers]]`): not supported. They widen member access in the Java server. |
| 7 | **Container menus** | 176 | 4 | AbstractContainerMenu(172), MenuProvider(121), MenuConstructor(51), SimpleMenuProvider(45) | Container menus (`AbstractContainerMenu`, `MenuProvider`, `MenuConstructor`): `registration.menu-type-definition` registers the menu type only. No open-menu function and no slot or quick-move callbacks yet. |
| 8 | **Block shapes** | 170 | 2 | VoxelShape(167), CollisionContext(134) | `VoxelShape`, `CollisionContext`: shapes come from the block type definition. A plugin cannot compute or return a shape. |
| 9 | **Ingredients and trades** | 146 | 3 | Ingredient(141), ItemCost(17), Merchant(6) | `Ingredient`, `ItemCost`, `Merchant`: recipes and trades are datapack JSON. There is no Java object API for them. |
| 10 | **Package common.util** | 133 | 3 | common.util(133), common.util.flag(0), common.util.strategy(0) | Package `common.util` (`FakePlayer`, `TriState`, `INBTSerializable`, `Lazy`, `BlockSnapshot`): no counterpart (`types` doc). |
| 11 | **Packages fluids** | 123 | 3 | fluids(122), fluids.crafting(28), fluids.crafting.display(0) | Packages `fluids` and `fluids.crafting` (`FluidStack`, `FluidUtil`, fluid ingredients): no package-level counterpart. `FluidType` and `BaseFlowingFluid` have rows of their own; handlers map to the planned `transfer.fluid` rows. |
| 12 | **Static registries without API** | 118 | 82 | recipe_serializer(74), recipe_type(49), particle_type(41), command_argument_type(21) | Static registries without a registration API (`recipe_serializer`, `recipe_type`, `particle_type`, command argument types, loot types, trigger types, worldgen types). Pumpkin crafts, loots and generates with fixed Rust sets. |
| 13 | **Creative tab contents** | 84 | 2 | BuildCreativeModeTabContentsEvent(75), CreativeModeTab.TabVisibility(26) | `BuildCreativeModeTabContentsEvent`, `CreativeModeTab.TabVisibility`: creative tab contents are client UI. The host accepts the tab entry and ignores it. |
| 14 | **RenderShape** | 81 | 1 | RenderShape(81) | `RenderShape`: client rendering. Common block classes reference it; a server plugin does not need it. |
| 15 | **World generation** | 72 | 31 | Heightmap(58), Heightmap.Types(58), Feature(24), GenerationStep(21) | World generation classes (`Heightmap`, `Feature`, `GenerationStep`, placement and structure types): Pumpkin generates worlds in Rust code (`handle-generate-phase`). No feature or placement API. |
| 16 | **FakePlayer** | 63 | 1 | FakePlayer(63) | `FakePlayer`: Pumpkin has no fake players. |
| 17 | **Package server** | 63 | 10 | server(59), server.command(9), server.command.generation(0), server.console(0) | Package `server` (`ServerLifecycleHooks.getCurrentServer`, 59 mods) and its command, console and timing subpackages: the `types` doc lists them as not supported. |
| 18 | **Custom game rules** | 53 | 1 | GameRules(53) | `GameRules`: reading vanilla rules works through `game-rules`. Registering custom rules needs a `game_rule` registry, which does not exist. |
| 19 | **Data conditions** | 52 | 11 | ICondition(42), ICondition.IContext(32), ConditionalOps(12), WithConditions(8) | Data conditions (`ICondition`, `ConditionalOps`, `WithConditions`, `ModLoadedCondition`): the datapack loader does not read `neoforge:conditions`. Data with conditions loads as if they were absent. |
| 20 | **Custom registries** | 48 | 4 | RegistryBuilder(47), NewRegistryEvent(31), DeferredRegister#makeRegistry(12), ModifyRegistriesEvent(2) | Custom registries (`RegistryBuilder`, `NewRegistryEvent`, `DeferredRegister#makeRegistry`): new registries need mutable registry types. Pumpkin registries are generated. |
| 21 | **Item abilities** | 45 | 2 | ItemAbility(45), ItemAbilities(41) | Item abilities (`ItemAbility`, `ItemAbilities`): tool actions have no Pumpkin equivalent. Only a name on the block tool modification event. |
| 22 | **Package common.crafting** | 43 | 1 | common.crafting(43) | Package `common.crafting` (`ICustomIngredient`, `IngredientType`, `SizedIngredient`, `DataComponentIngredient`): custom ingredient types. Recipes are datapack JSON; no counterpart. |
| 23 | **CommonHooks** | 42 | 1 | CommonHooks(42) | `CommonHooks` (`fireBlockBreak`, `canCropGrow`, `onRightClickBlock`): helper methods that fire NeoForge events by hand. No counterpart (`types` doc). |
| 24 | **Packages common.world** | 34 | 2 | common.world(22), common.world.chunk(13) | Packages `common.world` and `common.world.chunk` (biome modifier helpers, forced chunk tickets): no package-level counterpart. |
| 25 | **Package network.codec** | 34 | 1 | network.codec(34) | Package `network.codec` (`NeoForgeStreamCodecs`): stream codec helpers. Payloads are bytes (`network` doc). |
| 26 | **NeoForge registries without API** | 32 | 9 | neoforge:condition_codecs(16), neoforge:biome_modifier_serializers(10), neoforge:entity_data_serializers(9), neoforge:structure_modifier_serializers(1) | NeoForge registries without a registration API: `condition_codecs`, `biome_modifier_serializers`, `entity_data_serializers`, `structure_modifier_serializers`, `holder_set_type`, `ingredient_serializer`. |
| 27 | **ModList file data** | 30 | 8 | ModList#getModFileById(16), ModList#getAllScanData(16), ModList#getModFiles(3), ModList#forEachModFile(2) | `ModList#getModFileById`, `getAllScanData`, `getModFiles`: mod files and scan data describe Java jars. |
| 28 | **NeoForgeMod constants** | 30 | 1 | NeoForgeMod(30) | `NeoForgeMod` constants (`LAVA_TYPE`, `WATER_TYPE`, `SWIM_SPEED`, `CREATIVE_FLIGHT`, `MILK`, `enableMilkFluid`): no counterpart (`types` doc). |
| 29 | **Tooltip events** | 28 | 3 | ItemTooltipEvent(28), AddAttributeTooltipsEvent(0), GatherSkippedAttributeTooltipsEvent(0) | `ItemTooltipEvent` and the attribute tooltip events: tooltip lines are built by the client. Pumpkin never builds them. |
| 30 | **ArmorMaterial.Layer** | 24 | 1 | ArmorMaterial.Layer(24) | `ArmorMaterial.Layer`: client equipment asset. Armor is the `minecraft:equippable` component plus an equipment asset. |

Ranks 31 to 60, short form (mods, group): 23 SoundAction; 22 SpawnGroupData; 20 inv: SoundActions; 15 IEventBus#start; 15 Villager trade events; 13 mod: gap: flammability registration (`FireBlock#setFlammabl; 12 inv: IEventBus#unregister; 10 mod: gap: custom game rules not supported; 10 inv: RegisterTicketControllersEvent; 10 inv: UsernameCache; 8 inv: SpecialPlantable; 8 inv: network.connection; 7 inv: NetworkRegistry; 7 inv: PercentageAttribute; 7 inv: PlayerInteractEvent.LeftClickEmpty; 7 inv: registries.callback; 6 mod: gap: world generation, not supported; 6 inv: GameData; 6 inv: ModifyDefaultComponentsEvent; 5 inv: IAttributeExtension; 5 inv: IPayloadContext#connection; 5 inv: registries.holdersets; 4 inv: ConfigTracker; 3 inv: BooleanAttribute; 3 inv: ChunkTicketLevelUpdatedEvent; 3 inv: CreativeModeTabRegistry; 3 inv: EntityEvent.Size; 3 inv: IOwnedSpawner; 3 inv: PlayerSpawnPhantomsEvent; 3 inv: TranslatableEnum.

### 11.3 Used classes without a contract row

Server-side classes of the pack that no row of `mapping-table.md` names (neither as a class nor as an owner of a member row). 267 NeoForge, 2695 Minecraft and 139 Mojang classes. The contract lists 313 Minecraft and Mojang classes from the single-mod scan, so most of the Minecraft surface of the pack is outside it: that is the long tail of vanilla API, which the `pumpkin:plugin` interfaces cover or not. Top 60 by mods; the 26.3 column says whether the class still exists.

| # | Class | Mods (srv) | Mods (all) | 26.3 |
|--:|:--|--:|--:|:--|
| 1 | `network.FriendlyByteBuf` | 209 | 212 | same |
| 2 | `server.MinecraftServer` | 200 | 203 | same |
| 3 | `world.phys.BlockHitResult` | 191 | 203 | same |
| 4 | `core.Vec3i` | 177 | 190 | same |
| 5 | `world.item.TooltipFlag` | 173 | 185 | same |
| 6 | `world.item.Item$TooltipContext` | 168 | 179 | same |
| 7 | `com.mojang.serialization.DataResult` | 158 | 169 | same |
| 8 | `world.level.block.state.StateDefinition` | 154 | 159 | same |
| 9 | `world.level.LevelReader` | 153 | 156 | same |
| 10 | `network.chat.Style` | 149 | 177 | same |
| 11 | `world.level.block.state.StateDefinition$Builder` | 141 | 142 | same |
| 12 | `world.level.material.FluidState` | 141 | 152 | same |
| 13 | `neoforge.items.IItemHandler` | 139 | 140 | GONE |
| 14 | `com.mojang.serialization.DynamicOps` | 138 | 146 | same |
| 15 | `world.item.context.BlockPlaceContext` | 137 | 140 | same |
| 16 | `world.level.block.state.properties.BooleanProperty` | 137 | 139 | same |
| 17 | `world.level.block.state.properties.DirectionProperty` | 135 | 138 | GONE |
| 18 | `world.item.context.UseOnContext` | 134 | 135 | same |
| 19 | `world.level.block.state.properties.BlockStateProperties` | 133 | 135 | same |
| 20 | `fml.loading.FMLEnvironment` | 132 | 146 | same |
| 21 | `network.protocol.Packet` | 130 | 131 | same |
| 22 | `Util` | 129 | 164 | moved |
| 23 | `nbt.ListTag` | 129 | 131 | same |
| 24 | `network.codec.StreamDecoder` | 123 | 126 | same |
| 25 | `core.Direction$Axis` | 122 | 134 | same |
| 26 | `world.phys.shapes.Shapes` | 119 | 123 | same |
| 27 | `world.item.crafting.Recipe` | 118 | 136 | same |
| 28 | `world.item.crafting.RecipeHolder` | 118 | 122 | same |
| 29 | `world.item.crafting.RecipeInput` | 118 | 122 | same |
| 30 | `world.level.material.Fluids` | 118 | 124 | same |
| 31 | `world.item.crafting.RecipeType` | 116 | 120 | same |
| 32 | `com.mojang.datafixers.util.Function4` | 112 | 117 | same |
| 33 | `world.item.crafting.RecipeSerializer` | 112 | 114 | same |
| 34 | `world.ItemInteractionResult` | 110 | 110 | GONE |
| 35 | `world.item.crafting.RecipeManager` | 109 | 115 | same |
| 36 | `neoforge.fluids.FluidStack` | 108 | 110 | same |
| 37 | `server.packs.resources.ResourceManager` | 105 | 136 | same |
| 38 | `network.chat.FormattedText` | 101 | 172 | same |
| 39 | `server.network.ServerGamePacketListenerImpl` | 101 | 102 | same |
| 40 | `world.level.block.state.StateHolder` | 99 | 99 | same |
| 41 | `network.codec.StreamCodec$CodecOperation` | 97 | 98 | same |
| 42 | `network.codec.StreamEncoder` | 96 | 99 | same |
| 43 | `world.level.block.state.properties.EnumProperty` | 95 | 102 | same |
| 44 | `nbt.NbtOps` | 94 | 98 | same |
| 45 | `server.players.PlayerList` | 93 | 95 | same |
| 46 | `bus.api.Event` | 93 | 104 | same |
| 47 | `com.mojang.datafixers.Products$P4` | 92 | 99 | same |
| 48 | `network.protocol.game.ClientboundBlockEntityDataPacket` | 91 | 91 | same |
| 49 | `com.mojang.datafixers.util.Function5` | 90 | 95 | same |
| 50 | `core.component.DataComponentPatch` | 90 | 94 | same |
| 51 | `neoforge.fluids.capability.IFluidHandler` | 89 | 89 | GONE |
| 52 | `world.level.block.Rotation` | 88 | 91 | same |
| 53 | `core.RegistryAccess$Frozen` | 87 | 94 | same |
| 54 | `api.distmarker.OnlyIn` | 84 | 111 | same |
| 55 | `neoforgespi.language.IModInfo` | 81 | 92 | same |
| 56 | `fml.event.lifecycle.FMLClientSetupEvent` | 80 | 181 | same |
| 57 | `world.item.crafting.CraftingInput` | 79 | 83 | same |
| 58 | `world.level.biome.Biome` | 79 | 87 | same |
| 59 | `com.mojang.datafixers.util.Either` | 78 | 88 | same |
| 60 | `resources.RegistryOps` | 78 | 80 | same |

Top 30 NeoForge classes without a row: `neoforge.items.IItemHandler`(139)*, `fml.loading.FMLEnvironment`(132), `neoforge.fluids.FluidStack`(108), `bus.api.Event`(93), `neoforge.fluids.capability.IFluidHandler`(89)*, `api.distmarker.OnlyIn`(84), `neoforgespi.language.IModInfo`(81), `fml.event.lifecycle.FMLClientSetupEvent`(80), `fml.loading.FMLPaths`(77), `neoforge.fluids.capability.IFluidHandler$FluidAction`(77)*, `neoforge.energy.IEnergyStorage`(75)*, `neoforge.items.ItemHandlerHelper`(69)*, `neoforge.items.ItemStackHandler`(69)*, `neoforge.items.IItemHandlerModifiable`(65)*, `neoforge.server.ServerLifecycleHooks`(59), `neoforge.items.SlotItemHandler`(55)*, `neoforge.event.level.BlockEvent$BreakEvent`(52)*, `fml.ModLoadingContext`(45), `neoforge.fluids.capability.IFluidHandlerItem`(45)*, `neoforge.event.AddReloadListenerEvent`(41)*, `neoforge.fluids.FluidUtil`(41)*, `fml.InterModComms`(40), `neoforge.common.util.TriState`(40)*, `neoforge.common.util.INBTSerializable`(39)*, `neoforge.items.wrapper.InvWrapper`(39)*, `neoforge.fluids.capability.templates.FluidTank`(37)*, `fml.loading.LoadingModList`(35), `fml.IExtensionPoint`(34), `fml.LogicalSide`(34), `neoforge.network.codec.NeoForgeStreamCodecs`(34). `*` marks a class that is moved or GONE in 26.3.

Classes without a row, by mods: >=50: 119, 10-49: 520, 2-9: 1257, 1: 1205.

### 11.4 Order of work: inventory rows by status

The inventory rows have the status `supported` (works today), `planned` (WIT counterpart, waits for its phase) or `not supported`; the `mods.toml` key rows are left out. The planned rows are the work items of phases 4 and 6, so the mods count says which ones to build first.

| Status | Rows | Used | >=10 mods | >=50 mods | Mods needing at least one |
|:--|--:|--:|--:|--:|--:|
| not supported | 277 | 127 | 45 | 8 | 282 |
| planned | 446 | 364 | 212 | 98 | 412 |
| supported | 99 | 83 | 37 | 7 | 257 |

Top 50 planned rows by mods (`all` counts client and dev classes too):

| # | Row | Section | Mods | All |
|--:|:--|:--|--:|--:|
| 1 | `@Mod(value)` | Entry point annotations | 411 | 411 |
| 2 | `Level` | Attachment holders | 310 | 324 |
| 3 | `IBlockStateExtension` | Extension interfaces NeoForge adds to net.minecraft | 274 | 286 |
| 4 | `IEventBus#addListener` | Event bus and mod container | 263 | 304 |
| 5 | `Entity` | Attachment holders | 249 | 267 |
| 6 | `BlockEntity` | Attachment holders | 235 | 240 |
| 7 | `DeferredHolder` | Registration API | 224 | 230 |
| 8 | `NeoForge` | Types in the root common package | 219 | 248 |
| 9 | `DeferredRegister#register` | Registration API | 203 | 207 |
| 10 | `DeferredRegister#create` | Registration API | 201 | 205 |
| 11 | `ModList#get` | Event bus and mod container | 200 | 224 |
| 12 | `transfer` | Packages outside the families above | 189 | 192 |
| 13 | `@EventBusSubscriber(modid)` | Entry point annotations | 186 | 186 |
| 14 | `ModConfigSpec` | Config API | 182 | 195 |
| 15 | `ModConfigSpec` | Types in the root common package | 182 | 195 |
| 16 | `ModList#isLoaded` | Event bus and mod container | 175 | 199 |
| 17 | `RegisterPayloadHandlersEvent` | event class | 170 | 176 |
| 18 | `IPayloadHandler` | Configuration tasks and other network types | 166 | 170 |
| 19 | `ModConfig` | Config API | 165 | 191 |
| 20 | `ModConfig.Type` | Config API | 165 | 191 |
| 21 | `IConfigSpec` | Config API | 162 | 188 |
| 22 | `ModContainer#registerConfig` | Event bus and mod container | 159 | 185 |
| 23 | `ModConfigSpec.Builder#comment` | Config API | 156 | 167 |
| 24 | `IPayloadContext#player` | Payload registration and sending | 154 | 157 |
| 25 | `BlockCapability` | Capability API | 153 | 158 |
| 26 | `@EventBusSubscriber(value)` | Entry point annotations | 151 | 151 |
| 27 | `IPayloadContext#enqueueWork` | Payload registration and sending | 151 | 174 |
| 28 | `transfer.item` | Packages outside the families above | 149 | 150 |
| 29 | `FMLCommonSetupEvent` | event class | 148 | 168 |
| 30 | `ModConfigSpec.Builder#define` | Config API | 140 | 155 |
| 31 | `Capabilities` | Capability API | 138 | 141 |
| 32 | `ModConfigSpec.BooleanValue` | Config API | 138 | 152 |
| 33 | `ModConfigSpec.Builder#defineInRange` | Config API | 134 | 139 |
| 34 | `ModConfigSpec.Builder#push` | Config API | 132 | 137 |
| 35 | `item` | Static registries a DeferredRegister can target | 132 | 255 |
| 36 | `RegisterCapabilitiesEvent` | event class | 131 | 145 |
| 37 | `ModConfigSpec.Builder#pop` | Config API | 130 | 136 |
| 38 | `transfer.fluid` | Packages outside the families above | 130 | 132 |
| 39 | `ILevelExtension` | Extension interfaces NeoForge adds to net.minecraft | 127 | 135 |
| 40 | `PayloadRegistrar#playToClient` | Payload registration and sending | 125 | 127 |
| 41 | `ModConfigSpec.Builder#build` | Config API | 123 | 132 |
| 42 | `PayloadRegistrar#playToServer` | Payload registration and sending | 123 | 124 |
| 43 | `IItemStackExtension` | Extension interfaces NeoForge adds to net.minecraft | 122 | 129 |
| 44 | `ICapabilityProvider` | Capability API | 119 | 131 |
| 45 | `ModConfigSpec.IntValue` | Config API | 119 | 123 |
| 46 | `Capabilities.Item` | Capability API | 112 | 116 |
| 47 | `IEventBus#register` | Event bus and mod container | 112 | 133 |
| 48 | `NeoForgeRegistries` | Registration API | 111 | 116 |
| 49 | `Capabilities.Item.BLOCK` | Built-in capabilities | 107 | 112 |
| 50 | `DeferredItem` | Registration API | 104 | 105 |

364 planned rows are needed by at least one mod, 212 by 10 or more, 98 by 50 or more.

### 11.5 Every contract row with the number of mods

Rows used by at least one mod, by section; `cls` is cov (covered), par (partial), con (contract only), gap. `mods` is the srv count, `all` includes client and dev classes. Unused rows are counted per section and not listed. Rows with `n/a` do not exist in 21.1.

#### Inventory: Events (282 rows, 60 unused, 28 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `RegisterPayloadHandlersEvent` | cov | 170 | 176 |
| `FMLCommonSetupEvent` | cov | 148 | 168 |
| `RegisterCapabilitiesEvent` | cov | 131 | 145 |
| `PlayerEvent` | cov | 96 | 100 |
| `RegisterCommandsEvent` | cov | 93 | 98 |
| `ModConfigEvent` | cov | 86 | 92 |
| `PlayerInteractEvent` | cov | 85 | 89 |
| `BuildCreativeModeTabContentsEvent` | gap | 75 | 89 |
| `PlayerInteractEvent.RightClickBlock` | cov | 67 | 69 |
| `PlayerEvent.PlayerLoggedInEvent` | cov | 66 | 72 |
| `BlockEvent` | cov | 65 | 66 |
| `ServerTickEvent` | cov | 52 | 54 |
| `EntityJoinLevelEvent` | cov | 49 | 53 |
| `ModConfigEvent.Reloading` | cov | 48 | 54 |
| `RegisterEvent` | cov | 46 | 55 |
| `LivingDeathEvent` | cov | 46 | 47 |
| `ModConfigEvent.Loading` | cov | 43 | 49 |
| `ServerTickEvent.Post` | cov | 43 | 45 |
| `LevelEvent` | cov | 42 | 50 |
| `LevelTickEvent` | cov | 40 | 46 |
| `ServerStartedEvent` | cov | 40 | 43 |
| `LivingDamageEvent` | cov | 38 | 39 |
| `LivingIncomingDamageEvent` | cov | 37 | 38 |
| `PlayerTickEvent` | cov | 36 | 42 |
| `ServerStoppedEvent` | cov | 36 | 38 |
| `LevelTickEvent.Post` | cov | 32 | 37 |
| `InterModEnqueueEvent` | cov | 31 | 34 |
| `NewRegistryEvent` | gap | 31 | 34 |
| `LivingDropsEvent` | cov | 31 | 31 |
| `LevelEvent.Unload` | cov | 30 | 39 |
| `EntityAttributeCreationEvent` | cov | 29 | 32 |
| `OnDatapackSyncEvent` | cov | 29 | 31 |
| `ItemTooltipEvent` | gap | 28 | 68 |
| `PlayerEvent.PlayerLoggedOutEvent` | cov | 28 | 29 |
| `LevelEvent.Load` | cov | 27 | 32 |
| `PlayerInteractEvent.LeftClickBlock` | cov | 27 | 31 |
| `EntityTickEvent` | cov | 27 | 28 |
| `LivingDamageEvent.Pre` | cov | 27 | 27 |
| `ServerStartingEvent` | cov | 26 | 31 |
| `PlayerEvent.PlayerChangedDimensionEvent` | cov | 26 | 27 |
| `LivingDamageEvent.Post` | cov | 25 | 26 |
| `PlayerTickEvent.Post` | cov | 24 | 30 |
| `FMLLoadCompleteEvent` | cov | 24 | 29 |
| `ServerAboutToStartEvent` | cov | 24 | 28 |
| `PlayerEvent.PlayerRespawnEvent` | cov | 24 | 25 |
| `ServerStoppingEvent` | cov | 24 | 25 |
| `PlayerEvent.Clone` | cov | 23 | 24 |
| `PlayerInteractEvent.RightClickItem` | cov | 23 | 24 |
| `BlockEvent.EntityPlaceEvent` | cov | 23 | 23 |
| `EntityTeleportEvent` | cov | 22 | 22 |
| `LivingEvent` | cov | 22 | 22 |
| `ItemEntityPickupEvent` | cov | 20 | 21 |
| `TagsUpdatedEvent` | cov | 19 | 21 |
| `LevelTickEvent.Pre` | cov | 19 | 20 |
| `PlayerTickEvent.Pre` | cov | 18 | 19 |
| `PlayerEvent.BreakSpeed` | cov | 18 | 18 |
| `EntityTickEvent.Post` | cov | 18 | 18 |
| `ChunkEvent` | cov | 17 | 19 |
| `ItemEntityPickupEvent.Pre` | cov | 17 | 18 |
| `RegisterSpawnPlacementsEvent` | cov | 17 | 17 |
| `BlockDropsEvent` | cov | 17 | 17 |
| `ServerTickEvent.Pre` | cov | 17 | 17 |
| `EntityAttributeModificationEvent` | cov | 16 | 18 |
| `RegisterDataMapTypesEvent` | cov | 16 | 18 |
| `FinalizeSpawnEvent` | cov | 16 | 16 |
| `LivingEntityUseItemEvent` | cov | 16 | 16 |
| `PlayerInteractEvent.EntityInteract` | cov | 16 | 16 |
| `ChunkEvent.Unload` | cov | 15 | 18 |
| `AttackEntityEvent` | cov | 15 | 16 |
| `LivingEvent.LivingJumpEvent` | cov | 15 | 15 |
| `LivingChangeTargetEvent` | cov | 14 | 15 |
| `LivingFallEvent` | cov | 14 | 14 |
| `AddPackFindersEvent` | cov | 13 | 21 |
| `MobEffectEvent` | cov | 13 | 14 |
| `LivingExperienceDropEvent` | cov | 13 | 13 |
| `LivingHealEvent` | cov | 13 | 13 |
| `PlayerEvent.StartTracking` | cov | 13 | 13 |
| `ExplosionEvent` | cov | 12 | 13 |
| `EntityTickEvent.Pre` | cov | 12 | 13 |
| `ProjectileImpactEvent` | cov | 12 | 12 |
| `ItemAttributeModifierEvent` | cov | 11 | 11 |
| `MobSpawnEvent` | cov | 11 | 11 |
| `PlayerEvent.ItemCraftedEvent` | cov | 11 | 11 |
| `ChunkWatchEvent` | cov | 11 | 11 |
| `InterModProcessEvent` | cov | 10 | 14 |
| `RegisterTicketControllersEvent` | gap | 10 | 13 |
| `ChunkEvent.Load` | cov | 10 | 12 |
| `ExplosionEvent.Detonate` | cov | 10 | 11 |
| `EntityEvent` | cov | 10 | 10 |
| `LivingEquipmentChangeEvent` | cov | 10 | 10 |
| `PlayerContainerEvent` | cov | 10 | 10 |
| `EntityLeaveLevelEvent` | cov | 9 | 13 |
| `AnvilUpdateEvent` | cov | 9 | 11 |
| `EntityInvulnerabilityCheckEvent` | cov | 9 | 9 |
| `EntityMountEvent` | cov | 9 | 9 |
| `LivingEntityUseItemEvent.Finish` | cov | 9 | 9 |
| `LivingEntityUseItemEvent.Start` | cov | 9 | 9 |
| `LivingKnockBackEvent` | cov | 9 | 9 |
| `ItemEntityPickupEvent.Post` | cov | 9 | 9 |
| `PlayerContainerEvent.Open` | cov | 9 | 9 |
| `BlockEntityTypeAddBlocksEvent` | cov | 8 | 9 |
| `ItemTossEvent` | cov | 8 | 8 |
| `LivingShieldBlockEvent` | cov | 8 | 8 |
| `MobEffectEvent.Added` | cov | 8 | 8 |
| `PlayerInteractEvent.LeftClickEmpty` | gap | 7 | 14 |
| `MobEffectEvent.Applicable` | cov | 7 | 8 |
| `AdvancementEvent` | cov | 7 | 8 |
| `PlayerXpEvent` | cov | 7 | 8 |
| `BlockEvent.BlockToolModificationEvent` | cov | 7 | 8 |
| `LootTableLoadEvent` | cov | 7 | 7 |
| `MobSpawnEvent.PositionCheck` | cov | 7 | 7 |
| `RegisterConfigurationTasksEvent` | cov | 7 | 7 |
| `ModifyDefaultComponentsEvent` | gap | 6 | 8 |
| `EntityTravelToDimensionEvent` | cov | 6 | 7 |
| `AdvancementEvent.AdvancementEarnEvent` | cov | 6 | 7 |
| `PlayerXpEvent.PickupXp` | cov | 6 | 7 |
| `ServerChatEvent` | cov | 6 | 6 |
| `EntityTeleportEvent.TeleportCommand` | cov | 6 | 6 |
| `MobEffectEvent.Remove` | cov | 6 | 6 |
| `BlockEvent.NeighborNotifyEvent` | cov | 6 | 6 |
| `ChunkWatchEvent.Sent` | cov | 6 | 6 |
| `ExplosionEvent.Start` | cov | 6 | 6 |
| `ModConfigEvent.Unloading` | cov | 6 | 6 |
| `PermissionGatherEvent` | cov | 6 | 6 |
| `PermissionGatherEvent.Nodes` | cov | 6 | 6 |
| `CommandEvent` | cov | 5 | 5 |
| `EntityTeleportEvent.EnderEntity` | cov | 5 | 5 |
| `EntityTeleportEvent.EnderPearl` | cov | 5 | 5 |
| `LivingBreatheEvent` | cov | 5 | 5 |
| `LivingConversionEvent` | cov | 5 | 5 |
| `MobEffectEvent.Expired` | cov | 5 | 5 |
| `CriticalHitEvent` | cov | 5 | 5 |
| `PlayerContainerEvent.Close` | cov | 5 | 5 |
| `PlayerEvent.HarvestCheck` | cov | 5 | 5 |
| `ChunkDataEvent` | cov | 5 | 5 |
| `ChunkDataEvent.Load` | cov | 5 | 5 |
| `ChunkWatchEvent.UnWatch` | cov | 5 | 5 |
| `ChunkWatchEvent.Watch` | cov | 5 | 5 |
| `LevelEvent.Save` | cov | 5 | 5 |
| `FMLConstructModEvent` | cov | 4 | 5 |
| `LivingEntityUseItemEvent.Stop` | cov | 4 | 4 |
| `LivingEntityUseItemEvent.Tick` | cov | 4 | 4 |
| `LivingEvent.LivingVisibilityEvent` | cov | 4 | 4 |
| `MobDespawnEvent` | cov | 4 | 4 |
| `MobSpawnEvent.SpawnPlacementCheck` | cov | 4 | 4 |
| `BonemealEvent` | cov | 4 | 4 |
| `PlayerDestroyItemEvent` | cov | 4 | 4 |
| `PlayerEvent.ItemSmeltedEvent` | cov | 4 | 4 |
| `UseItemOnBlockEvent` | cov | 4 | 4 |
| `BlockEvent.EntityMultiPlaceEvent` | cov | 4 | 4 |
| `BlockEvent.FarmlandTrampleEvent` | cov | 4 | 4 |
| `ChunkDataEvent.Save` | cov | 4 | 4 |
| `DataMapsUpdatedEvent` | cov | 4 | 4 |
| `GameShuttingDownEvent` | cov | 3 | 4 |
| `GrindstoneEvent` | cov | 3 | 3 |
| `GrindstoneEvent.OnPlaceItem` | cov | 3 | 3 |
| `GrindstoneEvent.OnTakeItem` | cov | 3 | 3 |
| `EntityEvent.Size` | gap | 3 | 3 |
| `EntityMobGriefingEvent` | cov | 3 | 3 |
| `ArmorHurtEvent` | cov | 3 | 3 |
| `BabyEntitySpawnEvent` | cov | 3 | 3 |
| `LivingConversionEvent.Post` | cov | 3 | 3 |
| `LivingConversionEvent.Pre` | cov | 3 | 3 |
| `LivingDestroyBlockEvent` | cov | 3 | 3 |
| `PlayerRespawnPositionEvent` | cov | 3 | 3 |
| `PlayerSpawnPhantomsEvent` | gap | 3 | 3 |
| `ChunkTicketLevelUpdatedEvent` | gap | 3 | 3 |
| `NoteBlockEvent` | cov | 3 | 3 |
| `NoteBlockEvent.Play` | cov | 3 | 3 |
| `VillageSiegeEvent` | gap | 3 | 3 |
| `PlayerInteractEvent.RightClickEmpty` | gap | 2 | 4 |
| `PlayLevelSoundEvent` | cov | 2 | 3 |
| `PlayLevelSoundEvent.AtPosition` | cov | 2 | 3 |
| `ItemStackedOnOtherEvent` | cov | 2 | 2 |
| `PlayLevelSoundEvent.AtEntity` | cov | 2 | 2 |
| `GetEnchantmentLevelEvent` | cov | 2 | 2 |
| `EntityEvent.EntityConstructing` | cov | 2 | 2 |
| `EntityStruckByLightningEvent` | cov | 2 | 2 |
| `EntityTeleportEvent.SpreadPlayersCommand` | cov | 2 | 2 |
| `ItemExpireEvent` | cov | 2 | 2 |
| `AnimalTameEvent` | cov | 2 | 2 |
| `LivingGetProjectileEvent` | cov | 2 | 2 |
| `ArrowLooseEvent` | cov | 2 | 2 |
| `PlayerEvent.NameFormat` | cov | 2 | 2 |
| `PlayerEvent.SaveToFile` | cov | 2 | 2 |
| `BlockEvent.FluidPlaceBlockEvent` | cov | 2 | 2 |
| `BlockGrowFeatureEvent` | cov | 2 | 2 |
| `ExplosionKnockbackEvent` | cov | 2 | 2 |
| `LevelEvent.PotentialSpawns` | cov | 2 | 2 |
| `SleepFinishedTimeEvent` | cov | 2 | 2 |
| `CropGrowEvent` | cov | 2 | 2 |
| `CropGrowEvent.Post` | cov | 2 | 2 |
| `FMLDedicatedServerSetupEvent` | cov | 2 | 2 |
| `ParallelDispatchEvent` | cov | 2 | 2 |
| `ExtendPoiTypesEvent` | cov | 2 | 2 |
| `ModifyRegistriesEvent` | gap | 2 | 2 |
| `RegisterGameTestsEvent` | gap | 1 | 3 |
| `StatAwardEvent` | cov | 1 | 1 |
| `EntityEvent.EnteringSection` | cov | 1 | 1 |
| `ItemEvent` | cov | 1 | 1 |
| `LivingUseTotemEvent` | cov | 1 | 1 |
| `MobSplitEvent` | cov | 1 | 1 |
| `AdvancementEvent.AdvancementProgressEvent` | cov | 1 | 1 |
| `ArrowNockEvent` | cov | 1 | 1 |
| `CanContinueSleepingEvent` | cov | 1 | 1 |
| `CanPlayerSleepEvent` | cov | 1 | 1 |
| `ItemFishedEvent` | cov | 1 | 1 |
| `PlayerEvent.LoadFromFile` | cov | 1 | 1 |
| `PlayerEvent.StopTracking` | cov | 1 | 1 |
| `PlayerWakeUpEvent` | cov | 1 | 1 |
| `PlayerXpEvent.XpChange` | cov | 1 | 1 |
| `TradeWithVillagerEvent` | cov | 1 | 1 |
| `BlockEvent.PortalSpawnEvent` | cov | 1 | 1 |
| `ModifyCustomSpawnersEvent` | gap | 1 | 1 |
| `PistonEvent` | cov | 1 | 1 |
| `PistonEvent.Pre` | cov | 1 | 1 |
| `CropGrowEvent.Pre` | cov | 1 | 1 |
| `RegisterCauldronFluidContentEvent` | gap | 1 | 1 |
| `GatherDataEvent` | gap | 0 | 138 |
| `AddAttributeTooltipsEvent` | gap | 0 | 6 |
| `GatherSkippedAttributeTooltipsEvent` | gap | 0 | 2 |
| `EffectParticleModificationEvent` | cov | 0 | 1 |

#### Inventory: Registries (150 rows, 42 unused, 2 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `DeferredHolder` | cov | 224 | 230 |
| `DeferredRegister#register` | cov | 203 | 207 |
| `DeferredRegister#create` | cov | 201 | 205 |
| `item` | cov | 132 | 255 |
| `data_component_type` | con | 113 | 155 |
| `creative_mode_tab` | con | 112 | 154 |
| `NeoForgeRegistries` | cov | 111 | 116 |
| `DeferredItem` | cov | 104 | 105 |
| `block` | cov | 98 | 221 |
| `block_entity_type` | cov | 90 | 143 |
| `DeferredRegister#createItems` | cov | 85 | 88 |
| `menu` | cov | 78 | 116 |
| `DeferredBlock` | cov | 78 | 82 |
| `NeoForgeRegistries.Keys` | cov | 77 | 83 |
| `DeferredRegister.Items#register` | cov | 76 | 80 |
| `recipe_serializer` | gap | 74 | 101 |
| `DeferredRegister#createBlocks` | cov | 66 | 66 |
| `DeferredRegister.Blocks#register` | cov | 60 | 61 |
| `sound_event` | con | 55 | 93 |
| `neoforge:attachment_types` | cov | 54 | 66 |
| `recipe_type` | gap | 49 | 74 |
| `RegistryBuilder` | gap | 47 | 47 |
| `entity_type` | cov | 46 | 126 |
| `DeferredRegister#createDataComponents` | con | 45 | 48 |
| `particle_type` | gap | 41 | 57 |
| `DeferredRegister#getEntries` | cov | 36 | 47 |
| `IRegistryExtension` | cov | 31 | 34 |
| `DeferredRegister.DataComponents#registerComponentType` | con | 23 | 30 |
| `neoforge:global_loot_modifier_serializers` | cov | 22 | 27 |
| `command_argument_type` | gap | 21 | 28 |
| `mob_effect` | con | 19 | 57 |
| `trigger_type` | gap | 19 | 29 |
| `RegisterEvent.RegisterHelper#register` | cov | 18 | 19 |
| `DeferredRegister.Items#registerItem` | cov | 17 | 17 |
| `fluid` | con | 16 | 81 |
| `neoforge:fluid_type` | con | 16 | 29 |
| `loot_function_type` | gap | 16 | 29 |
| `neoforge:condition_codecs` | gap | 16 | 22 |
| `DeferredRegister.Items#registerSimpleBlockItem` | cov | 16 | 16 |
| `attribute` | con | 13 | 41 |
| `DeferredRegister#makeRegistry` | gap | 12 | 12 |
| `DeferredRegister#addAlias` | cov | 11 | 11 |
| `neoforge:biome_modifier_serializers` | gap | 10 | 13 |
| `point_of_interest_type` | gap | 9 | 19 |
| `neoforge:entity_data_serializers` | gap | 9 | 14 |
| `DeferredRegister.Blocks#registerBlock` | cov | 9 | 9 |
| `worldgen/placement_modifier_type` | gap | 8 | 14 |
| `worldgen/structure_type` | gap | 8 | 13 |
| `loot_condition_type` | gap | 7 | 16 |
| `worldgen/structure_processor` | gap | 7 | 14 |
| `loot_pool_entry_type` | gap | 7 | 13 |
| `registries.callback` | gap | 7 | 7 |
| `villager_profession` | gap | 6 | 19 |
| `worldgen/structure_piece` | gap | 6 | 10 |
| `GameData` | gap | 6 | 6 |
| `potion` | gap | 5 | 29 |
| `custom_stat` | gap | 5 | 16 |
| `registries.holdersets` | gap | 5 | 9 |
| `worldgen/chunk_generator` | gap | 4 | 5 |
| `enchantment_effect_component_type` | gap | 3 | 10 |
| `worldgen/structure_placement` | gap | 3 | 7 |
| `DeferredRegister#getRegistry` | cov | 3 | 3 |
| `memory_module_type` | gap | 2 | 5 |
| `sensor_type` | gap | 2 | 5 |
| `height_provider_type` | gap | 2 | 5 |
| `worldgen/trunk_placer_type` | gap | 2 | 5 |
| `worldgen/structure_pool_element` | gap | 2 | 5 |
| `map_decoration_type` | gap | 2 | 5 |
| `DeferredRegister#getNamespace` | cov | 2 | 2 |
| `DeferredRegister.Blocks#registerSimpleBlock` | cov | 2 | 2 |
| `rule_test` | gap | 1 | 4 |
| `int_provider_type` | gap | 1 | 4 |
| `worldgen/foliage_placer_type` | gap | 1 | 4 |
| `worldgen/tree_decorator_type` | gap | 1 | 4 |
| `entity_sub_predicate_type` | gap | 1 | 4 |
| `DataPackRegistriesHooks` | gap | 1 | 3 |
| `neoforge:fluid_ingredient_type` | gap | 1 | 3 |
| `enchantment_entity_effect_type` | gap | 1 | 3 |
| `DeferredRegister#getRegistryKey` | cov | 1 | 2 |
| `RegistryManager` | gap | 1 | 2 |
| `neoforge:structure_modifier_serializers` | gap | 1 | 2 |
| `worldgen/block_state_provider_type` | gap | 1 | 2 |
| `worldgen/biome_source` | gap | 1 | 2 |
| `worldgen/density_function_type` | gap | 1 | 2 |
| `DeferredRegister.Items#registerSimpleItem` | cov | 1 | 1 |
| `RegistrySnapshot` | cov | 1 | 1 |
| `neoforge:holder_set_type` | gap | 1 | 1 |
| `stat_type` | gap | 0 | 7 |
| `villager_type` | gap | 0 | 7 |
| `game_event` | gap | 0 | 6 |
| `loot_nbt_provider_type` | gap | 0 | 4 |
| `pos_rule_test` | gap | 0 | 3 |
| `activity` | gap | 0 | 3 |
| `loot_score_provider_type` | gap | 0 | 3 |
| `float_provider_type` | gap | 0 | 3 |
| `block_predicate_type` | gap | 0 | 3 |
| `worldgen/root_placer_type` | gap | 0 | 3 |
| `worldgen/feature_size_type` | gap | 0 | 3 |
| `neoforge:biome_modifier` | gap | 0 | 2 |
| `chunk_status` | gap | 0 | 2 |
| `rule_block_entity_modifier` | gap | 0 | 2 |
| `position_source_type` | gap | 0 | 2 |
| `enchantment_level_based_value_type` | gap | 0 | 2 |
| `neoforge:structure_modifier` | gap | 0 | 1 |
| `number_format_type` | gap | 0 | 1 |
| `enchantment_location_based_effect_type` | gap | 0 | 1 |
| `enchantment_value_effect_type` | gap | 0 | 1 |
| `enchantment_provider_type` | gap | 0 | 1 |

#### Inventory: Capabilities (35 rows, 4 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `BlockCapability` | cov | 153 | 158 |
| `Capabilities` | cov | 138 | 141 |
| `ICapabilityProvider` | cov | 119 | 131 |
| `Capabilities.Item` | cov | 112 | 116 |
| `Capabilities.Item.BLOCK` | cov | 107 | 112 |
| `RegisterCapabilitiesEvent#registerBlockEntity` | cov | 95 | 103 |
| `ItemCapability` | cov | 88 | 89 |
| `Capabilities.Fluid` | cov | 78 | 80 |
| `Capabilities.Energy` | cov | 73 | 74 |
| `Capabilities.Fluid.BLOCK` | cov | 70 | 71 |
| `Capabilities.Energy.BLOCK` | cov | 60 | 61 |
| `RegisterCapabilitiesEvent#registerItem` | cov | 53 | 57 |
| `Capabilities.Energy.ITEM` | cov | 47 | 48 |
| `EntityCapability` | cov | 44 | 44 |
| `Capabilities.Fluid.ITEM` | cov | 42 | 43 |
| `IBlockCapabilityProvider` | cov | 41 | 44 |
| `RegisterCapabilitiesEvent#registerBlock` | cov | 37 | 42 |
| `BlockCapabilityCache` | cov | 35 | 35 |
| `Capabilities.Item.ITEM` | cov | 22 | 22 |
| `Capabilities.Item.ENTITY` | cov | 18 | 18 |
| `RegisterCapabilitiesEvent#registerEntity` | cov | 16 | 16 |
| `ICapabilityInvalidationListener` | cov | 8 | 8 |
| `Capabilities.Item.ENTITY_AUTOMATION` | cov | 7 | 7 |
| `RegisterCapabilitiesEvent#isBlockRegistered` | cov | 5 | 6 |
| `Capabilities.Energy.ENTITY` | cov | 5 | 5 |
| `Capabilities.Fluid.ENTITY` | cov | 4 | 4 |
| `BaseCapability` | cov | 4 | 4 |
| `RegisterCapabilitiesEvent#setProxyable` | cov | 3 | 3 |
| `CapabilityRegistry` | cov | 1 | 1 |
| `CapabilityRegistry.CapabilityConstructor` | cov | 1 | 1 |
| `RegisterCapabilitiesEvent#setNonProxyable` | cov | 1 | 1 |

#### Inventory: Attachments (26 rows, 5 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `Level` | cov | 310 | 324 |
| `Entity` | cov | 249 | 267 |
| `BlockEntity` | cov | 235 | 240 |
| `LevelChunk` | cov | 70 | 75 |
| `ChunkAccess` | cov | 65 | 70 |
| `IAttachmentHolder#getData` | cov | 57 | 57 |
| `AttachmentType.Builder#build` | cov | 55 | 59 |
| `AttachmentType#builder` | cov | 46 | 50 |
| `AttachmentType.Builder#serialize` | cov | 43 | 47 |
| `AttachmentType.Builder#copyOnDeath` | cov | 25 | 25 |
| `IAttachmentHolder#hasData` | cov | 16 | 17 |
| `AttachmentType#serializable` | cov | 15 | 15 |
| `AttachmentType.Builder#sync` | cov | 10 | 10 |
| `IAttachmentSerializer` | cov | 7 | 7 |
| `AttachmentType.Builder#copyHandler` | cov | 5 | 5 |
| `IAttachmentCopyHandler` | cov | 5 | 5 |
| `IAttachmentHolder#getExistingData` | cov | 5 | 5 |
| `AttachmentHolder` | cov | 3 | 3 |
| `IAttachmentHolder#getExistingDataOrNull` | cov | 3 | 3 |
| `IAttachmentHolder#syncData` | cov | 2 | 2 |
| `IAttachmentHolder#hasAttachments` | cov | 1 | 1 |

#### Inventory: Data maps (24 rows, 5 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `DataMapType` | cov | 27 | 42 |
| `DataMapType.Builder` | cov | 18 | 19 |
| `registries.datamaps.builtin` | cov | 5 | 17 |
| `WAXABLES` | cov | 5 | 6 |
| `OXIDIZABLES` | cov | 3 | 4 |
| `AdvancedDataMapType` | cov | 3 | 3 |
| `AdvancedDataMapType.Builder` | cov | 2 | 2 |
| `DataMapValueRemover` | cov | 2 | 2 |
| `PARROT_IMITATIONS` | cov | 1 | 2 |
| `RAID_HERO_GIFTS` | cov | 1 | 2 |
| `VIBRATION_FREQUENCIES` | cov | 1 | 2 |
| `DataMapEntry` | cov | 1 | 1 |
| `DataMapEntry.Removal` | cov | 1 | 1 |
| `DataMapFile` | cov | 1 | 1 |
| `DataMapValueMerger` | cov | 1 | 1 |
| `DataMapValueRemover.Default` | cov | 1 | 1 |
| `IWithData` | cov | 1 | 1 |
| `MONSTER_ROOM_MOBS` | cov | 1 | 1 |
| `VILLAGER_TYPES` | cov | 1 | 1 |

#### Inventory: Config (36 rows, 2 unused, 2 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `ModConfigSpec` | cov | 182 | 195 |
| `ModConfig` | cov | 165 | 191 |
| `ModConfig.Type` | cov | 165 | 191 |
| `IConfigSpec` | cov | 162 | 188 |
| `ModConfigSpec.Builder#comment` | cov | 156 | 167 |
| `ModConfigSpec.Builder#define` | cov | 140 | 155 |
| `ModConfigSpec.BooleanValue` | cov | 138 | 152 |
| `ModConfigSpec.Builder#defineInRange` | cov | 134 | 139 |
| `ModConfigSpec.Builder#push` | cov | 132 | 137 |
| `ModConfigSpec.Builder#pop` | cov | 130 | 136 |
| `ModConfigSpec.Builder#build` | cov | 123 | 132 |
| `ModConfigSpec.IntValue` | cov | 119 | 123 |
| `ModConfigSpec.ConfigValue` | cov | 104 | 112 |
| `ModConfig.Type.LOCAL` | cov | 102 | 110 |
| `ModConfig.Type.SYNCED` | cov | 75 | 80 |
| `ModConfigSpec.DoubleValue` | cov | 68 | 71 |
| `ModConfigSpec.Builder#configure` | cov | 57 | 61 |
| `ModConfigSpec.EnumValue` | cov | 39 | 40 |
| `ModConfigSpec.Builder#translation` | cov | 38 | 39 |
| `ModConfigSpec.Builder#defineEnum` | cov | 37 | 38 |
| `ModConfigSpec.Builder#defineList` | cov | 31 | 32 |
| `ModConfigSpec.Builder#defineListAllowEmpty` | cov | 31 | 31 |
| `ModConfigSpec.LongValue` | cov | 22 | 22 |
| `ModConfig.Type.STARTUP` | cov | 17 | 19 |
| `ModConfigSpec.Builder#worldRestart` | cov | 15 | 15 |
| `ModConfigSpec.ValueSpec` | cov | 7 | 10 |
| `IConfigSpec.ILoadedConfig` | cov | 5 | 7 |
| `ModConfigSpec.Range` | cov | 5 | 6 |
| `ModConfigSpec.Builder#gameRestart` | cov | 4 | 4 |
| `ConfigTracker` | gap | 4 | 4 |
| `ModConfigSpec.RestartType` | cov | 3 | 4 |
| `ModConfigs` | cov | 3 | 4 |
| `ModConfigSpec.Builder#defineInList` | cov | 1 | 2 |
| `ModConfigSpec.ListValueSpec` | cov | 0 | 1 |

#### Inventory: Networking (63 rows, 27 unused, 2 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `IPayloadHandler` | cov | 166 | 170 |
| `IPayloadContext#player` | cov | 154 | 157 |
| `IPayloadContext#enqueueWork` | cov | 151 | 174 |
| `PayloadRegistrar#playToClient` | cov | 125 | 127 |
| `PayloadRegistrar#playToServer` | cov | 123 | 124 |
| `PacketDistributor#sendToPlayer` | cov | 121 | 123 |
| `IContainerFactory` | cov | 84 | 93 |
| `PayloadRegistrar#versioned` | cov | 54 | 58 |
| `PacketDistributor#sendToPlayersTrackingChunk` | cov | 43 | 43 |
| `PayloadRegistrar#optional` | cov | 42 | 45 |
| `IPayloadContext#flow` | cov | 41 | 41 |
| `PacketDistributor#sendToAllPlayers` | cov | 39 | 39 |
| `network.codec` | gap | 34 | 34 |
| `PayloadRegistrar#playBidirectional` | cov | 32 | 34 |
| `PacketDistributor#sendToPlayersTrackingEntity` | cov | 27 | 27 |
| `PacketDistributor#sendToPlayersNear` | cov | 20 | 20 |
| `PacketDistributor#sendToPlayersTrackingEntityAndSelf` | cov | 19 | 19 |
| `IPayloadContext#disconnect` | cov | 17 | 18 |
| `HandlerThread` | cov | 12 | 12 |
| `PacketDistributor#sendToPlayersInDimension` | cov | 11 | 11 |
| `PayloadRegistrar#executesOn` | cov | 10 | 10 |
| `network.connection` | gap | 8 | 9 |
| `PayloadRegistrar#configurationToClient` | cov | 7 | 7 |
| `NetworkRegistry` | gap | 7 | 7 |
| `IPayloadContext#finishCurrentTask` | cov | 6 | 7 |
| `IPayloadContext#connection` | gap | 5 | 5 |
| `IPayloadContext#reply` | cov | 5 | 5 |
| `ICustomConfigurationTask` | cov | 5 | 5 |
| `PayloadRegistrar#commonBidirectional` | cov | 3 | 3 |
| `MainThreadPayloadHandler` | cov | 3 | 3 |
| `PayloadRegistrar#configurationToServer` | cov | 2 | 2 |
| `PayloadRegistrar#commonToServer` | cov | 2 | 2 |
| `IPayloadContext#protocol` | cov | 2 | 2 |
| `IPayloadContext#listener` | gap | 1 | 2 |
| `PayloadRegistrar#commonToClient` | cov | 1 | 1 |
| `network.bundle` | gap | 1 | 1 |

#### Inventory: Lifecycle, entry points and metadata (72 rows, 13 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `file modLoader` | gap | 414 | 414 |
| `file loaderVersion` | gap | 414 | 414 |
| `file license` | cov | 414 | 414 |
| `file mods` | cov | 414 | 414 |
| `[[mods]] modId` | cov | 414 | 414 |
| `[[mods]] version` | cov | 414 | 414 |
| `[[mods]] displayName` | cov | 414 | 414 |
| `[[mods]] description` | cov | 414 | 414 |
| `@Mod(value)` | cov | 411 | 411 |
| `[[dependencies.<modid>]] modId` | cov | 404 | 404 |
| `[[dependencies.<modid>]] versionRange` | cov | 404 | 404 |
| `[[dependencies.<modid>]] side` | cov | 398 | 398 |
| `[[mods]] authors` | cov | 392 | 392 |
| `[[dependencies.<modid>]] ordering` | cov | 388 | 388 |
| `[[dependencies.<modid>]] type` | cov | 361 | 361 |
| `[[mixins]] config` | gap | 291 | 291 |
| `file mixins` | gap | 278 | 278 |
| `IEventBus#addListener` | cov | 263 | 304 |
| `file issueTrackerURL` | cov | 236 | 236 |
| `ModList#get` | cov | 200 | 224 |
| `@EventBusSubscriber(modid)` | cov | 186 | 186 |
| `ModList#isLoaded` | cov | 175 | 199 |
| `ModContainer#registerConfig` | cov | 159 | 185 |
| `@EventBusSubscriber(value)` | cov | 151 | 151 |
| `IEventBus#register` | cov | 112 | 133 |
| `EventPriority` | cov | 99 | 122 |
| `IEventBus#post` | cov | 86 | 95 |
| `@Mod(dist)` | cov | 77 | 77 |
| `@SubscribeEvent(priority)` | cov | 76 | 76 |
| `ModList#getModContainerById` | cov | 69 | 78 |
| `ModContainer#getModInfo` | cov | 61 | 72 |
| `file accessTransformers` | gap | 55 | 55 |
| `[[accessTransformers]] file` | gap | 55 | 55 |
| `[[mods]] updateJSONURL` | gap | 38 | 38 |
| `ModContainer#getEventBus` | cov | 32 | 37 |
| `ModList#getMods` | cov | 31 | 35 |
| `ModList#getAllScanData` | gap | 16 | 19 |
| `ModList#getModFileById` | gap | 16 | 18 |
| `ModContainer#getModId` | cov | 15 | 20 |
| `IEventBus#start` | gap | 15 | 16 |
| `[[dependencies.<modid>]] reason` | cov | 14 | 14 |
| `[[mods]] enumExtensions` | gap | 13 | 13 |
| `IEventBus#unregister` | gap | 12 | 13 |
| `[[mods]] modUrl` | cov | 10 | 10 |
| `[[mods]] issueTrackerURL` | cov | 8 | 8 |
| `@SubscribeEvent(receiveCanceled)` | cov | 6 | 6 |
| `[[mods]] featureFlags` | gap | 4 | 4 |
| `ModList#getModFiles` | gap | 3 | 3 |
| `ModList#forEachModInOrder` | cov | 3 | 3 |
| `[[dependencies.<modid>]] referralUrl` | cov | 3 | 3 |
| `ModList#size` | cov | 2 | 3 |
| `ModList#forEachModFile` | gap | 2 | 2 |
| `ModList#forEachModContainer` | cov | 2 | 2 |
| `ModList#getSortedMods` | cov | 2 | 2 |
| `[[mixins]] requiredMods` | gap | 2 | 2 |
| `ModList#applyForEachModContainer` | cov | 1 | 2 |
| `ModContainer#getNamespace` | cov | 1 | 1 |
| `ModContainer#acceptEvent` | gap | 1 | 1 |
| `file showAsResourcePack` | gap | 1 | 1 |

#### Inventory: Loot modifiers, loot conditions and data conditions (25 rows, 6 unused, 6 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `ICondition` | gap | 42 | 79 |
| `ICondition.IContext` | gap | 32 | 34 |
| `IGlobalLootModifier` | cov | 29 | 32 |
| `LootModifier` | cov | 25 | 26 |
| `ConditionalOps` | gap | 12 | 14 |
| `WithConditions` | gap | 8 | 16 |
| `ModLoadedCondition` | gap | 7 | 34 |
| `LootTableIdCondition` | cov | 2 | 9 |
| `AddTableLootModifier` | cov | 2 | 4 |
| `CanItemPerformAbility` | gap | 2 | 4 |
| `AndCondition` | gap | 2 | 4 |
| `NotCondition` | gap | 1 | 20 |
| `TagEmptyCondition` | gap | 1 | 12 |
| `LootTableIdCondition.Builder` | cov | 1 | 8 |
| `OrCondition` | gap | 1 | 4 |
| `LootModifierManager` | cov | 1 | 1 |
| `ConditionContext` | gap | 1 | 1 |
| `GlobalLootModifierProvider` | gap | 0 | 14 |
| `WithConditions.Builder` | gap | 0 | 1 |

#### Inventory: Extension interfaces NeoForge adds to net.minecraft (58 rows, 21 unused, 14 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `IBlockStateExtension` | par | 274 | 286 |
| `ILevelExtension` | par | 127 | 135 |
| `IItemStackExtension` | par | 122 | 129 |
| `ICommandSourceStackExtension` | par | 104 | 109 |
| `ICommonPacketListener` | par | 97 | 101 |
| `IDataComponentHolderExtension` | par | 90 | 91 |
| `IEntityExtension` | par | 87 | 90 |
| `IPlayerExtension` | par | 85 | 88 |
| `IBlockEntityExtension` | par | 85 | 86 |
| `IMenuTypeExtension` | par | 70 | 79 |
| `IItemExtension` | par | 65 | 68 |
| `IHolderExtension` | par | 60 | 68 |
| `IFluidExtension` | par | 49 | 53 |
| `IFluidStateExtension` | par | 47 | 51 |
| `IBlockExtension` | par | 47 | 48 |
| `ILevelReaderExtension` | par | 37 | 39 |
| `IItemPropertiesExtensions` | par | 32 | 32 |
| `IPacketFlowExtension` | par | 31 | 31 |
| `IHolderLookupProviderExtension` | par | 27 | 29 |
| `IDataComponentMapBuilderExtensions` | par | 24 | 24 |
| `IServerCommonPacketListenerExtension` | par | 19 | 19 |
| `IFriendlyByteBufExtension` | par | 13 | 13 |
| `IBlockGetterExtension` | par | 9 | 16 |
| `IBucketPickupExtension` | par | 7 | 7 |
| `IServerConfigurationPacketListenerExtension` | par | 6 | 7 |
| `IDispensibleContainerItemExtension` | par | 6 | 6 |
| `IAttributeExtension` | gap | 5 | 6 |
| `IBaseRailBlockExtension` | par | 4 | 4 |
| `IOwnedSpawner` | gap | 3 | 3 |
| `ILivingEntityExtension` | par | 2 | 3 |
| `ITransformationExtension` | gap | 1 | 10 |
| `IMobEffectExtension` | par | 1 | 1 |
| `ITagBuilderExtension` | gap | 1 | 1 |
| `TooltipFlagExtension` | gap | 1 | 1 |
| `IRecipeOutputExtension` | gap | 0 | 47 |
| `ITagAppenderExtension` | gap | 0 | 17 |
| `IAdvancementBuilderExtension` | gap | 0 | 3 |

#### Inventory: Other packages (87 rows, 35 unused, 4 of them n/a in 21.1)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `NeoForge` | cov | 219 | 248 |
| `transfer` | cov | 189 | 192 |
| `ModConfigSpec` | cov | 182 | 195 |
| `transfer.item` | cov | 149 | 150 |
| `common.util` | gap | 133 | 141 |
| `transfer.fluid` | cov | 130 | 132 |
| `fluids` | gap | 122 | 124 |
| `transfer.energy` | cov | 78 | 79 |
| `Tags` | cov | 77 | 130 |
| `server` | gap | 59 | 60 |
| `Tags.Items` | cov | 51 | 111 |
| `ItemAbility` | gap | 45 | 47 |
| `common.crafting` | gap | 43 | 54 |
| `CommonHooks` | gap | 42 | 43 |
| `ItemAbilities` | gap | 41 | 43 |
| `Tags.Blocks` | cov | 30 | 56 |
| `NeoForgeMod` | gap | 30 | 35 |
| `fluids.crafting` | gap | 28 | 30 |
| `SoundAction` | gap | 23 | 23 |
| `common.world` | gap | 22 | 34 |
| `Tags.EntityTypes` | cov | 20 | 23 |
| `entity` | cov | 20 | 21 |
| `MutableDataComponentHolder` | cov | 20 | 20 |
| `SoundActions` | gap | 20 | 20 |
| `common.damagesource` | cov | 18 | 18 |
| `common.world.chunk` | gap | 13 | 13 |
| `Tags.Fluids` | cov | 12 | 15 |
| `Tags.DamageTypes` | cov | 11 | 17 |
| `IShearable` | cov | 11 | 11 |
| `UsernameCache` | gap | 10 | 10 |
| `server.command` | gap | 9 | 11 |
| `resource` | cov | 8 | 9 |
| `SpecialPlantable` | gap | 8 | 8 |
| `server.permission` | cov | 7 | 7 |
| `server.permission.nodes` | cov | 7 | 7 |
| `PercentageAttribute` | gap | 7 | 7 |
| `Tags.Biomes` | cov | 4 | 15 |
| `TranslatableEnum` | gap | 3 | 4 |
| `BooleanAttribute` | gap | 3 | 3 |
| `CreativeModeTabRegistry` | gap | 3 | 3 |
| `common.world.poi` | cov | 2 | 2 |
| `FarmlandWaterManager` | gap | 2 | 2 |
| `common.enums` | gap | 1 | 1 |
| `common.ticket` | gap | 1 | 1 |
| `internal` | gap | 1 | 1 |
| `IOUtilities` | gap | 1 | 1 |
| `LenientUnboundedMapCodec` | gap | 1 | 1 |
| `data.event` | gap | 0 | 138 |
| `common.data` | gap | 0 | 127 |
| `data.loading` | gap | 0 | 19 |
| `gametest` | gap | 0 | 3 |
| `DataMapHooks` | gap | 0 | 2 |

#### Single-mod scan: NeoForge registries (9 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.registries.DeferredHolder` | cov | 224 | 230 |
| `neoforge.registries.DeferredRegister` | cov | 207 | 211 |
| `neoforge.registries.NeoForgeRegistries` | cov | 111 | 116 |
| `neoforge.registries.DeferredItem` | cov | 104 | 105 |
| `neoforge.registries.DeferredRegister$Items` | cov | 86 | 90 |
| `neoforge.registries.DeferredBlock` | cov | 78 | 82 |
| `neoforge.registries.NeoForgeRegistries$Keys` | cov | 77 | 83 |
| `neoforge.registries.DeferredRegister$Blocks` | cov | 67 | 67 |
| `neoforge.registries.DeferredRegister$DataComponents` | con | 47 | 50 |

#### Single-mod scan: NeoForge events and event bus (50 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `bus.api.IEventBus` | cov | 342 | 376 |
| `bus.api.SubscribeEvent` | cov | 207 | 271 |
| `neoforge.network.event.RegisterPayloadHandlersEvent` | cov | 170 | 176 |
| `fml.event.lifecycle.FMLCommonSetupEvent` | cov | 148 | 168 |
| `fml.common.EventBusSubscriber` | cov | 126 | 209 |
| `neoforge.event.entity.player.PlayerEvent` | cov | 96 | 100 |
| `neoforge.event.RegisterCommandsEvent` | cov | 93 | 98 |
| `neoforge.event.entity.player.PlayerInteractEvent` | cov | 85 | 89 |
| `neoforge.event.BuildCreativeModeTabContentsEvent` | gap | 75 | 89 |
| `fml.common.EventBusSubscriber$Bus` | cov | 69 | 128 |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedInEvent` | cov | 66 | 72 |
| `neoforge.event.level.BlockEvent` | cov | 65 | 66 |
| `neoforge.event.EventHooks` | cov | 51 | 52 |
| `neoforge.event.entity.EntityJoinLevelEvent` | cov | 49 | 53 |
| `neoforge.event.server.ServerStartedEvent` | cov | 40 | 43 |
| `neoforge.event.entity.living.LivingDamageEvent` | cov | 38 | 39 |
| `neoforge.event.entity.living.LivingIncomingDamageEvent` | cov | 37 | 38 |
| `neoforge.event.tick.PlayerTickEvent` | cov | 36 | 42 |
| `neoforge.event.entity.living.LivingDropsEvent` | cov | 31 | 31 |
| `neoforge.event.entity.EntityAttributeCreationEvent` | cov | 29 | 32 |
| `neoforge.event.entity.player.PlayerEvent$PlayerLoggedOutEvent` | cov | 28 | 29 |
| `neoforge.event.entity.player.PlayerInteractEvent$LeftClickBlock` | cov | 27 | 31 |
| `neoforge.event.entity.player.PlayerEvent$PlayerChangedDimensionEvent` | cov | 26 | 27 |
| `neoforge.event.entity.living.LivingDamageEvent$Post` | cov | 25 | 26 |
| `neoforge.event.tick.PlayerTickEvent$Post` | cov | 24 | 30 |
| `neoforge.event.entity.player.PlayerEvent$PlayerRespawnEvent` | cov | 24 | 25 |
| `neoforge.event.entity.player.PlayerEvent$Clone` | cov | 23 | 24 |
| `neoforge.event.level.BlockEvent$EntityPlaceEvent` | cov | 23 | 23 |
| `neoforge.event.entity.player.PlayerEvent$BreakSpeed` | cov | 18 | 18 |
| `neoforge.event.level.BlockDropsEvent` | cov | 17 | 17 |
| `neoforge.event.entity.EntityAttributeModificationEvent` | cov | 16 | 18 |
| `neoforge.event.entity.living.FinalizeSpawnEvent` | cov | 16 | 16 |
| `neoforge.event.entity.living.LivingEntityUseItemEvent` | cov | 16 | 16 |
| `neoforge.event.entity.player.AttackEntityEvent` | cov | 15 | 16 |
| `neoforge.event.AddPackFindersEvent` | cov | 13 | 21 |
| `neoforge.event.entity.living.MobEffectEvent` | cov | 13 | 14 |
| `neoforge.event.entity.living.LivingExperienceDropEvent` | cov | 13 | 13 |
| `neoforge.event.entity.player.PlayerEvent$StartTracking` | cov | 13 | 13 |
| `neoforge.event.entity.ProjectileImpactEvent` | cov | 12 | 12 |
| `neoforge.event.village.VillagerTradesEvent` | gap | 12 | 12 |
| `neoforge.event.AnvilUpdateEvent` | cov | 9 | 11 |
| `neoforge.event.entity.living.LivingEntityUseItemEvent$Start` | cov | 9 | 9 |
| `neoforge.event.entity.living.LivingKnockBackEvent` | cov | 9 | 9 |
| `neoforge.event.village.WandererTradesEvent` | gap | 9 | 9 |
| `neoforge.event.entity.living.LivingShieldBlockEvent` | cov | 8 | 8 |
| `neoforge.event.entity.living.MobEffectEvent$Applicable` | cov | 7 | 8 |
| `neoforge.event.entity.living.MobEffectEvent$Applicable$Result` | cov | 7 | 8 |
| `neoforge.network.event.RegisterConfigurationTasksEvent` | cov | 7 | 7 |
| `neoforge.event.entity.EntityTravelToDimensionEvent` | cov | 6 | 7 |
| `neoforge.event.entity.player.CriticalHitEvent` | cov | 5 | 5 |

#### Single-mod scan: NeoForge mod loading and config (10 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `fml.common.Mod` | cov | 340 | 411 |
| `fml.ModContainer` | cov | 246 | 285 |
| `api.distmarker.Dist` | cov | 199 | 326 |
| `neoforge.common.ModConfigSpec` | cov | 182 | 195 |
| `neoforge.common.ModConfigSpec$Builder` | cov | 178 | 191 |
| `fml.config.ModConfig` | cov | 165 | 191 |
| `fml.config.ModConfig$Type` | cov | 165 | 191 |
| `fml.config.IConfigSpec` | cov | 162 | 188 |
| `neoforge.common.ModConfigSpec$ConfigValue` | cov | 104 | 112 |
| `fml.loading.FMLLoader` | cov | 64 | 73 |

#### Single-mod scan: NeoForge attachments (2 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.attachment.AttachmentType` | cov | 68 | 68 |
| `neoforge.attachment.AttachmentType$Builder` | cov | 57 | 61 |

#### Single-mod scan: NeoForge networking (6 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.network.handling.IPayloadContext` | cov | 183 | 184 |
| `neoforge.network.registration.PayloadRegistrar` | cov | 167 | 171 |
| `neoforge.network.handling.IPayloadHandler` | cov | 166 | 170 |
| `neoforge.network.PacketDistributor` | cov | 157 | 163 |
| `neoforge.network.IContainerFactory` | cov | 84 | 93 |
| `neoforge.network.configuration.ICustomConfigurationTask` | cov | 5 | 5 |

#### Single-mod scan: NeoForge fluids (6 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.fluids.FluidType` | cov | 75 | 78 |
| `neoforge.fluids.FluidType$Properties` | cov | 32 | 33 |
| `neoforge.fluids.BaseFlowingFluid` | cov | 30 | 30 |
| `neoforge.fluids.BaseFlowingFluid$Properties` | cov | 26 | 27 |
| `neoforge.fluids.BaseFlowingFluid$Flowing` | cov | 23 | 23 |
| `neoforge.fluids.BaseFlowingFluid$Source` | cov | 21 | 22 |

#### Single-mod scan: NeoForge common utilities (14 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.common.Tags` | cov | 77 | 130 |
| `neoforge.common.extensions.IMenuTypeExtension` | cov | 70 | 79 |
| `neoforge.common.util.FakePlayer` | gap | 63 | 64 |
| `neoforge.common.Tags$Items` | cov | 51 | 111 |
| `neoforge.common.ItemAbility` | gap | 45 | 47 |
| `neoforge.common.ItemAbilities` | gap | 41 | 43 |
| `neoforge.common.Tags$Blocks` | cov | 30 | 56 |
| `neoforge.common.SoundAction` | cov | 23 | 23 |
| `neoforge.common.SoundActions` | cov | 20 | 20 |
| `neoforge.common.DeferredSpawnEggItem` | cov | 19 | 19 |
| `neoforge.common.damagesource.DamageContainer` | cov | 17 | 17 |
| `neoforge.common.SimpleTier` | cov | 5 | 5 |
| `neoforge.common.damagesource.DamageContainer$Reduction` | cov | 2 | 2 |
| `neoforge.common.damagesource.IReductionFunction` | cov | 1 | 1 |

#### Single-mod scan: NeoForge loot and world modifiers (6 rows, 1 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `neoforge.common.loot.IGlobalLootModifier` | cov | 29 | 32 |
| `neoforge.common.loot.LootModifier` | cov | 25 | 26 |
| `neoforge.common.world.BiomeModifier` | gap | 14 | 25 |
| `neoforge.common.world.BiomeModifiers` | gap | 1 | 12 |
| `neoforge.common.world.BiomeModifiers$AddFeaturesBiomeModifier` | gap | 0 | 10 |

#### Single-mod scan: Minecraft core, registries and tags (29 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `resources.ResourceLocation` | cov | 367 | 386 |
| `resources.ResourceKey` | cov | 306 | 324 |
| `core.BlockPos` | cov | 296 | 303 |
| `core.registries.Registries` | cov | 265 | 280 |
| `core.Holder` | cov | 258 | 276 |
| `core.registries.BuiltInRegistries` | cov | 257 | 274 |
| `core.Direction` | cov | 249 | 258 |
| `core.HolderLookup` | cov | 245 | 270 |
| `core.HolderLookup$Provider` | cov | 239 | 266 |
| `core.DefaultedRegistry` | cov | 239 | 261 |
| `core.Registry` | cov | 239 | 249 |
| `tags.TagKey` | cov | 232 | 257 |
| `core.RegistryAccess` | cov | 198 | 209 |
| `core.Holder$Reference` | cov | 177 | 203 |
| `core.NonNullList` | cov | 172 | 178 |
| `core.particles.ParticleOptions` | cov | 133 | 137 |
| `core.particles.SimpleParticleType` | cov | 114 | 117 |
| `core.particles.ParticleTypes` | cov | 113 | 115 |
| `tags.BlockTags` | cov | 96 | 133 |
| `core.HolderSet` | cov | 94 | 106 |
| `core.HolderLookup$RegistryLookup` | cov | 81 | 92 |
| `core.Position` | cov | 75 | 90 |
| `tags.ItemTags` | cov | 74 | 110 |
| `core.BlockPos$MutableBlockPos` | cov | 67 | 77 |
| `core.HolderSet$Named` | cov | 63 | 78 |
| `core.HolderGetter` | cov | 46 | 60 |
| `tags.DamageTypeTags` | cov | 34 | 43 |
| `core.HolderSet$Direct` | cov | 32 | 45 |
| `tags.BiomeTags` | cov | 9 | 27 |

#### Single-mod scan: Minecraft data components (5 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `core.component.DataComponentType` | con | 232 | 243 |
| `core.component.DataComponents` | cov | 153 | 165 |
| `core.component.DataComponentType$Builder` | cov | 148 | 157 |
| `core.component.DataComponentMap` | cov | 103 | 109 |
| `core.component.TypedDataComponent` | cov | 27 | 29 |

#### Single-mod scan: Minecraft utilities (10 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `ChatFormatting` | gap | 248 | 272 |
| `util.RandomSource` | gap | 200 | 221 |
| `util.Mth` | gap | 191 | 224 |
| `util.StringRepresentable` | gap | 127 | 131 |
| `util.StringRepresentable$EnumCodec` | gap | 83 | 86 |
| `util.valueproviders.UniformInt` | cov | 16 | 18 |
| `util.valueproviders.IntProvider` | cov | 15 | 22 |
| `util.valueproviders.ConstantInt` | cov | 6 | 12 |
| `util.random.SimpleWeightedRandomList` | gap | 6 | 9 |
| `util.random.SimpleWeightedRandomList$Builder` | gap | 3 | 6 |

#### Single-mod scan: Minecraft sounds (3 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `sounds.SoundEvent` | cov | 190 | 198 |
| `sounds.SoundSource` | cov | 165 | 169 |
| `sounds.SoundEvents` | cov | 162 | 177 |

#### Single-mod scan: Minecraft items (36 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.item.ItemStack` | cov | 327 | 341 |
| `world.item.Item` | cov | 312 | 326 |
| `world.item.Item$Properties` | cov | 217 | 217 |
| `world.item.CreativeModeTab` | cov | 183 | 194 |
| `world.item.Items` | cov | 180 | 223 |
| `world.item.BlockItem` | cov | 179 | 188 |
| `world.item.CreativeModeTab$Builder` | cov | 142 | 147 |
| `world.item.CreativeModeTab$Output` | cov | 131 | 135 |
| `world.item.CreativeModeTab$ItemDisplayParameters` | cov | 127 | 132 |
| `world.item.CreativeModeTab$DisplayItemsGenerator` | cov | 121 | 125 |
| `world.item.DyeColor` | cov | 82 | 91 |
| `world.item.CreativeModeTabs` | cov | 73 | 86 |
| `world.item.Rarity` | cov | 70 | 76 |
| `world.item.component.ItemAttributeModifiers` | cov | 57 | 59 |
| `world.item.ArmorItem` | cov | 46 | 50 |
| `world.food.FoodProperties` | cov | 45 | 48 |
| `world.item.ItemCooldowns` | cov | 44 | 48 |
| `world.item.Tier` | cov | 44 | 45 |
| `world.item.BucketItem` | cov | 38 | 41 |
| `world.item.ArmorMaterial` | cov | 37 | 39 |
| `world.item.alchemy.PotionContents` | cov | 36 | 45 |
| `world.item.SwordItem` | cov | 35 | 36 |
| `world.item.alchemy.Potion` | cov | 34 | 37 |
| `world.item.UseAnim` | cov | 34 | 35 |
| `world.item.ArmorItem$Type` | cov | 31 | 33 |
| `world.item.alchemy.Potions` | cov | 27 | 35 |
| `world.item.AxeItem` | cov | 27 | 28 |
| `world.item.CreativeModeTab$TabVisibility` | gap | 26 | 28 |
| `world.item.ArmorMaterial$Layer` | gap | 24 | 24 |
| `world.item.PickaxeItem` | cov | 21 | 24 |
| `world.food.FoodProperties$Builder` | cov | 21 | 22 |
| `world.item.ShovelItem` | cov | 21 | 22 |
| `world.item.HoeItem` | cov | 17 | 19 |
| `world.item.PotionItem` | cov | 10 | 12 |
| `world.item.ItemNameBlockItem` | cov | 10 | 10 |
| `world.item.component.SeededContainerLoot` | cov | 4 | 4 |

#### Single-mod scan: Minecraft enchantments (6 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.item.enchantment.Enchantment` | cov | 68 | 75 |
| `world.item.enchantment.EnchantmentHelper` | cov | 65 | 65 |
| `world.item.enchantment.Enchantments` | cov | 53 | 64 |
| `world.item.enchantment.ItemEnchantments` | cov | 50 | 54 |
| `world.item.enchantment.ItemEnchantments$Mutable` | cov | 32 | 33 |
| `world.item.enchantment.EnchantmentInstance` | cov | 15 | 18 |

#### Single-mod scan: Minecraft recipes and trading (5 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.item.crafting.Ingredient` | gap | 141 | 159 |
| `world.item.trading.MerchantOffer` | cov | 19 | 20 |
| `world.item.trading.ItemCost` | gap | 17 | 17 |
| `world.item.trading.MerchantOffers` | cov | 10 | 11 |
| `world.item.trading.Merchant` | gap | 6 | 6 |

#### Single-mod scan: Minecraft blocks and block entities (27 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.level.block.state.BlockState` | cov | 280 | 285 |
| `world.level.block.Block` | cov | 275 | 285 |
| `world.level.block.entity.BlockEntity` | cov | 235 | 240 |
| `world.level.block.state.properties.Property` | cov | 197 | 201 |
| `world.level.block.entity.BlockEntityType` | cov | 191 | 195 |
| `world.level.block.state.BlockBehaviour` | cov | 186 | 190 |
| `world.level.block.Blocks` | cov | 183 | 199 |
| `world.level.block.state.BlockBehaviour$Properties` | cov | 178 | 181 |
| `world.level.block.entity.BlockEntityType$BlockEntitySupplier` | cov | 144 | 145 |
| `world.level.block.SoundType` | cov | 127 | 128 |
| `world.level.block.entity.BlockEntityTicker` | cov | 114 | 114 |
| `world.level.block.entity.BlockEntityType$Builder` | cov | 112 | 115 |
| `world.level.block.EntityBlock` | cov | 102 | 104 |
| `world.level.block.state.properties.IntegerProperty` | cov | 90 | 91 |
| `world.level.block.RenderShape` | gap | 81 | 97 |
| `world.level.block.LiquidBlock` | cov | 49 | 49 |
| `world.level.block.SlabBlock` | cov | 29 | 33 |
| `world.level.block.StairBlock` | cov | 27 | 32 |
| `world.level.block.CropBlock` | cov | 26 | 26 |
| `world.level.block.BaseFireBlock` | cov | 24 | 24 |
| `world.level.block.RotatedPillarBlock` | cov | 23 | 28 |
| `world.level.block.DoorBlock` | cov | 17 | 20 |
| `world.level.block.entity.SpawnerBlockEntity` | cov | 14 | 14 |
| `world.level.block.FireBlock` | gap | 13 | 14 |
| `world.level.block.entity.RandomizableContainerBlockEntity` | cov | 13 | 13 |
| `world.level.block.SnowLayerBlock` | cov | 8 | 8 |
| `world.level.block.DropExperienceBlock` | cov | 4 | 5 |

#### Single-mod scan: Minecraft entities (44 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.entity.player.Player` | cov | 302 | 311 |
| `world.entity.Entity` | cov | 249 | 267 |
| `world.entity.LivingEntity` | cov | 219 | 239 |
| `world.entity.player.Inventory` | cov | 207 | 213 |
| `world.entity.EntityType` | cov | 174 | 182 |
| `world.entity.item.ItemEntity` | cov | 160 | 160 |
| `world.entity.EquipmentSlot` | cov | 113 | 120 |
| `world.entity.Mob` | cov | 82 | 82 |
| `world.entity.player.Abilities` | cov | 81 | 84 |
| `world.entity.MobCategory` | cov | 80 | 83 |
| `world.entity.EntityType$EntityFactory` | cov | 65 | 66 |
| `world.entity.EntityType$Builder` | cov | 64 | 64 |
| `world.entity.MobSpawnType` | cov | 46 | 46 |
| `world.entity.projectile.Projectile` | cov | 44 | 44 |
| `world.entity.EntityDimensions` | cov | 37 | 38 |
| `world.entity.PathfinderMob` | cov | 37 | 37 |
| `world.entity.ai.goal.GoalSelector` | cov | 35 | 35 |
| `world.entity.ai.goal.Goal` | cov | 34 | 34 |
| `world.entity.projectile.AbstractArrow` | cov | 27 | 28 |
| `world.entity.SpawnGroupData` | gap | 22 | 22 |
| `world.entity.ai.goal.target.NearestAttackableTargetGoal` | cov | 19 | 19 |
| `world.entity.monster.Creeper` | cov | 18 | 19 |
| `world.entity.animal.horse.AbstractHorse` | cov | 18 | 18 |
| `world.entity.ai.goal.LookAtPlayerGoal` | cov | 17 | 17 |
| `world.entity.animal.Wolf` | cov | 17 | 17 |
| `world.entity.ai.goal.target.HurtByTargetGoal` | cov | 16 | 16 |
| `world.entity.ai.goal.FloatGoal` | cov | 16 | 16 |
| `world.entity.ai.goal.RandomLookAroundGoal` | cov | 16 | 16 |
| `world.entity.monster.Zombie` | cov | 14 | 17 |
| `world.entity.ai.goal.WrappedGoal` | cov | 13 | 13 |
| `world.entity.projectile.Arrow` | cov | 11 | 11 |
| `world.entity.monster.Skeleton` | cov | 9 | 11 |
| `world.entity.ai.goal.AvoidEntityGoal` | cov | 9 | 9 |
| `world.entity.monster.Spider` | cov | 8 | 8 |
| `world.entity.animal.IronGolem` | cov | 8 | 8 |
| `world.entity.AreaEffectCloud` | cov | 7 | 8 |
| `world.entity.monster.Blaze` | cov | 7 | 7 |
| `world.entity.monster.AbstractSkeleton` | cov | 6 | 7 |
| `world.entity.monster.ZombifiedPiglin` | cov | 3 | 3 |
| `world.entity.ai.goal.MoveToBlockGoal` | cov | 3 | 3 |
| `world.entity.animal.Rabbit` | cov | 3 | 3 |
| `world.entity.monster.CaveSpider` | cov | 2 | 2 |
| `world.entity.ai.goal.FleeSunGoal` | cov | 2 | 2 |
| `world.entity.ai.goal.RestrictSunGoal` | cov | 2 | 2 |

#### Single-mod scan: Minecraft attributes (9 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.entity.ai.attributes.Attributes` | cov | 79 | 84 |
| `world.entity.ai.attributes.AttributeModifier` | cov | 73 | 76 |
| `world.entity.ai.attributes.AttributeModifier$Operation` | cov | 65 | 68 |
| `world.entity.ai.attributes.AttributeInstance` | cov | 62 | 63 |
| `world.entity.ai.attributes.Attribute` | cov | 56 | 58 |
| `world.entity.ai.attributes.AttributeSupplier` | cov | 38 | 39 |
| `world.entity.ai.attributes.AttributeSupplier$Builder` | cov | 38 | 38 |
| `world.entity.ai.attributes.AttributeMap` | cov | 31 | 31 |
| `world.entity.ai.attributes.RangedAttribute` | cov | 23 | 24 |

#### Single-mod scan: Minecraft effects and damage (9 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.damagesource.DamageSource` | cov | 113 | 114 |
| `world.effect.MobEffectInstance` | cov | 94 | 97 |
| `world.effect.MobEffect` | cov | 72 | 79 |
| `world.effect.MobEffects` | cov | 67 | 77 |
| `world.damagesource.DamageSources` | cov | 62 | 62 |
| `world.damagesource.DamageType` | cov | 43 | 49 |
| `world.effect.MobEffectCategory` | cov | 39 | 42 |
| `world.damagesource.DamageTypes` | cov | 33 | 36 |
| `world.damagesource.DamageScaling` | gap | 0 | 6 |

#### Single-mod scan: Minecraft menus and containers (11 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.inventory.AbstractContainerMenu` | gap | 172 | 179 |
| `world.inventory.MenuType` | cov | 157 | 159 |
| `world.inventory.Slot` | cov | 139 | 146 |
| `world.Container` | cov | 135 | 138 |
| `world.MenuProvider` | gap | 121 | 122 |
| `world.inventory.InventoryMenu` | cov | 68 | 101 |
| `world.SimpleContainer` | cov | 52 | 52 |
| `world.inventory.ContainerLevelAccess` | cov | 52 | 52 |
| `world.inventory.MenuConstructor` | gap | 51 | 52 |
| `world.SimpleMenuProvider` | gap | 45 | 46 |
| `world.inventory.EnchantmentMenu` | cov | 2 | 2 |

#### Single-mod scan: Minecraft level and world (28 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.level.Level` | cov | 310 | 324 |
| `world.level.ItemLike` | cov | 262 | 280 |
| `world.phys.Vec3` | cov | 219 | 238 |
| `world.InteractionHand` | cov | 214 | 216 |
| `world.level.BlockGetter` | cov | 198 | 207 |
| `world.InteractionResult` | cov | 186 | 188 |
| `world.phys.shapes.VoxelShape` | gap | 167 | 173 |
| `world.level.LevelAccessor` | cov | 162 | 166 |
| `world.phys.AABB` | cov | 161 | 186 |
| `world.phys.shapes.CollisionContext` | gap | 134 | 139 |
| `world.InteractionResultHolder` | cov | 131 | 132 |
| `world.phys.HitResult` | cov | 127 | 143 |
| `world.level.ChunkPos` | cov | 111 | 114 |
| `world.phys.HitResult$Type` | cov | 70 | 91 |
| `world.level.chunk.LevelChunk` | cov | 70 | 75 |
| `world.level.Explosion` | cov | 64 | 65 |
| `world.phys.EntityHitResult` | cov | 58 | 61 |
| `world.level.ServerLevelAccessor` | cov | 54 | 54 |
| `world.level.GameRules` | gap | 53 | 55 |
| `world.level.GameRules$Key` | cov | 52 | 53 |
| `world.level.WorldGenLevel` | cov | 30 | 31 |
| `world.DifficultyInstance` | cov | 26 | 27 |
| `world.level.Level$ExplosionInteraction` | cov | 24 | 25 |
| `world.level.GameRules$BooleanValue` | cov | 16 | 16 |
| `world.level.BaseSpawner` | cov | 15 | 15 |
| `world.level.GameRules$Type` | gap | 10 | 10 |
| `world.level.SpawnData` | cov | 8 | 8 |
| `world.level.GameRules$Category` | gap | 6 | 6 |

#### Single-mod scan: Minecraft fluids and materials (4 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.level.material.Fluid` | cov | 155 | 165 |
| `world.level.material.MapColor` | cov | 107 | 112 |
| `world.level.material.FlowingFluid` | cov | 103 | 108 |
| `world.level.material.PushReaction` | cov | 55 | 57 |

#### Single-mod scan: Minecraft world generation (28 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.level.levelgen.Heightmap` | gap | 58 | 61 |
| `world.level.levelgen.Heightmap$Types` | gap | 58 | 61 |
| `world.level.levelgen.feature.Feature` | gap | 24 | 28 |
| `world.level.levelgen.GenerationStep` | gap | 21 | 30 |
| `world.level.levelgen.placement.PlacedFeature` | gap | 20 | 29 |
| `world.level.levelgen.feature.configurations.FeatureConfiguration` | gap | 18 | 25 |
| `world.level.levelgen.feature.FeaturePlaceContext` | gap | 18 | 18 |
| `world.level.levelgen.GenerationStep$Decoration` | gap | 16 | 27 |
| `world.level.levelgen.placement.PlacementModifierType` | gap | 14 | 14 |
| `world.level.levelgen.feature.ConfiguredFeature` | gap | 13 | 25 |
| `world.level.levelgen.structure.templatesystem.RuleTest` | gap | 10 | 19 |
| `world.level.levelgen.placement.PlacementContext` | gap | 10 | 10 |
| `world.level.levelgen.feature.configurations.OreConfiguration` | gap | 8 | 16 |
| `world.level.levelgen.placement.PlacementFilter` | gap | 8 | 8 |
| `world.level.levelgen.placement.PlacementModifier` | gap | 7 | 16 |
| `world.level.levelgen.VerticalAnchor` | gap | 6 | 15 |
| `world.level.levelgen.feature.configurations.OreConfiguration$TargetBlockState` | gap | 6 | 11 |
| `world.level.levelgen.feature.stateproviders.BlockStateProvider` | gap | 5 | 11 |
| `world.level.levelgen.placement.InSquarePlacement` | gap | 3 | 14 |
| `world.level.levelgen.placement.BiomeFilter` | gap | 3 | 13 |
| `world.level.levelgen.structure.templatesystem.TagMatchTest` | gap | 3 | 13 |
| `world.level.levelgen.placement.CountPlacement` | gap | 3 | 10 |
| `world.level.levelgen.placement.RarityFilter` | gap | 2 | 12 |
| `world.level.levelgen.structure.templatesystem.BlockMatchTest` | gap | 2 | 3 |
| `world.level.levelgen.placement.HeightRangePlacement` | gap | 1 | 12 |
| `world.level.levelgen.feature.stateproviders.WeightedStateProvider` | gap | 1 | 4 |
| `world.level.levelgen.feature.configurations.SimpleBlockConfiguration` | gap | 0 | 6 |
| `world.level.levelgen.placement.HeightmapPlacement` | gap | 0 | 1 |

#### Single-mod scan: Minecraft loot (4 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `world.level.storage.loot.LootContext` | cov | 59 | 63 |
| `world.level.storage.loot.LootTable` | cov | 58 | 106 |
| `world.level.storage.loot.predicates.LootItemCondition` | cov | 51 | 69 |
| `world.level.storage.loot.BuiltInLootTables` | cov | 22 | 28 |

#### Single-mod scan: Minecraft server and commands (17 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `server.level.ServerPlayer` | cov | 263 | 265 |
| `server.level.ServerLevel` | cov | 252 | 254 |
| `commands.CommandSourceStack` | cov | 127 | 130 |
| `commands.Commands` | cov | 123 | 126 |
| `stats.Stats` | cov | 71 | 76 |
| `stats.StatType` | cov | 66 | 72 |
| `stats.Stat` | cov | 63 | 67 |
| `commands.arguments.EntityArgument` | cov | 51 | 52 |
| `commands.CommandBuildContext` | cov | 47 | 49 |
| `server.packs.PackType` | cov | 31 | 51 |
| `server.packs.repository.Pack` | cov | 21 | 28 |
| `server.packs.repository.PackSource` | cov | 17 | 23 |
| `server.packs.repository.Pack$Position` | cov | 14 | 20 |
| `stats.ServerStatsCounter` | cov | 12 | 12 |
| `commands.arguments.ResourceArgument` | cov | 9 | 9 |
| `server.network.ConfigurationTask` | cov | 7 | 7 |
| `server.network.ConfigurationTask$Type` | cov | 7 | 7 |

#### Single-mod scan: Minecraft network and chat (8 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `network.chat.Component` | cov | 335 | 363 |
| `network.chat.MutableComponent` | cov | 325 | 351 |
| `network.codec.StreamCodec` | gap | 252 | 255 |
| `network.RegistryFriendlyByteBuf` | gap | 226 | 229 |
| `network.protocol.common.custom.CustomPacketPayload` | cov | 209 | 210 |
| `network.codec.ByteBufCodecs` | gap | 204 | 206 |
| `network.protocol.common.custom.CustomPacketPayload$Type` | cov | 200 | 201 |
| `network.protocol.configuration.ServerConfigurationPacketListener` | cov | 7 | 7 |

#### Single-mod scan: Minecraft NBT and codecs (2 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `nbt.CompoundTag` | cov | 257 | 262 |
| `nbt.Tag` | cov | 210 | 217 |

#### Single-mod scan: Minecraft data generation (2 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `data.worldgen.BootstrapContext` | gap | 0 | 39 |
| `data.worldgen.placement.OrePlacements` | gap | 0 | 1 |

#### Single-mod scan: Mojang Brigadier (10 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `com.mojang.brigadier.exceptions.CommandSyntaxException` | cov | 124 | 130 |
| `com.mojang.brigadier.context.CommandContext` | cov | 122 | 127 |
| `com.mojang.brigadier.Command` | cov | 121 | 126 |
| `com.mojang.brigadier.CommandDispatcher` | cov | 120 | 128 |
| `com.mojang.brigadier.builder.LiteralArgumentBuilder` | cov | 120 | 127 |
| `com.mojang.brigadier.builder.ArgumentBuilder` | cov | 120 | 126 |
| `com.mojang.brigadier.tree.LiteralCommandNode` | cov | 118 | 125 |
| `com.mojang.brigadier.arguments.ArgumentType` | cov | 105 | 109 |
| `com.mojang.brigadier.builder.RequiredArgumentBuilder` | cov | 104 | 108 |
| `com.mojang.brigadier.arguments.IntegerArgumentType` | cov | 48 | 53 |

#### Single-mod scan: Mojang DataFixerUpper codecs (14 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `com.mojang.serialization.Codec` | gap | 244 | 255 |
| `com.mojang.serialization.MapCodec` | gap | 220 | 230 |
| `com.mojang.serialization.codecs.PrimitiveCodec` | gap | 196 | 205 |
| `com.mojang.datafixers.Products` | gap | 195 | 204 |
| `com.mojang.datafixers.kinds.App` | gap | 195 | 204 |
| `com.mojang.datafixers.kinds.Applicative` | gap | 195 | 204 |
| `com.mojang.serialization.codecs.RecordCodecBuilder` | gap | 191 | 202 |
| `com.mojang.serialization.codecs.RecordCodecBuilder$Instance` | gap | 191 | 202 |
| `com.mojang.datafixers.util.Function3` | gap | 156 | 159 |
| `com.mojang.datafixers.Products$P2` | gap | 132 | 134 |
| `com.mojang.datafixers.util.Pair` | gap | 129 | 139 |
| `com.mojang.datafixers.Products$P3` | gap | 125 | 129 |
| `com.mojang.datafixers.types.Type` | gap | 125 | 128 |
| `com.mojang.datafixers.Products$P1` | gap | 102 | 111 |

#### Single-mod scan: Mojang logging and auth (2 rows, 0 unused)

| Row | cls | mods | all |
|:--|:--|--:|--:|
| `com.mojang.logging.LogUtils` | cov | 107 | 126 |
| `com.mojang.authlib.GameProfile` | cov | 89 | 92 |

## 12. Open points and what could not be classified

- The side of a class is a heuristic (markers and package names). 8436 classes are mixed (7% of all classes): they reference client API without a marker and count as server-side in the srv columns. Method-level `@OnlyIn` is not tracked.
- 370 `DeferredRegister` call sites and 9 `addListener` calls could not be tied to a registry or an event class (instance fields and dynamic receivers). Loops and helpers hide entries: registry counts are lower bounds.
- Resource counts rest on complete `assets` and `data` folders. Mods that generate resources at runtime (KubeJS scripts in the pack folder, dynamic resource packs) are not counted.
- Kotlin and low-code mods (modLoader: `kotlinforforge` 5, `lowcodefml` 2) are scanned as bytecode like the others.
- 4 moved classes have several candidates with the same simple name; the table shows the first. 5 referenced NeoForge classes are not in the 21.1 index (artifacts `coremods`, `earlydisplay` and `JarJar` were not indexed): `coremod.api.ASMAPI`, `fml.earlydisplay.ColourScheme`, `fml.earlydisplay.ColourScheme$Colour`, `fml.earlydisplay.DisplayWindow`, `jarjar.nio.util.Lazy`.
- 39 referenced members are inherited from a library class that no index holds (guava, netty, gson) and are neither counted as resolved nor as broken. 33 mixin classes are in no mixin config, and 134 mixin targets belong to no jar of the pack and no known library.
- The 26.3 check has no client jar: `net.minecraft.client.*` and `com.mojang.blaze3d.*` classes and members are not checked.
- The mapping rows are in 26.3 names. A row whose class was renamed between 21.1 and 26.3 without an alias in section 11 shows 0 mods although the pack may use the old name; section 11.3 lists the old names that have no row.
- Behaviour changes behind an unchanged name or signature are not detected (see section 10.1).

