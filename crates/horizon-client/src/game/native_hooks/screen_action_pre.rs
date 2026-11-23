use super::{Game, ScreenView};
use mod_api::{__private::listened, HookId, ScreenAction, ScreenActionInput, ScreenActionSource};

impl Game<'_> {
    pub(in crate::game) fn native_screen_action_pre(
        &self,
        screen: &ScreenView<'_>,
        input: &ScreenActionInput,
        canceled: &mut bool,
    ) {
        self.fire_screen_action_pre(screen, input, canceled);
    }

    pub(in crate::game) fn screen_action_canceled(
        &self,
        action: ScreenAction,
        source: ScreenActionSource,
        focused: impl FnOnce() -> Option<[f32; 4]>,
    ) -> bool {
        if !listened(HookId::ScreenActionPre)
            || self.prompt_open()
            || self.title.as_ref().is_some_and(|title| {
                matches!(title.mode, horizon_client::title::Mode::MissingMods { .. })
            })
        {
            return false;
        }
        let Some(screen) = self.screen_view_now() else {
            return false;
        };
        let input = ScreenActionInput {
            action,
            source,
            focused: focused(),
        };
        let mut canceled = false;
        self.native_screen_action_pre(&screen, &input, &mut canceled);
        canceled
    }
}
