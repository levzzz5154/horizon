use super::*;

impl Game<'_> {
    pub(in crate::game) fn native_register_villager_types(
        context: &mod_api::RegistrationContext,
        types: &mut dyn VillagerTypes,
    ) {
        Self::fire_register_villager_types(context, types);
    }
}
