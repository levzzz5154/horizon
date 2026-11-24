//! The `sprite_source_read` hooks: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// An entry of an atlas definition (including the built-in `minecraft:blocks` and
    /// `minecraft:items` atlases as well as mod-registered atlases) whose `type` a mod registered
    /// (`register_sprite_sources`, vanilla's `SpriteSource.run`), asked at the entry's place in the
    /// definition's `sources` with a scoped [`ResourceReader`] (`read`, `read_stack`, `list`), each
    /// time the atlas is loaded or reloaded, on the resource loader. `config` is the entry's JSON
    /// object converted to a `Tag`, `atlas` the atlas being listed, and `resources` reads the
    /// active client resource packs for the duration of the callback. Push `SpriteSourceOutput`s
    /// (`Texture`, `Pixels`, `Remove`) to `output` only for a source your mod owns; they apply in
    /// order once every mod has answered.
    #[notes = r#"## Built-in atlas sprite sources and scoped `ResourceReader`

Mod-registered sprite sources (`register_sprite_sources`) run for the game's built-in
`minecraft:blocks` and `minecraft:items` atlases as well as mod-registered atlases
(`register_texture_atlases`). Alongside `sprite_source`, `sprite_source_read` passes a scoped
`ResourceReader` (`read`, `read_stack`, `list`) over the active client resource packs for the
duration of the callback so a source can inspect textures, metadata, or custom pack files and emit
`SpriteSourceOutput::Texture`, `SpriteSourceOutput::Pixels`, or `SpriteSourceOutput::Remove`."#]
    #[hook(
        side = "client",
        contract = "collect",
        out = "output",
        context = "registration",
        neoforge = "none (vanilla SpriteSource.run with ResourceManager for blocks, items and mod atlases)",
        target = "horizon_client_render::mod_sprite_sources::native_sprite_source_read"
    )]
    fn sprite_source_read();
}
