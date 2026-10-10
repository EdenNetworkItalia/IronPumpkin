//! One recorded packet, and the decoding of custom payload bodies through the `NeoForge` codecs.

use std::fmt::Write as _;

use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_protocol::java::neoforge::{
    CommonRegisterPayload, CommonVersionPayload, ConfigFilePayload,
    ExtensibleEnumAcknowledgePayload, ExtensibleEnumDataPayload, FeatureFlagAcknowledgePayload,
    FeatureFlagDataPayload, FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload,
    FrozenRegistrySyncStartPayload, KnownRegistryDataMapsPayload,
    KnownRegistryDataMapsReplyPayload, MinecraftRegisterPayload, MinecraftUnregisterPayload,
    ModdedNetworkPayload, ModdedNetworkQueryPayload, ModdedNetworkSetupFailedPayload, NetworkCheck,
    PacketFlow, SplitPacketPayload, decode_exact,
};
use pumpkin_protocol::ser::{NetworkReadSliceExt, ReadingError};
use pumpkin_util::identifier::Identifier;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Clientbound,
    Serverbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Login,
    Configuration,
}

/// Result of decoding a custom payload body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decode {
    /// The channel has a codec and the whole body decoded.
    Ok,
    /// The channel has a codec and decoding failed or left bytes over.
    Error,
    /// No codec for this channel; the body was only recorded.
    NoCodec,
    /// The entry is not a custom payload.
    NotPayload,
}

/// One packet, as written to the JSONL recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: usize,
    pub direction: Direction,
    pub state: State,
    /// Packet name, such as `custom_payload` or `select_known_packs`.
    pub packet: String,
    /// The custom payload channel, for `custom_payload` packets.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub channel: Option<String>,
    /// Length of the custom payload body, or of the packet body for other packets.
    pub length: usize,
    /// The packet was reassembled from `neoforge:split` slices.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub reassembled: bool,
    pub decode: Decode,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    /// The body bytes, hex encoded.
    pub data: String,
}

impl Entry {
    /// One line for the console.
    #[must_use]
    pub fn line(&self) -> String {
        let arrow = match self.direction {
            Direction::Clientbound => "S->C",
            Direction::Serverbound => "C->S",
        };
        let mut line = format!("{:>4} {arrow} {}", self.seq, self.packet);
        if let Some(channel) = &self.channel {
            let _ = write!(line, " {channel}");
        }
        let _ = write!(line, " len={}", self.length);
        if self.reassembled {
            line.push_str(" (reassembled)");
        }
        match self.decode {
            Decode::Ok => line.push_str(" decode=ok"),
            Decode::Error => line.push_str(" decode=ERROR"),
            Decode::NoCodec => line.push_str(" decode=no_codec"),
            Decode::NotPayload => {}
        }
        if let Some(summary) = &self.summary {
            let _ = write!(line, " {summary}");
        }
        if let Some(error) = &self.error {
            let _ = write!(line, " error: {error}");
        }
        line
    }
}

/// Decodes a custom payload body with the codec for `channel`.
#[must_use]
pub fn decode_payload(channel: &str, data: &[u8]) -> (Decode, Option<String>, Option<String>) {
    let result = match channel {
        CommonVersionPayload::CHANNEL => summarize(data, CommonVersionPayload::read, |p| {
            format!("versions={:?}", p.versions)
        }),
        CommonRegisterPayload::CHANNEL => summarize(data, CommonRegisterPayload::read, |p| {
            format!(
                "version={} protocol={} channels={}",
                p.version,
                p.protocol.map_or("unknown", |p| p.id()),
                join(p.channels.iter().map(ToString::to_string))
            )
        }),
        ModdedNetworkQueryPayload::CHANNEL => {
            summarize(data, ModdedNetworkQueryPayload::read, |p| show_query(&p))
        }
        ModdedNetworkPayload::CHANNEL => summarize(data, ModdedNetworkPayload::read, |p| {
            let protocols = p.setup.channels.iter().map(|(protocol, channels)| {
                let channels = channels
                    .values()
                    .map(|c| format!("{}@{}", c.id, c.chosen_version));
                format!("{}={}", protocol.id(), join(channels))
            });
            format!("setup={{{}}}", protocols.collect::<Vec<_>>().join(" "))
        }),
        ModdedNetworkSetupFailedPayload::CHANNEL => {
            summarize(data, ModdedNetworkSetupFailedPayload::read, |p| {
                join(
                    p.failure_reasons
                        .into_iter()
                        .map(|(id, reason)| format!("{id}: {}", reason.get_text())),
                )
            })
        }
        FrozenRegistrySyncStartPayload::CHANNEL => {
            summarize(data, FrozenRegistrySyncStartPayload::read, |p| {
                format!(
                    "registries={}",
                    join(p.to_access.iter().map(ToString::to_string))
                )
            })
        }
        FrozenRegistryPayload::CHANNEL => summarize(data, FrozenRegistryPayload::read, |p| {
            format!(
                "registry={} ids={} aliases={}",
                p.registry_name,
                p.snapshot.ids.len(),
                p.snapshot.aliases.len()
            )
        }),
        FrozenRegistrySyncCompletedPayload::CHANNEL => {
            summarize(data, FrozenRegistrySyncCompletedPayload::read, |_| {
                String::new()
            })
        }
        ConfigFilePayload::CHANNEL => summarize(data, ConfigFilePayload::read, |p| {
            format!("file={} bytes={}", p.file_name, p.contents.len())
        }),
        FeatureFlagDataPayload::CHANNEL => summarize(data, FeatureFlagDataPayload::read, |p| {
            format!(
                "flags={}",
                join(p.modded_flags.iter().map(ToString::to_string))
            )
        }),
        SplitPacketPayload::CHANNEL => summarize(data, SplitPacketPayload::read, |p| {
            format!(
                "state={} slice={}",
                p.payload.first().copied().unwrap_or_default(),
                p.payload.len().saturating_sub(1)
            )
        }),
        MinecraftRegisterPayload::CHANNEL => summarize(data, MinecraftRegisterPayload::read, |p| {
            format!(
                "channels={}",
                join(p.channels.iter().map(ToString::to_string))
            )
        }),
        MinecraftUnregisterPayload::CHANNEL => {
            summarize(data, MinecraftUnregisterPayload::read, |p| {
                format!(
                    "channels={}",
                    join(p.channels.iter().map(ToString::to_string))
                )
            })
        }
        "minecraft:brand" => summarize(
            data,
            |r| r.get_str_borrowed().map(str::to_owned),
            |brand| format!("brand={brand:?}"),
        ),
        _ => match decode_check_payload(channel, data) {
            Some(result) => result,
            None => return (Decode::NoCodec, None, None),
        },
    };
    match result {
        Ok(summary) => (Decode::Ok, (!summary.is_empty()).then_some(summary), None),
        Err(e) => (Decode::Error, None, Some(e.to_string())),
    }
}

