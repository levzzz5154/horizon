//! The `screen_action_pre` hook: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// A semantic controller or keyboard action is about to reach the open screen. Cancel to
    /// skip its built-in movement, activation, return, scrolling or paging. An accepted controller
    /// activation keeps the normal press/release timing; a canceled press cannot click on release.
    ///
    /// `input.focused` is the current registered widget rectangle in GUI pixels, when focus mode has
    /// one. A mod can register its controls through `DrawContext::widget_button`, allow normal
    /// focus navigation, and use the same rectangle to dispatch activation. A replacement without
    /// registered widgets can consume navigation and track its own selection instead.
    ///
    /// Controller actions use the player's active controller settings, including stick deadzones,
    /// repeat and scroll speed. Keyboard actions follow the existing named-key pre-hook. No action
    /// is sent to the screen behind a protected prompt, during loading, or while its on-screen
    /// keyboard or controller binding capture owns input.
    #[hook(
        side = "client",
        contract = "cancel",
        cancellable = true,
        out = "canceled",
        neoforge = "none (Horizon semantic screen input)",
        target = "horizon_client::game::native_hooks::screen_action_pre::Game<'_>::native_screen_action_pre"
    )]
    fn screen_action_pre();
}
