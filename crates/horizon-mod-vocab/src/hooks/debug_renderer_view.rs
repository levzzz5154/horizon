//! The native pre-draw view for the built-in hitboxes and active mod debug renderers.

pub trait Hooks {
    /// Draw an active debug renderer for a world view, after `submit_custom_geometry`.
    /// `id` is the mod's registered renderer id or `ENTITY_HITBOX_RENDERER` (F3+B).
    /// `entities` is the same interpolated set used by the built-in hitbox renderer, including
    /// when F3+B is off and only a custom renderer is enabled. The built-in call precedes its
    /// lines; `control.cancel_builtin()` skips those lines without changing debug selection,
    /// other renderers, or submitted geometry. Every listener still runs; cancellation is
    /// one-way. `draw` is the same world draw context as `submit_custom_geometry`.
    /// Not called on title or connection screens. Client-side.
    /// Not on wasm: the hitbox projection and draw decision are borrowed native views.
    #[notes = r#"## Replacing the built-in entity hitboxes

Native `Hooks::debug_renderer_view` sees the interpolated hitbox set for each active
debug renderer. For `ENTITY_HITBOX_RENDERER` (F3+B), call `event.control.cancel_builtin()`
before drawing replacement lines with `event.draw`. Other renderers and submitted geometry
remain, and the F3+B entry stays selected. Each active custom renderer sees the same
hitboxes even if F3+B is off. The supplied client host exposes `local_player` and level
block reads. Narrow the player with `host.living(player.entity())` for held items and
blocking. The entity handles also support live distance and buffered attack reach reads."#]
    #[hook(side = "client", contract = "listen")]
    fn debug_renderer_view();
}
