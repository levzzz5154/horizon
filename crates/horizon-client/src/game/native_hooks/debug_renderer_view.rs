use super::*;

impl Game<'_> {
    pub(in crate::game) fn native_debug_renderer_view(
        &self,
        id: &str,
        view: &mod_api::WorldRenderView,
        draw: &mut horizon_client::world_geometry::WorldGeometry,
        entities: &[mod_api::DebugEntityHitbox],
        control: &mod_api::DebugRendererControl,
    ) {
        self.fire_debug_renderer_view(id, view, draw, entities, control);
    }
}
