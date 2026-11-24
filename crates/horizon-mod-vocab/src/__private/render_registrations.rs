//! Engine-internal render registrations shared by the native client and its patch.
//!
//! The shared vocabulary library owns the callbacks and their diagnostic guards. Rebuilding
//! renderer resources reads the same registrations without resetting those guards.
//! This is an engine implementation detail, not a mod API.

use crate::{BiomeColorResolver, SectionGeometry, TintLayer};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// One section geometry registration and its persistent diagnostic guards.
// no wire: engine-internal registration, never crosses the wire.
#[non_exhaustive]
pub struct SectionRenderer {
    callback: Arc<dyn SectionGeometry>,
    failed: AtomicBool,
    refused: AtomicBool,
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

    /// The mod callback invoked by meshing workers.
    pub fn callback(&self) -> &Arc<dyn SectionGeometry> {
        &self.callback
    }

    /// Whether the callback panicked and should no longer be invoked.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Mark the callback as panicked; true when this is the first time.
    pub fn mark_failed(&self) -> bool {
        !self.failed.swap(true, Ordering::Relaxed)
    }

    /// Mark a refused batch as logged; true when this is the first time.
    pub fn mark_refused(&self) -> bool {
        !self.refused.swap(true, Ordering::Relaxed)
    }
}

/// One biome color resolver registration and its persistent diagnostic guard.
// no wire: engine-internal registration, never crosses the wire.
#[non_exhaustive]
pub struct RegisteredResolver {
    callback: Arc<dyn BiomeColorResolver>,
    warned: AtomicBool,
}

impl RegisteredResolver {
    /// Wrap a callback with a fresh diagnostic guard.
    pub fn new(callback: Arc<dyn BiomeColorResolver>) -> Self {
        Self {
            callback,
            warned: AtomicBool::new(false),
        }
    }

    /// The mod callback that computes the biome color.
    pub fn callback(&self) -> &Arc<dyn BiomeColorResolver> {
        &self.callback
    }

    /// Whether a panic has been logged and the resolver should no longer be invoked.
    pub fn warned(&self) -> bool {
        self.warned.load(Ordering::Relaxed)
    }

    /// Mark the resolver as panicked; true when this is the first time.
    pub fn mark_warned(&self) -> bool {
        !self.warned.swap(true, Ordering::Relaxed)
    }
}

static SECTION_GEOMETRY: OnceLock<Arc<[SectionRenderer]>> = OnceLock::new();
static BLOCK_TINT_SOURCES: OnceLock<BTreeMap<String, Arc<[TintLayer]>>> = OnceLock::new();
static COLOR_RESOLVERS: OnceLock<Box<[RegisteredResolver]>> = OnceLock::new();

/// Publish section geometry callbacks in registration order before the first resource load.
/// Returns false if already published.
pub fn register_section_geometry(renderers: Vec<Arc<dyn SectionGeometry>>) -> bool {
    published(
        SECTION_GEOMETRY
            .set(renderers.into_iter().map(SectionRenderer::new).collect())
            .is_ok(),
        "section geometry",
    )
}

/// Publish block tint layers by block ID before the first resource load.
/// Returns false if already published.
pub fn register_block_tint_sources(layers: BTreeMap<String, Arc<[TintLayer]>>) -> bool {
    published(BLOCK_TINT_SOURCES.set(layers).is_ok(), "block tint sources")
}

/// Publish biome color resolvers in registration order, before the block tint sources that use
/// their indices. Returns false if already published.
pub fn register_color_resolvers(resolvers: Arc<[Arc<dyn BiomeColorResolver>]>) -> bool {
    published(
        COLOR_RESOLVERS
            .set(
                resolvers
                    .iter()
                    .map(|callback| RegisteredResolver::new(Arc::clone(callback)))
                    .collect(),
            )
            .is_ok(),
        "colour resolvers",
    )
}

/// Log a second publication: the first one stays for the whole process.
fn published(ok: bool, what: &str) -> bool {
    if !ok {
        log::warn!("Horizon: {what} already registered; keeping the first registration");
    }
    ok
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
pub fn color_resolvers() -> Option<&'static [RegisteredResolver]> {
    COLOR_RESOLVERS.get().map(Box::as_ref)
}
