use super::super::Game;

impl Game<'_> {
    /// A menu approved by `container_opened` is about to replace the player's open menu.
    pub(in crate::game) fn native_container_opening(
        &mut self,
        player: u128,
        name: &str,
        kind: &str,
        pos: Option<[i32; 3]>,
        menu_owner: Option<&str>,
        cancel: &mut bool,
    ) {
        self.fire_container_opening(player, name, kind, pos, menu_owner, cancel);
    }
}
