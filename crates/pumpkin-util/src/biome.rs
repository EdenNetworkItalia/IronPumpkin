use std::sync::LazyLock;

use serde::Deserialize;

use crate::{noise::simplex::OctaveSimplexNoiseSampler, random::legacy_rand::LegacyRand};

/// Global noise sampler for biome temperature variation.
pub static TEMPERATURE_NOISE: LazyLock<OctaveSimplexNoiseSampler> = LazyLock::new(|| {
    let mut rand = LegacyRand::from_seed(1234);
    OctaveSimplexNoiseSampler::new(&mut rand, &[0])
});

/// Global noise sampler for frozen ocean biome adjustments.
pub static FROZEN_OCEAN_NOISE: LazyLock<OctaveSimplexNoiseSampler> = LazyLock::new(|| {
    let mut rand = LegacyRand::from_seed(3456);
    OctaveSimplexNoiseSampler::new(&mut rand, &[-2, -1, 0])
});

/// Global noise sampler for foliage-based temperature adjustments.
pub static FOLIAGE_NOISE: LazyLock<OctaveSimplexNoiseSampler> = LazyLock::new(|| {
    let mut rand = LegacyRand::from_seed(2345);
    OctaveSimplexNoiseSampler::new(&mut rand, &[0])
});

/// Modifiers that adjust the base biome temperature.
#[derive(Clone, Deserialize, Copy, Hash, PartialEq, Eq, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum TemperatureModifier {
    /// No temperature modification.
    None,
    /// Frozen biome adjustment, typically used for frozen oceans and snowy areas.
    Frozen,
}

impl TemperatureModifier {
    /// Applies the temperature modifier to a given position and base temperature.
    ///
    /// # Parameters
    /// - `x`: X coordinate in the world.
    /// - `z`: Z coordinate in the world.
    /// - `temperature`: The base biome temperature.
    ///
    /// # Returns
    /// - The modified temperature as an `f32`.
    pub fn convert_temperature(&self, x: f64, z: f64, temperature: f32) -> f32 {
        match self {
            Self::None => temperature,
            Self::Frozen => {
                // Vanilla samples in `f32` and compares in `f64`.
                let frozen_ocean_sample =
                    f64::from(FROZEN_OCEAN_NOISE.sample_f32(x * 0.05, z * 0.05) * 7.0);
                let foliage_sample = f64::from(FOLIAGE_NOISE.sample_f32(x * 0.2, z * 0.2));

                let threshold = frozen_ocean_sample + foliage_sample;
                if threshold < 0.3 {
                    let foliage_sample = f64::from(FOLIAGE_NOISE.sample_f32(x * 0.09, z * 0.09));
                    if foliage_sample < 0.8 {
                        return 0.2f32;
                    }
                }

                temperature
            }
        }
    }
}

/// Represents weather information for a biome, including temperature and precipitation.
#[derive(Clone, Debug)]
pub struct Weather {
    has_precipitation: bool,
    /// Base temperature of the biome.
    temperature: f32,
    /// Modifier affecting the base temperature.
    temperature_modifier: TemperatureModifier,
    /// Rate of rainfall or snowfall in the biome.
    #[expect(dead_code)]
    downfall: f32,
}

impl Weather {
    /// Creates a new `Weather` instance.
    ///
    /// # Parameters
    /// - `has_precipitation`: Whether the biome has precipitation.
    /// - `temperature`: Base temperature of the biome.
    /// - `temperature_modifier`: Modifier affecting the temperature.
    /// - `downfall`: Amount of rainfall or snowfall.
    #[must_use]
    pub const fn new(
        has_precipitation: bool,
        temperature: f32,
        temperature_modifier: TemperatureModifier,
        downfall: f32,
    ) -> Self {
        Self {
            has_precipitation,
            temperature,
            temperature_modifier,
            downfall,
        }
    }

    /// Returns the raw base temperature of the biome, without any height or modifier adjustment.
    #[must_use]
    pub const fn base_temperature(&self) -> f32 {
        self.temperature
    }

    #[must_use]
    pub const fn has_precipitation(&self) -> bool {
        self.has_precipitation
    }

