#[allow(clippy::wildcard_imports)]
use super::*;

impl BedrockClient {
    pub fn handle_request_ability(
        &self,
        player: &Arc<Player>,
        packet: &pumpkin_protocol::bedrock::server::request_ability::SRequestAbility,
    ) {
        player.update_last_action_time();
        let ability_id = packet.ability.0;
        match ability_id {
            9 => {
                // Flying
                if let pumpkin_protocol::bedrock::server::request_ability::AbilityValue::Bool(
                    requested_flying,
                ) = packet.value
                {
                    player.request_flying(requested_flying);
                    player.send_abilities_update();
                }
            }
            _ => {
                debug!("Received RequestAbility packet for unhandled ability {ability_id}");
            }
        }
    }
}
