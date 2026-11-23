//! The `modify_baking_result` hook: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// Inspect or edit the candidate block and standalone geometry after baking and before
    /// publication, once per client resource load that reaches this stage. The first listener
    /// sees the fresh bake; later listeners see accepted edits. Client-native, on the resource
    /// loader, without a live client host. A candidate may still fail to publish.
    ///
    /// Not on wasm: the model catalog is a short-lived borrow of native renderer data.
    #[notes = r#"## Modify baked block models

`modify_baking_result` borrows the candidate after the current pack stack has baked block and
standalone models, including native-loader faces and blockstate rotations/UV locking. This native
client hook runs on the resource loader at startup and again for each rebake, before connected
textures are derived and before publication. It is not a reload-complete notification: a later
load stage can fail. Use `model_baking_completed` to resolve published `ModelRef`s.

Use `event.models.model(key)` for a named transformed model or registered standalone id, or
`event.models.visit_block_state(block, properties, visit)` to see all alternatives of that
exact state. Quad positions and sprite-relative UVs are already baked, and views borrow them and
their sprite names only for the callback. `edit_quads` can keep, remove, or replace the geometry
of each quad in a named bake, retaining its original sprite, tint and lighting attributes. A
model slot is shared by every state or standalone reference that selects the identical bake;
editing it does not make a state-local copy. Later listeners see accepted edits, but a reload
starts with fresh geometry from that pack stack. There is no drawing or world access in this
hook, no delegate retained at draw time, and no item-stack model, ModelData or Wasm API here."#]
    #[hook(
        side = "client",
        contract = "listen",
        context = "registration",
        target = "horizon_client_render::models::catalog::baking_result::native_modify_baking_result",
        neoforge = "ModelEvent.ModifyBakingResult (block and standalone bake-time subset)"
    )]
    fn modify_baking_result();
}
