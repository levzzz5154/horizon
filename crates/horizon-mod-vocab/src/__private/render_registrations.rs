//! Engine-internal render registrations shared by the native client and its patch.
//!
//! The shared vocabulary library owns the callbacks and their diagnostic guards. Rebuilding
//! renderer resources reads the same registrations without resetting those guards.
//! This is an engine implementation detail, not a mod API.

use crate::{BiomeColorResolver, SectionGeometry, TintLayer};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock, atomic::AtomicBool},
};

/// One section geometry registration and its persistent diagnostic guards.
pub struct SectionRenderer {
    /// The mod callback invoked by meshing workers.
    pub callback: Arc<dyn SectionGeometry>,
    /// Whether the callback panicked and should no longer be invoked.
    pub failed: AtomicBool,
    /// Whether a refused batch from this callback has already been logged.
    pub refused: AtomicBool,
}

impl SectionRenderer {
    /// Wrap a callback with fresh diagnostic guards.
    pub fn new(callback: Arc<dyn SectionGeometry>) -> Self {
        Self {
            callback,
            failed: AtomicBool::new(false),
            refused: AtomicBool::new(false),
        }
    }
}

/// One biome color resolver registration and its persistent diagnostic guard.
pub struct ColorResolver {
    /// The mod callback that computes the biome color.
    pub callback: Arc<dyn BiomeColorResolver>,
    /// Whether a panic has been logged and the resolver should no longer be invoked.
    pub warned: AtomicBool,
}

static SECTION_GEOMETRY: OnceLock<Arc<[SectionRenderer]>> = OnceLock::new();
static BLOCK_TINT_SOURCES: OnceLock<BTreeMap<String, Arc<[TintLayer]>>> = OnceLock::new();
static COLOR_RESOLVERS: OnceLock<Box<[ColorResolver]>> = OnceLock::new();

/// Publish section geometry callbacks in registration order before the first resource load.
/// Returns false if already published.
pub fn register_section_geometry(renderers: Vec<Arc<dyn SectionGeometry>>) -> bool {
    SECTION_GEOMETRY
        .set(renderers.into_iter().map(SectionRenderer::new).collect())
        .is_ok()
}

/// Publish block tint layers by block ID before the first resource load.
/// Returns false if already published.
pub fn register_block_tint_sources(layers: BTreeMap<String, Arc<[TintLayer]>>) -> bool {
    BLOCK_TINT_SOURCES.set(layers).is_ok()
}

/// Publish biome color resolvers in registration order, before the block tint sources that use
/// their indices. Returns false if already published.
pub fn register_color_resolvers(resolvers: Arc<[Arc<dyn BiomeColorResolver>]>) -> bool {
    COLOR_RESOLVERS
        .set(
            resolvers
                .iter()
                .map(|callback| ColorResolver {
                    callback: Arc::clone(callback),
                    warned: AtomicBool::new(false),
                })
                .collect(),
        )
        .is_ok()
}

/// Borrow the section geometry registrations and their guards, or `None` before publication.
pub fn section_geometry() -> Option<&'static Arc<[SectionRenderer]>> {
    SECTION_GEOMETRY.get()
}

/// Borrow the block tint registrations, or `None` before publication.
pub fn block_tint_sources() -> Option<&'static BTreeMap<String, Arc<[TintLayer]>>> {
    BLOCK_TINT_SOURCES.get()
}

/// Borrow the biome color resolver registrations and their guards, or `None` before publication.
pub fn color_resolvers() -> Option<&'static [ColorResolver]> {
    COLOR_RESOLVERS.get().map(Box::as_ref)
}
