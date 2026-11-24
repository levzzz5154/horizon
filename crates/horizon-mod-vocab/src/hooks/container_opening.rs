//! The `container_opening` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0359

pub trait Hooks {
    /// A player is about to open a menu allowed by `container_opened`. The event
    /// adds `menu_owner`: the acting player's name if the menu uses their ender
    /// chest inventory, and `None` for every other menu, even a regular 9x3 chest.
    /// `kind` is the menu type; `pos` is the optional block position. Server-side,
    /// with the player's state swapped in. Set `cancel` to stop the open and
    /// leave the previous menu intact. A `container_opened` veto skips this hook.
    #[hook(
        side = "server",
        contract = "cancel",
        out = "cancel",
        neoforge = "none (not NeoForge's PlayerContainerEvent.Open: this hook can cancel the opening)",
        cancellable = true
    )]
    fn container_opening();
}
