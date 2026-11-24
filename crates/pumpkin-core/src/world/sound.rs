//! The sound entry points of a world.
//!
//! Every sound that a world sends to its players goes through [`World::play_seeded_sound`] (a
//! position) or [`World::play_seeded_sound_at_entity`] (an entity), as in vanilla
//! `Level.playSeededSound`. The two calls fire the `NeoForge` `PlayLevelSoundEvent` and pick the
//! recipients with the range of the sound.
//!
//! Three kinds of sound packet stay direct, as in vanilla: a sound for one player
//! (`Player::play_sound`, like `ServerPlayer.playNotifySound`), the `/playsound` command, and
//! the Bedrock level sound event.

use std::sync::Arc;

use pumpkin_data::{dynamic_registries, sound::Sound, sound::SoundCategory};
use pumpkin_protocol::{IdOr, SoundEvent, java::client::play::CEntitySoundEffect};
use pumpkin_util::math::vector3::Vector3;

use super::World;
use crate::{
    entity::{Entity, player::Player},
    net::ClientPlatform,
    plugin::api::events::world::play_level_sound::{
        PlayLevelSoundEventAtEntity, PlayLevelSoundEventAtPosition,
    },
};

/// A sound event as a client reads it: a registry id, or an event with a name and an optional
/// fixed range (`Holder<SoundEvent>`).
pub type SoundHolder = IdOr<SoundEvent>;

/// The sound event with the name `name`.
///
/// A vanilla sound gives its registry id. A sound that a mod registered gives its fixed range. A
/// name that no registry knows, such as a sound of a resource pack, gives a direct event with the
/// default range.
#[must_use]
pub fn sound_holder_from_name(name: &str) -> SoundHolder {
    if let Some(path) = name.strip_prefix("minecraft:")
        && let Some(sound) = Sound::from_name(path)
    {
        return IdOr::Id(sound as u16);
    }
    if !name.contains(':')
        && let Some(sound) = Sound::from_name(name)
    {
        return IdOr::Id(sound as u16);
    }
    IdOr::Value(SoundEvent {
        sound_name: name.into(),
        range: dynamic_registries::sound_event_by_name(name).and_then(|sound| sound.fixed_range),
    })
}

/// The name of a sound event, without the `minecraft:` namespace for a vanilla sound.
#[must_use]
pub fn sound_holder_name(sound: &SoundHolder) -> String {
    match sound {
        IdOr::Id(id) => Sound::NAMES
            .get(usize::from(*id))
            .map_or_else(String::new, |name| (*name).to_owned()),
        IdOr::Value(event) => event.sound_name.clone(),
    }
}

/// The range in blocks of `sound` at `volume`, as `SoundEvent.getRange`: the fixed range, or 16
/// blocks times a volume above 1.
#[must_use]
pub fn sound_range(sound: &SoundHolder, volume: f32) -> f32 {
    match sound {
        IdOr::Value(SoundEvent {
            range: Some(range), ..
        }) => *range,
        _ if volume > 1.0 => 16.0 * volume,
        _ => 16.0,
    }
}

/// Whether `player` is strictly closer than `range` to `position`, as `PlayerList.broadcast` tests.
pub(super) fn is_closer_than(player: &Player, position: &Vector3<f64>, range: f64) -> bool {
    player
        .living_entity
        .entity
        .pos
        .load()
        .squared_distance_to_vec(position)
        < range * range
}

/// The position that a player gets for a global level event at `center`.
///
/// As `ServerLevel.globalLevelEvent`, a player of the same world closer than 32 blocks gets the
/// center. A farther player gets a point 32 blocks from the player toward the center. A player of
/// another world gets its own position.
#[must_use]
pub fn global_event_position(
    center: Vector3<f64>,
    player: Vector3<f64>,
    same_world: bool,
) -> Vector3<f64> {
    if !same_world {
        player
    } else if center.squared_distance_to_vec(&player) < 32.0 * 32.0 {
        center
    } else {
        player.add(&center.sub(&player).normalize().multiply(32.0, 32.0, 32.0))
    }
}

