//! Canonical outfit identities, independent of biome mapping and of protocol ids.

use super::{Mob, render::VILLAGER_TYPES};
use std::{collections::BTreeMap, sync::Arc};

pub const REGISTRY_ID: &str = "minecraft:villager_type";

/// The order is the engine's existing vanilla variant order, not the protocol's order.
const VANILLA_IDS: [&str; VILLAGER_TYPES.len()] = super::render::VILLAGER_TYPE_IDS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VillagerType {
    Vanilla(u8),
    Custom(Arc<str>),
}

impl VillagerType {
    pub fn vanilla(id: &str) -> Option<Self> {
        let id = id.strip_prefix("minecraft:").unwrap_or(id);
        VILLAGER_TYPES
            .iter()
            .position(|&name| name == id)
            .map(|i| Self::Vanilla(i as u8))
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Vanilla(slot) => vanilla_id(*slot),
            Self::Custom(id) => id,
        }
    }

    pub fn apply(self, mob: &mut Mob) {
        match self {
            Self::Vanilla(slot) => {
                mob.extra.variant = slot;
                mob.extra.villager_type = None;
            }
            Self::Custom(id) => {
                mob.extra.variant = 0;
                mob.extra.villager_type = Some(id);
            }
        }
    }

    pub fn of(mob: &Mob) -> Self {
        mob.extra.villager_type.as_ref().map_or_else(
            || Self::Vanilla(mob.extra.variant),
            |id| Self::Custom(id.clone()),
        )
    }
}

pub fn vanilla_id(slot: u8) -> &'static str {
    VANILLA_IDS[usize::from(slot) % VANILLA_IDS.len()]
}

pub fn type_id(mob: &Mob) -> &str {
    mob.extra
        .villager_type
        .as_deref()
        .unwrap_or_else(|| vanilla_id(mob.extra.variant))
}

/// Keep a saved name even if its declaring mod is absent. Never turn malformed strings into ids.
pub fn set_type(mob: &mut Mob, id: &str) {
    if let Some(kind) = VillagerType::vanilla(id) {
        kind.apply(mob);
    } else if horizon_core::identifier::Identifier::parse_namespaced(id).is_some() {
        VillagerType::Custom(Arc::from(id)).apply(mob);
    }
}

#[derive(Debug)]
struct Outfit {
    villager: Arc<str>,
    zombie: Arc<str>,
}

/// World-local declared identities; vanilla entries are implicit and retain their original ids.
#[derive(Debug, Default)]
pub struct Registry {
    custom: BTreeMap<Arc<str>, Outfit>,
}
impl Registry {
    pub fn prepare(
        declarations: &[(String, mod_api::VillagerTypeDefinition)],
    ) -> Result<Self, String> {
        let mut out = Self::default();
        for (owner, definition) in declarations {
            let id = &definition.key;
            let invalid = |reason: &str| format!("mod {owner}, villager type {id}: {reason}");
            if id.namespace().is_empty() || id.path().is_empty() {
                return Err(invalid("expected a nonempty namespace and path"));
            }
            if id.namespace() == "minecraft" {
                return Err(invalid("the minecraft namespace is reserved"));
            }
            let name = id.to_string();
            if out.custom.contains_key(name.as_str()) {
                return Err(invalid("duplicate declaration"));
            }
            let sprite = |texture: &mod_api::Identifier| -> Result<Arc<str>, String> {
                if !texture.path().starts_with("entity/") || texture.path().ends_with(".png") {
                    return Err(format!(
                        "mod {owner}, villager type {id}: texture {texture} must start with entity/ and omit .png"
                    ));
                }
                Ok(if texture.namespace() == "minecraft" {
                    Arc::from(texture.path())
                } else {
                    Arc::from(texture.to_string())
                })
            };
            out.custom.insert(
                Arc::from(name),
                Outfit {
                    villager: sprite(&definition.villager_texture)?,
                    zombie: sprite(&definition.zombie_villager_texture)?,
                },
            );
        }
        Ok(out)
    }

    pub fn resolve(&self, id: &str) -> Option<VillagerType> {
        VillagerType::vanilla(id).or_else(|| {
            self.custom
                .get_key_value(id)
                .map(|(id, _)| VillagerType::Custom(id.clone()))
        })
    }

    pub fn contains(&self, id: &str) -> bool {
        VillagerType::vanilla(id).is_some() || self.custom.contains_key(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        VANILLA_IDS
            .into_iter()
            .chain(self.custom.keys().map(AsRef::as_ref))
    }

    pub fn outfits(&self) -> impl Iterator<Item = (&str, [&str; 2])> {
        self.custom.iter().map(|(id, outfit)| {
            (
                id.as_ref(),
                [outfit.villager.as_ref(), outfit.zombie.as_ref()],
            )
        })
    }
}
