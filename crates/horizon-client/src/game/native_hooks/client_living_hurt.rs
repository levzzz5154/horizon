use super::*;
use crate::game::commands::Target;
use horizon_protocol::packets::play::DamageEvent;

impl Game<'_> {
    pub(in crate::game) fn observe_client_living_hurt(&mut self, packet: &DamageEvent) {
        if !mod_api::__private::listened(mod_api::HookId::ClientLivingHurt) {
            return;
        }
        let (Some(epoch), Ok(id)) = (self.native_lifecycle.epoch(), u32::try_from(packet.id))
        else {
            return;
        };
        let entity = EntityRef::new(epoch, id);
        if !matches!(
            self.native_client_entity_target(entity),
            Some(Target::Player | Target::Mob(_) | Target::Dragon(_))
        ) {
            return;
        }
        let resolve = |id: i32| self.entity(u32::try_from(id).ok()?);
        let causing_entity = packet.cause.and_then(resolve);
        let direct_entity = packet.direct.and_then(resolve);
        self.native_client_living_hurt(entity, causing_entity, direct_entity);
    }

    pub(in crate::game) fn native_client_living_hurt(
        &mut self,
        entity: EntityRef,
        causing_entity: Option<EntityRef>,
        direct_entity: Option<EntityRef>,
    ) {
        self.fire_client_living_hurt(entity, causing_entity, direct_entity);
    }
}