impl World {
    /// This world as an `Arc`, or `None` when the server does not list it.
    pub(super) fn arc_self(&self) -> Option<Arc<Self>> {
        self.weak_self().and_then(|world| world.upgrade())
    }

    /// Fires the event that `make` builds from this world, or returns `None` without a call to
    /// `make` when no plugin listens.
    pub fn plugin_event_in_world<E: crate::plugin::Payload + Send + Sync + 'static>(
        &self,
        make: impl FnOnce(Arc<Self>) -> E,
    ) -> Option<E> {
        let server = self.server.upgrade()?;
        if !server.plugin_manager.has_handlers::<E>() {
            return None;
        }
        let mut event = make(self.arc_self()?);
        server.plugin_manager.fire_blocking(&server, &mut event);
        Some(event)
    }

    /// Plays `sound` at `position` to the Java players in its range, except `except`, and
    /// returns how many players got it.
    ///
    /// Vanilla `ServerLevel.playSeededSound` with a position. It fires
    /// [`PlayLevelSoundEventAtPosition`]: a handler can change the sound, the category, the
    /// volume and the pitch, and a cancel or an empty sound sends nothing. The range is
    /// [`sound_range`] of the sound with the volume after the handlers. `PlayerList.broadcast`
    /// sends to the players closer than the range, in this world.
    ///
    /// An active [`FireDeferral`](super::fire_deferral::FireDeferral) queues the sound and
    /// returns 0.
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_sound(
        &self,
        except: Option<&Player>,
        position: &Vector3<f64>,
        sound: SoundHolder,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> usize {
        if super::fire_deferral::is_active()
            && let Some(world) = self.arc_self()
        {
            let except_id = except.map(|player| player.gameprofile.id);
            let position = *position;
            if let Some(fire) = super::fire_deferral::defer(move || {
                let except = except_id.and_then(|id| world.get_player_by_uuid(id));
                world.play_seeded_sound_now(
                    except.as_deref(),
                    &position,
                    sound,
                    category,
                    volume,
                    pitch,
                    seed,
                );
            }) {
                fire();
            }
            return 0;
        }
        self.play_seeded_sound_now(except, position, sound, category, volume, pitch, seed)
    }

    #[allow(clippy::too_many_arguments)]
    fn play_seeded_sound_now(
        &self,
        except: Option<&Player>,
        position: &Vector3<f64>,
        sound: SoundHolder,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> usize {
        let fired = self.plugin_event_in_world(|world| {
            PlayLevelSoundEventAtPosition::new(
                world,
                *position,
                sound.clone(),
                category,
                volume,
                pitch,
            )
        });
        let (sound, category, volume, pitch) = match fired {
            None => (sound, category, volume, pitch),
            Some(event) if event.cancelled => return 0,
            Some(event) => match event.sound {
                Some(changed) => (changed, event.source, event.new_volume, event.new_pitch),
                None => return 0,
            },
        };
        let range = sound_range(&sound, volume);
        let packet = pumpkin_protocol::java::client::play::CSoundEffect::new(
            sound, category, position, volume, pitch, seed,
        );
        self.send_to_players_in_range(except, position, range, &packet)
    }

    /// Plays `sound` at `position` to all players in range with a random seed, like vanilla
    /// `Level.playSound(null, x, y, z, sound, source, volume, pitch)`. Returns how many Java
    /// players got it, or 0 when an active [`FireDeferral`](super::fire_deferral::FireDeferral)
    /// queued the sound.
    pub fn play_sound_holder(
        &self,
        position: &Vector3<f64>,
        sound: SoundHolder,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
    ) -> usize {
        self.play_seeded_sound(
            None,
            position,
            sound,
            category,
            volume,
            pitch,
            rand::random::<i64>(),
        )
    }

    /// Plays `sound` so that it follows `entity`, to the Java players in range of the entity,
    /// except `except`, and returns how many players got it.
    ///
    /// Vanilla `ServerLevel.playSeededSound` with an entity. It fires
    /// [`PlayLevelSoundEventAtEntity`], with the rules of [`Self::play_seeded_sound`]. The event
    /// carries the entity that the world lists under the id of `entity`; an entity that the world
    /// does not list plays the sound without the event.
    ///
    /// An active [`FireDeferral`](super::fire_deferral::FireDeferral) queues the sound and
    /// returns 0.
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_sound_at_entity(
        &self,
        except: Option<&Player>,
        entity: &Entity,
        sound: SoundHolder,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> usize {
        let listed = self.get_entity_by_id(entity.entity_id);
        let (entity_id, position) = (entity.entity_id, entity.pos.load());
        if super::fire_deferral::is_active()
            && let Some(world) = self.arc_self()
        {
            let except_id = except.map(|player| player.gameprofile.id);
            if let Some(fire) = super::fire_deferral::defer(move || {
                let except = except_id.and_then(|id| world.get_player_by_uuid(id));
                world.play_seeded_sound_at_entity_now(
                    except.as_deref(),
                    entity_id,
                    &position,
                    listed,
                    sound,
                    category,
                    volume,
                    pitch,
                    seed,
                );
            }) {
                fire();
            }
            return 0;
        }
        self.play_seeded_sound_at_entity_now(
            except, entity_id, &position, listed, sound, category, volume, pitch, seed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn play_seeded_sound_at_entity_now(
        &self,
        except: Option<&Player>,
        entity_id: i32,
        position: &Vector3<f64>,
        listed: Option<Arc<dyn crate::entity::EntityBase>>,
        sound: SoundHolder,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> usize {
        let fired = listed.and_then(|listed| {
            self.plugin_event_in_world(|world| {
                PlayLevelSoundEventAtEntity::new(
                    world,
                    listed,
                    sound.clone(),
                    category,
                    volume,
                    pitch,
                )
            })
        });
        let (sound, category, volume, pitch) = match fired {
            None => (sound, category, volume, pitch),
            Some(event) if event.cancelled => return 0,
            Some(event) => match event.sound {
                Some(changed) => (changed, event.source, event.new_volume, event.new_pitch),
                None => return 0,
            },
        };
        let range = sound_range(&sound, volume);
        let packet = CEntitySoundEffect::new(
            sound,
            category,
            pumpkin_protocol::VarInt(entity_id),
            volume,
            pitch,
            seed,
        );
        self.send_to_players_in_range(except, position, range, &packet)
    }

    /// Plays the vanilla `sound` so that it follows `entity`, with a random seed, like
    /// `Level.playSound(except, entity, sound, source, volume, pitch)`. Returns 0 when an active
    /// [`FireDeferral`](super::fire_deferral::FireDeferral) queued the sound.
    pub fn play_sound_at_entity(
        &self,
        except: Option<&Player>,
        entity: &Entity,
        sound: Sound,
        category: SoundCategory,
        volume: f32,
        pitch: f32,
    ) -> usize {
        self.play_seeded_sound_at_entity(
            except,
            entity,
            IdOr::Id(sound as u16),
            category,
            volume,
            pitch,
            rand::random::<i64>(),
        )
    }

    /// Sends `packet` to the Java players of this world closer than `range` to `position`,
    /// except `except`, and returns how many players got it.
    fn send_to_players_in_range<P: pumpkin_protocol::ClientPacket>(
        &self,
        except: Option<&Player>,
        position: &Vector3<f64>,
        range: f32,
        packet: &P,
    ) -> usize {
        let range = f64::from(range);
        let mut count = 0;
        let players = self.players.load();
        let recipients = players
            .iter()
            .filter(|player| {
                except.is_none_or(|except| except.gameprofile.id != player.gameprofile.id)
                    && matches!(player.client.as_ref(), ClientPlatform::Java(_))
                    && is_closer_than(player, position, range)
            })
            .inspect(|_| count += 1);
        Self::broadcast_java_players(packet, recipients);
        count
    }
}
