use super::*;

impl Game<'_> {
    pub(in crate::game) fn native_edit_creative_mode_tab_contents(
        &self,
        tab: &Identifier,
        side: LogicalSide,
        has_permissions: bool,
        edits: &mut Vec<CreativeTabEdit>,
    ) {
        self.fire_edit_creative_mode_tab_contents(tab, side, has_permissions, edits);
    }
}
