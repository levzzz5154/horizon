//! The native celestial sky hook: declaration read by the mod API generator.

pub trait Hooks {
    /// Add textured triangles to the main view's camera-relative sky, before the celestial bodies.
    #[notes = r#"## Celestial sky geometry

Native mods can inject into `mod_render::render::celestial_sky` to add a backdrop such as a nebula
and leave the vanilla sun, moon and stars intact. This is the visible Overworld-style sky of the
main world view only. The layer draws after the disc, sunrise and custom resource-pack sky layers,
before the sun, moon, stars and all terrain. It is not called when the sky is hidden, on the title
screen, for the End sky, or when a shader pack owns the sky or its celestial pass.

The `CelestialSky` argument has the actual camera-relative `view_proj` (including view effects
and jitter) and `star_angle_degrees`, the vanilla star X angle even when the stars are too dim to
draw. Ordinary positions are camera-relative world offsets, without subtracting the camera.
To follow the stars, first rotate the offsets by `star_rotation()` (`Y(-90°) × X(star_angle)`).

`draw_triangles(sprite, vertices)` takes a triangle list in either winding, with each UV in 0..1
across one `AtlasRegion`. Resolve it from `gui_sprite` at `texture_atlas_stitched` and refresh it
after a GUI atlas reload. RGB tint is unlit vanilla gamma (0..16, 1 at paper white); alpha is
coverage (0..1). The engine multiplies the sampled texture by the interpolated tint, converts to
the frame's storage encoding, and blends without writing depth. Float frames keep light above
white; the backdrop does not get the stock sun's HDR lift.

Submissions share a 4,096-triangle budget across native mods for the frame. Incomplete triangles,
nonfinite or out-of-range coordinates, UVs, colors and invalid atlas regions are refused, without
partially drawing a batch. Coordinates must be in -512..512 on each axis. There is no GPU handle
or WebAssembly wire form.

`python3 tools/run_world_views.py <output-dir> --celestial` draws the stock and modded sky through
the main renderer and compares the pixels at two camera positions, after rotating the camera
and stars, and where opaque terrain occludes the layer."#]
    #[hook(
        side = "client",
        contract = "listen",
        target = "mod_render::render::celestial_sky",
        neoforge = "none (Horizon-only; NeoForge has no counterpart)"
    )]
    fn celestial_sky();
}