    /// Computes the effective temperature at a given position.
    ///
    /// # Parameters
    /// - `x`, `z`: World coordinates.
    /// - `y`: Y-level of the position.
    /// - `sea_level`: Sea level in the world.
    ///
    /// # Returns
    /// - The temperature at the position as `f32`.
    ///
    /// # Notes
    /// - This function is computationally expensive and should be cached.
    /// - Temperature is influenced by `TemperatureModifier` and noise samplers.
    pub fn compute_temperature(&self, x: f64, y: i32, z: f64, sea_level: i32) -> f32 {
        let modified_temperature =
            self.temperature_modifier
                .convert_temperature(x, z, self.temperature);
        let offset_sea_level = sea_level + 17;

        if y > offset_sea_level {
            // Vanilla divides the block coordinates as `f32`.
            let temperature_noise = TEMPERATURE_NOISE
                .sample_f32(f64::from(x as f32 / 8.0), f64::from(z as f32 / 8.0))
                * 8.0;

            modified_temperature
                - (temperature_noise + y as f32 - offset_sea_level as f32) * 0.05f32 / 40.0f32
        } else {
            modified_temperature
        }
    }

    #[must_use]
    pub fn warm_enough_to_rain(&self, x: i32, y: i32, z: i32, sea_level: i32) -> bool {
        self.compute_temperature(f64::from(x), y, f64::from(z), sea_level) >= 0.15
    }

    #[must_use]
    pub fn is_rain_at(&self, x: i32, y: i32, z: i32, sea_level: i32) -> bool {
        self.has_precipitation && self.warm_enough_to_rain(x, y, z, sea_level)
    }
}

#[cfg(test)]
mod tests {
    use super::{TemperatureModifier, Weather};

    /// The bits of the vanilla 26.3 floats `BIOME_INFO_NOISE.get(x / 400.0, z / 400.0)`,
    /// `FROZEN_TEMPERATURE_NOISE.get(x * 0.05, z * 0.05)` and
    /// `TEMPERATURE_NOISE.get(x / 8.0F, z / 8.0F)`, from the server jar.
    #[test]
    fn noise_matches_the_vanilla_floats() {
        use super::{FOLIAGE_NOISE, FROZEN_OCEAN_NOISE, TEMPERATURE_NOISE};
        for (x, z, info, frozen, temperature) in [
            (768, -1824, 0x3ee9_c199, 0x3bb8_9f00, 0x3f5e_1d57),
            (-12345, 6789, 0xbe83_2e19, 0xbe34_9962, 0xbedb_c560),
            (
                29_999_983,
                -29_999_977,
                0x3e77_3983,
                0xbe73_b02f,
                0xbefd_7cfd,
            ),
            (
                16_777_217,
                -20_000_001,
                0x3e35_2f57,
                0xbeec_1367,
                0x3f62_fe11,
            ),
            (-79070, 1786, 0x3f00_0000, 0x3e8e_74ae, 0x3f64_143b),
        ] {
            let (fx, fz) = (f64::from(x), f64::from(z));
            let bits = |value: f32| value.to_bits();
            assert_eq!(
                bits(FOLIAGE_NOISE.sample_f32(fx / 400.0, fz / 400.0)),
                info,
                "{x} {z}"
            );
            assert_eq!(
                bits(FROZEN_OCEAN_NOISE.sample_f32(fx * 0.05, fz * 0.05)),
                frozen,
                "{x} {z}"
            );
            let (tx, tz) = (f64::from(x as f32 / 8.0), f64::from(z as f32 / 8.0));
            assert_eq!(
                bits(TEMPERATURE_NOISE.sample_f32(tx, tz)),
                temperature,
                "{x} {z}"
            );
        }
    }

    #[test]
    fn precipitation_controls_rain() {
        let weather = Weather::new(false, 1.0, TemperatureModifier::None, 0.0);
        assert!(!weather.is_rain_at(0, 64, 0, 63));
    }

    #[test]
    fn warm_precipitation_is_rain() {
        let weather = Weather::new(true, 1.0, TemperatureModifier::None, 0.0);
        assert!(weather.is_rain_at(0, 64, 0, 63));
    }

    #[test]
    fn cold_precipitation_is_snow() {
        let weather = Weather::new(true, 0.0, TemperatureModifier::None, 0.0);
        assert!(!weather.is_rain_at(0, 64, 0, 63));
    }
}
