use super::*;

#[test]
fn config_custom_payload_appends_only_guest_responses() {
    let mut state = PluginHostState::new();
    let component = ModdedNetworkQueryComponent {
        id: Identifier::parse_static("probe:sync"),
        version: "2".to_string(),
        flow: Some(PacketFlow::Clientbound),
        optional: true,
    };
    let modded_channels = BTreeMap::from([(
        ConnectionProtocol::Configuration,
        BTreeSet::from([component]),
    )]);
    let mut event = ConfigCustomPayloadEvent {
        connection_id: 7,
        player_name: "Probe".to_string(),
        player_uuid: uuid::Uuid::from_u128(1),
        version: pumpkin_data::packet::CURRENT_MC_VERSION,
        connection_type: ConnectionType::NeoForge,
        modded_channels: modded_channels.clone(),
        ad_hoc_channels: Vec::new(),
        channel: "minecraft:brand".to_string(),
        data: Bytes::from_static(b"neoforge"),
        responses: vec![("native:reply".to_string(), Bytes::from_static(b"n"))],
    };
    let Event::ConfigCustomPayloadEvent(mut returned) = event.to_wasm_event(&mut state) else {
        panic!("expected a config custom payload event");
    };
    assert_eq!(
        from_wit_modded_channels(returned.modded_channels.clone()),
        modded_channels
    );
    returned.player_name = "Rewritten".to_string();
    returned.connection_type = WitConnectionType::Other;
    returned.modded_channels.clear();
    returned.channel = "probe:other".to_string();
    returned.responses = vec![OutgoingPayload {
        channel: "guest:reply".to_string(),
        data: b"g".to_vec(),
    }];
    returned.responses.push(OutgoingPayload {
        channel: "guest:second".to_string(),
        data: b"s".to_vec(),
    });

    event.apply_wasm_event(Event::ConfigCustomPayloadEvent(returned), &mut state);

    assert_eq!(event.player_name, "Probe");
    assert_eq!(event.connection_type, ConnectionType::NeoForge);
    assert_eq!(event.modded_channels, modded_channels);
    assert_eq!(event.channel, "minecraft:brand");
    assert_eq!(
        event.responses,
        vec![
            ("native:reply".to_string(), Bytes::from_static(b"n")),
            ("guest:second".to_string(), Bytes::from_static(b"s")),
        ]
    );
}
