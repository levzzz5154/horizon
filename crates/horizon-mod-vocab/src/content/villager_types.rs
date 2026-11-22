//! Declarative, load-owned villager types.

use crate::Identifier;

/// A villager type, its spawn selectors and its two clothing textures.
// exhaustive: a mod declares its villager type as a struct literal.
#[derive(Clone, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
pub struct VillagerTypeDefinition {
    /// Persistent namespaced identity of the type.
    pub key: Identifier,
    /// Individual biomes that select this type.
    pub biomes: Vec<Identifier>,
    /// Biome tags that select this type, written without a leading `#`.
    pub biome_tags: Vec<Identifier>,
    /// Villager clothing texture identifier, relative to `textures/` and without `.png`.
    pub villager_texture: Identifier,
    /// Zombie-villager clothing texture identifier, relative to `textures/` and without `.png`.
    pub zombie_villager_texture: Identifier,
}

/// Load-scoped villager type registrar. The calling mod is recorded as owner.
pub trait VillagerTypes {
    /// Declare a type for validation with the rest of the candidate registry.
    fn register(&mut self, definition: VillagerTypeDefinition);
}

// no wire: engine-owned registrar backend, borrowed only during registration.
/// Engine storage for declarations and their dispatch owners.
// exhaustive: its fields are private; only the engine constructs it.
#[derive(Default)]
pub struct VillagerTypeDeclarations {
    entries: Vec<(String, VillagerTypeDefinition)>,
}

impl VillagerTypeDeclarations {
    /// Consume the collected declarations, retaining the engine-recorded owner.
    // hidden: only the game consumes the collected declarations.
    #[doc(hidden)]
    pub fn into_entries(self) -> Vec<(String, VillagerTypeDefinition)> {
        self.entries
    }
}

impl VillagerTypes for VillagerTypeDeclarations {
    fn register(&mut self, definition: VillagerTypeDefinition) {
        let owner = crate::content::crash::current_native_mod()
            .expect("villager type registration inside mod dispatch");
        self.entries.push((owner.into(), definition));
    }
}
