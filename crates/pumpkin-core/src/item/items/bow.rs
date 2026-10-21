use std::any::Any;
use std::sync::atomic::Ordering;

use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::entity::projectile::arrow::ArrowEntity;
use crate::item::items::projectile_weapon::ProjectileWeaponItem;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_protocol::IdOr;
use pumpkin_protocol::java::client::play::CSoundEffect;
use pumpkin_util::GameMode;

pub struct BowItem;

impl ItemMetadata for BowItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::BOW.id])
    }
}

impl ItemBehaviour for BowItem {
    fn normal_use(&self, _item: &Item, player: &Player) {
        let inventory = player.inventory();
        let stack = inventory.held_item();

        // Vanilla `BowItem.use`: a creative player always finds a projectile.
        let (projectile, _) =
            ProjectileWeaponItem::get_projectile(player, &stack, player.find_arrow());
        let found_projectile = !projectile.is_empty();
        if crate::plugin::player_events::arrow_nock(player, found_projectile) || !found_projectile {
            return;
        }

        // Start the bow drawing animation
        player
            .living_entity
            .set_active_hand(pumpkin_util::Hand::Right, stack, Self::USE_DURATION);
    }

    fn on_stopped_using(&self, stack: &ItemStack, player: &Player) {
        Self::release_bow(player, stack);
    }

    fn get_use_duration(&self) -> i32 {
        Self::USE_DURATION
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl BowItem {
    /// The maximum number of ticks a bow can be drawn for
    pub const USE_DURATION: i32 = 72000;
    const MAX_DRAW_DURATION: f32 = 20.0;
    pub const ARROW_SPEED_MULTIPLIER: f32 = 3.0;

    /// Called when the player releases the bow
    pub fn release_bow(player: &Player, weapon: &ItemStack) {
        // Get the used ticks
        let use_ticks = player.living_entity.item_use_time.load(Ordering::Relaxed);
        let use_ticks = Self::USE_DURATION - use_ticks;

        // Check arrows again
        let (projectile, arrow_slot) =
            ProjectileWeaponItem::get_projectile(player, weapon, player.find_arrow());
        let gamemode = player.gamemode.load();

        if projectile.is_empty() {
            return;
        }

        let Some(use_ticks) = crate::plugin::player_events::arrow_loose(player, weapon, use_ticks)
        else {
            return;
        };
        // Check minimum draw time
        if use_ticks < 3 {
            return;
        }

        let projectile = projectile.copy_with_count(1);
        let infinite_projectile = projectile.item.id == Item::ARROW.id;

        // Calculate power and fire
        let power = Self::get_power_for_time(use_ticks);

        // Check for Infinity enchantment
        let has_infinity = weapon
            .get_data_component::<pumpkin_data::data_component_impl::EnchantmentsImpl>()
            .is_some_and(|enchantments| {
                enchantments
                    .enchantment
                    .iter()
                    .any(|(e, _)| **e == pumpkin_data::Enchantment::INFINITY)
            });

        let is_crit = (power - 1.0).abs() < f32::EPSILON;
        Self::shoot(
            player,
            weapon,
            std::slice::from_ref(&projectile),
            power,
            1.0,
            is_crit,
        );

        // Consume arrow (if not creative and no Infinity)
        if let Some(slot) = arrow_slot
            && gamemode != GameMode::Creative
            && !(has_infinity && infinite_projectile)
        {
            player.consume_arrow(slot);
        }

        // Damage bow
        player.damage_held_item(1);
    }

    /// Calculate the power/charge of the bow based on time held
    #[must_use]
    pub fn get_power_for_time(time_held: i32) -> f32 {
        let mut power = time_held as f32 / Self::MAX_DRAW_DURATION;
        power = (power * power + power * 2.0) / 3.0;
        if power > 1.0 {
            power = 1.0;
        }
        power
    }

    /// Creates projectile matching vanilla `ProjectileWeaponItem::createProjectile`.
    pub fn create_projectile(
        player: &Player,
        weapon: &ItemStack,
        projectile: &ItemStack,
        is_crit: bool,
    ) -> ArrowEntity {
        let world = player.world();
        let is_creative = player.gamemode.load() == GameMode::Creative;
        ProjectileWeaponItem::create_projectile(
            world,
            player.get_entity(),
            weapon,
            projectile,
            is_crit,
            is_creative,
        )
    }

    /// Shoot projectile matching vanilla `ProjectileWeaponItem::shoot`.
    pub fn shoot(
        player: &Player,
        weapon: &ItemStack,
        projectiles: &[ItemStack],
        power: f32,
        uncertainty: f32,
        is_crit: bool,
    ) {
        if power < 0.1 || projectiles.is_empty() {
            return;
        }

        let world = player.world();
        let is_creative = player.gamemode.load() == GameMode::Creative;
        let speed = power * ProjectileWeaponItem::ARROW_SPEED_MULTIPLIER;

        ProjectileWeaponItem::shoot_projectiles(
            &world,
            player.get_entity(),
            weapon,
            projectiles,
            speed,
            uncertainty,
            is_crit,
            is_creative,
        );

        let sound_pitch = 1.0 / (rand::random::<f32>() * 0.4 + 1.2) + power * 0.5;
        let sound_packet = CSoundEffect::new(
            IdOr::Id(Sound::EntityArrowShoot as u16),
            SoundCategory::Neutral,
            &player.position(),
            1.0,
            sound_pitch,
            0,
        );
        let chunk_pos = player.get_entity().chunk_pos.load();
        world.broadcast_to_chunk(chunk_pos, &sound_packet);
    }

    /// Fire an arrow from the bow with explicit critical flag
    pub fn fire_arrow_with_crit(
        player: &Player,
        power: f32,
        projectile: &ItemStack,
        is_crit: bool,
    ) {
        let held = player.inventory().held_item();
        Self::shoot(
            player,
            &held,
            std::slice::from_ref(projectile),
            power,
            1.0,
            is_crit,
        );
    }

    /// Fire an arrow from the bow
    pub fn fire_arrow(player: &Player, power: f32, projectile: &ItemStack) {
        Self::fire_arrow_with_crit(player, power, projectile, power >= 1.0);
    }
}
