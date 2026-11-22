//! Prepare the declared registry once per mod load and bind biome selectors per world.
use super::Game;
use horizon_client::mobs::villager_types::{Registry, VillagerType};
use mod_api::__private::game::RegistrationContextNew;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

type Declarations = [(String, mod_api::VillagerTypeDefinition)];
type BiomeTags = HashMap<String, HashSet<String>>;

impl Game<'_> {
    fn villager_type_declarations(&self) -> &Declarations {
        self.mods.villager_type_declarations.get_or_init(|| {
            let mut registrar = mod_api::VillagerTypeDeclarations::default();
            Self::native_register_villager_types(
                &mod_api::RegistrationContext::new(),
                &mut registrar,
            );
            registrar.into_entries()
        })
    }

    pub(in crate::game) fn declared_villager_types(&self) -> Result<&Arc<Registry>, &str> {
        self.mods
            .villager_types
            .get_or_init(|| Registry::prepare(self.villager_type_declarations()).map(Arc::new))
            .as_ref()
            .map_err(String::as_str)
    }

    pub(in crate::game) fn install_client_villager_types(&self) -> super::Result<()> {
        let table = self.declared_villager_types().map_err(str::to_owned)?;
        horizon_client::mobs::render::villager_client::install_types(table);
        Ok(())
    }

    pub(super) fn bind_villager_types(
        &self,
        tags: Option<&BiomeTags>,
    ) -> super::Result<(Arc<Registry>, BTreeMap<String, VillagerType>)> {
        let registry = self.declared_villager_types().map_err(str::to_owned)?;
        let biomes = self
            .world_biomes
            .as_ref()
            .unwrap_or_else(|| self.res.biomes());
        let outfits = bind(self.villager_type_declarations(), registry, tags, |id| {
            biomes.contains(id)
        })?;
        Ok((registry.clone(), outfits))
    }
}

fn bind(
    declarations: &Declarations,
    registry: &Registry,
    tags: Option<&BiomeTags>,
    knows_biome: impl Fn(&str) -> bool,
) -> Result<BTreeMap<String, VillagerType>, String> {
    let mut outfits = BTreeMap::<String, VillagerType>::new();
    for (owner, definition) in declarations {
        let id = definition.key.to_string();
        let kind = registry.resolve(&id).expect("validated villager type");
        let invalid = |reason: &str| format!("mod {owner}, villager type {id}: {reason}");
        let mut insert = |biome: &str| {
            match outfits.entry(biome.to_owned()) {
                std::collections::btree_map::Entry::Occupied(entry) if entry.get() != &kind => {
                    return Err(invalid(&format!(
                        "biome {biome} also selects {}",
                        entry.get().id()
                    )));
                }
                std::collections::btree_map::Entry::Occupied(_) => {}
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(kind.clone());
                }
            }
            Ok(())
        };
        for biome in &definition.biomes {
            let biome = biome.to_string();
            if !knows_biome(&biome) {
                return Err(invalid(&format!("unknown biome {biome}")));
            }
            insert(&biome)?;
        }
        for tag in &definition.biome_tags {
            let tag = tag.to_string();
            let members = tags
                .and_then(|tags| tags.get(&tag))
                .ok_or_else(|| invalid(&format!("unknown biome tag #{tag}")))?;
            // A tag can contain optional entries not present in this world.
            let mut members: Vec<_> = members.iter().filter(|biome| knows_biome(biome)).collect();
            members.sort_unstable();
            for biome in members {
                insert(biome)?;
            }
        }
    }
    Ok(outfits)
}

#[cfg(test)]
#[path = "villager_types/tests.rs"]
mod tests;
