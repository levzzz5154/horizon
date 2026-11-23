//! Startup callbacks for random-spread candidates, bound to the actual level and sets as its
//! generator is built. Workers never read the current game or a reloadable global registry.
use super::start::StructureSet;
use crate::worldgen::DecodeError;
use horizon_core::position::ChunkPos;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

/// The inputs one native selector sees.
pub struct StructureCell<'a> {
    pub level: &'a str,
    pub generator: &'a str,
    pub structure_set: &'a str,
    pub seed: i64,
    pub cell: [i32; 2],
    pub spacing: i32,
    pub separation: i32,
    pub triangular: bool,
    pub salt: i32,
}

/// A selector's proposed replacement for vanilla's candidate.
pub enum Candidate {
    Vanilla,
    Chunk(ChunkPos),
    Skip,
}

type Selector = dyn Fn(&StructureCell<'_>) -> Candidate + Send + Sync;
/// Startup declarations keyed by their namespaced selector id.
pub type Selectors = BTreeMap<String, Arc<Selector>>;
static SELECTORS: OnceLock<Selectors> = OnceLock::new();

/// Publish the native mods' declarations once, before any world opens.
pub fn install(selectors: Selectors) -> Result<(), Selectors> {
    SELECTORS.set(selectors)
}

/// The field of a level stem's generator that opts its sets in.
pub const FIELD: &str = "horizon:structure_candidates";

pub(super) struct LevelSelectors {
    pub level: Box<str>,
    pub generator: Box<str>,
    pub sets: Selectors,
}

impl std::fmt::Debug for LevelSelectors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LevelSelectors")
            .field("level", &self.level)
            .field("generator", &self.generator)
            .field("sets", &self.sets.keys())
            .finish()
    }
}

impl LevelSelectors {
    pub fn bind(
        level: &str,
        generator: &Value,
        sets: &[StructureSet],
    ) -> Result<Option<Box<Self>>, DecodeError> {
        let Some(config) = generator.get(FIELD) else {
            return Ok(None);
        };
        let error =
            |reason: &str| DecodeError::Registry(format!("level {level} {FIELD}: {reason}"));
        let config = config
            .as_object()
            .ok_or_else(|| error("expected an object"))?;
        if config.is_empty() {
            return Ok(None);
        }
        let mut bound = Selectors::new();
        for (set, selector) in config {
            let Some(placement) = sets.iter().find(|entry| entry.name == *set) else {
                return Err(error(&format!("unknown structure set {set}")));
            };
            if placement.spacing().is_none() {
                return Err(error(&format!("structure set {set} is not random-spread")));
            }
            let selector = selector
                .as_str()
                .ok_or_else(|| error(&format!("expected a selector id for structure set {set}")))?;
            let callback = SELECTORS
                .get()
                .and_then(|selectors| selectors.get(selector));
            let Some(callback) = callback else {
                return Err(error(&format!(
                    "unknown selector {selector} for structure set {set}"
                )));
            };
            bound.insert(set.clone(), callback.clone());
        }
        let generator = generator["type"]
            .as_str()
            .ok_or_else(|| error("expected a generator type"))?;
        Ok(Some(Box::new(Self {
            level: level.into(),
            generator: generator.into(),
            sets: bound,
        })))
    }
}
