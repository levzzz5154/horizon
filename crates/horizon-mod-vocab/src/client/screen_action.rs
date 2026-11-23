//! Semantic input shared by controller and keyboard screen focus.

/// How a screen action was initiated, independent of controller hardware and mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum ScreenActionSource {
    /// The active gamepad, including virtual controller input.
    Controller,
    /// The screen's keyboard navigation and activation keys.
    Keyboard,
}

/// The direction of a focus or pointer step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum ScreenDirection {
    /// Towards the top of the screen.
    Up,
    /// Towards the bottom of the screen.
    Down,
    /// Towards the left edge of the screen.
    Left,
    /// Towards the right edge of the screen.
    Right,
}

/// What the player asked the open screen to do.
#[derive(Clone, Copy, Debug, PartialEq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum ScreenAction {
    /// Move in the indicated direction using the screen's focus or pointer rules.
    Navigate(ScreenDirection),
    /// Move along the registered tab order (Tab or Shift+Tab).
    FocusNext {
        /// True for Tab and false for Shift+Tab.
        forward: bool,
    },
    /// Confirm the current focus or pointer target.
    Activate,
    /// Return to the parent screen or close the current one.
    Back,
    /// Right-stick scroll in wheel lines, after controller speed but before mouse-wheel settings.
    Scroll {
        /// Horizontal lines; negative scrolls right.
        x: f32,
        /// Vertical lines; positive scrolls up.
        y: f32,
    },
    /// Next/previous page or tab, as the active screen treats the bumpers.
    Page {
        /// True for the next page or tab.
        forward: bool,
    },
}

/// The action and focus visible to a screen input listener.
#[derive(Clone, Copy, Debug, PartialEq, horizon_mod_api_macros::Wire)]
pub struct ScreenActionInput {
    /// The requested action.
    pub action: ScreenAction,
    /// The input family that requested it.
    pub source: ScreenActionSource,
    /// The focused registered widget, in GUI pixels `[x, y, width, height]`.
    pub focused: Option<[f32; 4]>,
}
