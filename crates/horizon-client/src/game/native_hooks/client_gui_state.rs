use super::*;

impl Game<'_> {
    /// The last window focus state reported by the window system.
    pub(in crate::game) fn window_focused(&self) -> bool {
        self.focused
    }

    /// The same screen identifier provided to screen hooks.
    pub(in crate::game) fn observed_screen_id(&self) -> Option<ScreenId> {
        self.observed_screen_view().map(|view| ScreenId {
            id: view.id.to_owned(),
        })
    }

    /// The live screen's identity, independent of render state or hook subscriptions.
    pub(in crate::game) fn current_screen(&self) -> Option<mod_api::ScreenIdentity> {
        let (id, menu) = self.current_screen_parts()?;
        Some(mod_api::ScreenIdentity {
            id: id.to_owned(),
            menu: menu.map(str::to_owned),
        })
    }
}