/// The payloads of the `NeoForge` data map, extensible enum and feature flag checks.
fn decode_check_payload(channel: &str, data: &[u8]) -> Option<Result<String, ReadingError>> {
    Some(match channel {
        FeatureFlagAcknowledgePayload::CHANNEL => {
            summarize(data, FeatureFlagAcknowledgePayload::read, |_| String::new())
        }
        KnownRegistryDataMapsPayload::CHANNEL => {
            summarize(data, KnownRegistryDataMapsPayload::read, |p| {
                show_data_maps(p.data_maps.iter().map(|(registry, maps)| {
                    let maps = maps
                        .iter()
                        .map(|m| format!("{}{}", m.id, if m.mandatory { "!" } else { "" }));
                    (registry, join(maps))
                }))
            })
        }
        KnownRegistryDataMapsReplyPayload::CHANNEL => {
            summarize(data, KnownRegistryDataMapsReplyPayload::read, |p| {
                show_data_maps(
                    p.data_maps.iter().map(|(registry, ids)| {
                        (registry, join(ids.iter().map(ToString::to_string)))
                    }),
                )
            })
        }
        ExtensibleEnumDataPayload::CHANNEL => {
            summarize(data, ExtensibleEnumDataPayload::read, |p| show_enums(&p))
        }
        ExtensibleEnumAcknowledgePayload::CHANNEL => {
            summarize(data, ExtensibleEnumAcknowledgePayload::read, |_| {
                String::new()
            })
        }
        _ => return None,
    })
}

fn show_query(payload: &ModdedNetworkQueryPayload) -> String {
    let protocols = payload.queries.iter().map(|(protocol, components)| {
        let channels = components.iter().map(|c| {
            let flow = match c.flow {
                Some(PacketFlow::Serverbound) => ">s",
                Some(PacketFlow::Clientbound) => ">c",
                None => "",
            };
            let optional = if c.optional { "?" } else { "" };
            format!("{}@{}{flow}{optional}", c.id, c.version)
        });
        format!("{}={}", protocol.id(), join(channels))
    });
    format!("queries={{{}}}", protocols.collect::<Vec<_>>().join(" "))
}

fn show_data_maps<'a>(registries: impl Iterator<Item = (&'a Identifier, String)>) -> String {
    let registries = registries.map(|(registry, maps)| format!("{registry}={maps}"));
    format!("data_maps={{{}}}", registries.collect::<Vec<_>>().join(" "))
}

fn show_enums(payload: &ExtensibleEnumDataPayload) -> String {
    let entries = payload.enum_entries.values().map(|e| {
        let check = match e.network_check {
            NetworkCheck::Clientbound => ">c",
            NetworkCheck::Serverbound => ">s",
            NetworkCheck::Bidirectional => "",
        };
        let extension = e.data.as_ref().map_or_else(String::new, |d| {
            format!(
                "+{}/{}{}",
                d.vanilla_count,
                d.total_count,
                join(d.entries.iter().cloned())
            )
        });
        format!("{}{check}{extension}", e.class_name)
    });
    format!("enums={}", join(entries))
}

fn summarize<T>(
    data: &[u8],
    read: impl FnOnce(&mut &[u8]) -> Result<T, ReadingError>,
    show: impl FnOnce(T) -> String,
) -> Result<String, ReadingError> {
    decode_exact(data, read).map(show)
}

fn join(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
}

/// The reason of a configuration disconnect: one text component and nothing after it.
pub fn read_disconnect_reason(
    data: &[u8],
) -> Result<pumpkin_util::text::TextComponent, ReadingError> {
    decode_exact(data, |read| read.get_component(&CURRENT_MC_VERSION))
}
