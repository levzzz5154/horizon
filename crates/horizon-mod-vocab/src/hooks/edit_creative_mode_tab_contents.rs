//! The richer creative catalog edit lane. The client-only append hook is unchanged.

pub trait Hooks {
    /// Collect ordered edits for a creative tab on `side`, including Search, Hotbar and Inventory.
    /// Push [`CreativeTabEdit`] values to `edits`. They run in listener order after declarations
    /// and the legacy client appends; Search edits run after category edits. `has_permissions`
    /// is false for the server's player-independent catalog. This edits catalog contents, not
    /// a player's survival inventory or the saved toolbar file.
    #[hook(side = "both", host = "read", contract = "collect", out = "edits")]
    fn edit_creative_mode_tab_contents();
}
