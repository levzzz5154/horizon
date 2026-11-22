//! Client window focus and observed screen reads.

pub trait Notes {
    #[notes = r#"## Client window and screen state

`ClientHost::window_focused()` reports the last focus state from the window
system. `ClientHost::observed_screen_id()` returns the same ID screen hooks see,
including overlays. `None` means no view is available: there may be no open
screen or no renderer yet. It does not mean the client is outside a world.
Use `ClientHost::current_screen()` for the topmost screen's ID and menu type
without a renderer, for example to gate an item tooltip on the enchanting table.
Read again after a screen or focus change."#]
    fn window_focused();
}
