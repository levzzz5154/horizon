//! What a mod's own atlas sprite source answers (`sprite_source`).
use crate::Identifier;

/// One step a mod's sprite source takes on the atlas it is listed in, as vanilla's
/// `SpriteSource.Output` takes them. `Hooks::sprite_source` pushes them in order, and they apply
/// under the built-in sources' rule: a sprite of an id listed already replaces the earlier one.
#[derive(Clone, Debug, PartialEq, Eq)]
// exhaustive: mods construct outputs and the game matches every kind
#[derive(horizon_mod_api_macros::Wire)]
pub enum SpriteSourceOutput {
    /// `add(Identifier, Resource)`: sprite `sprite` is the texture `texture`
    /// (`assets/<namespace>/textures/<path>.png`), with its `.mcmeta`. A texture the resources
    /// lack is logged and skipped.
    Texture {
        /// The sprite's id in the atlas (`mymod:gem`).
        sprite: Identifier,
        /// The texture it is (`mymod:item/gem`).
        texture: Identifier,
    },
    /// `add(Identifier, DiscardableLoader)`: sprite `sprite` is these pixels, `width` by `height`,
    /// rows top first, four bytes (red, green, blue, alpha) a pixel. Pixels whose length isn't
    /// `width * height * 4`, or a side over the largest a sprite can have, are logged and skipped.
    Pixels {
        /// The sprite's id in the atlas.
        sprite: Identifier,
        /// Its width in pixels.
        width: u32,
        /// Its height in pixels.
        height: u32,
        /// Its RGBA bytes.
        rgba: Vec<u8>,
    },
    /// `removeAll`: drops sprite `sprite` from what the sources before listed, read in the event's
    /// `listed`. An id not listed changes nothing.
    Remove {
        /// The sprite to drop.
        sprite: Identifier,
    },
}

/// Scoped read-only view of the client's active resource packs handed to a mod's sprite source
/// during atlas loading (`sprite_source_read`). Every identifier maps to `assets/<namespace>/<path>`
/// (for example, `mymod:textures/item/gem.png` reads `assets/mymod/textures/item/gem.png`).
///
/// Valid only for the duration of the callback; on wasm it is backed by a scoped host frame that is
/// invalidated when the hook returns.
pub trait ResourceReader {
    /// Winning bytes of `assets/<namespace>/<path>`, or `None` if absent or unreadable.
    fn read(&self, id: &Identifier) -> Option<Vec<u8>>;
    /// Every pack's bytes for `assets/<namespace>/<path>`, lowest priority first (matching
    /// vanilla's `ResourceManager.getResourceStack`). Empty when absent.
    fn read_stack(&self, id: &Identifier) -> Vec<Vec<u8>>;
    /// Asset identifiers under `assets/<namespace>/<path>` whose path starts with `prefix` and ends
    /// with `suffix`, in deterministic sorted order.
    fn list(&self, prefix: &str, suffix: &str) -> Vec<Identifier>;
}
