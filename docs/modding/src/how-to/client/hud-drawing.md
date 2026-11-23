# Drawing on the HUD and in screens

A HUD layer, a screen or a tooltip hook draws through the `DrawContext` it is handed, in GUI pixels. Each call
is one of vanilla's `GuiGraphics` calls: `fill`, `text`, `sprite`, `item_stack` and the others. `gui_paint`
takes a buffer of `GuiDraw` ops for the same calls, with `Push` / `Pop` for the pose. Native and wasm mods use
the same methods and the same ops.

## Gradient fills

`DrawContext::fill_gradient(rect, top, bottom)` is vanilla's `GuiGraphics.fillGradient`, the call screen
backgrounds and tooltips are drawn with. `rect` is `[x, y, w, h]` in GUI pixels, as `fill` takes it. `top` and
`bottom` are straight-alpha ARGB colours (`0xAARRGGBB`): `top` at `y`, blending to `bottom` at `y + h`.
With positive extents these are the visual top and bottom edges. Alpha blends with the colours, so a gradient can
fade out. The current HUD shader discards interpolated alpha below 0.004, not just exact zero; near-transparent
fragments can therefore differ from vanilla's exact-zero discard.

```rust,ignore
fn gui_render_post(&self, _: &dyn ClientHost, event: &mut GuiRenderPostEvent<'_>) {
    // A panel shaded from dark purple at the top to black at the bottom.
    event.batch.fill_gradient([20.0, 20.0, 120.0, 60.0], 0xF010_0010, 0xF000_0000);
    // A shade that fades out downwards.
    event.batch.fill_gradient([20.0, 80.0, 120.0, 10.0], 0x8000_0000, 0x0000_0000);
}
```

In a `gui_paint` buffer the same call is the op `GuiDraw::FillGradient { x0, y0, x1, y1, top, bottom }`, with
vanilla's corner arguments: `top` stays on the `y0` edge and `bottom` on the `y1` edge.

- **Pose and clip.** A gradient follows the current pose (a `gui_paint` buffer's `Push`) and the hook call's
  clip (`push_clip`), as `fill` does. A clip that cuts the rectangle keeps the colour ramp: the cut edge has the
  colour that was there.
- **Empty or inverted.** Zero width or height draws nothing, as does inversion on exactly one axis.
  Inverting both axes (`w < 0` and `h < 0`, or `x1 < x0` and `y1 < y0`) draws sorted bounds with the
  colours flipped: the first colour stays on its input y edge, now the lower edge. This preserves vanilla's
  visible winding and colour placement. A `fill_gradient` rectangle with a NaN or infinite coordinate draws nothing.
  Zero-area calls are culled here; vanilla can retain their layering bounds without drawing pixels.
- **Cost.** One quad on the GUI batch, as `fill`; nothing is drawn or kept when a mod doesn't call it.
- **Wire layout.** `GuiDraw` gained the `FillGradient` variant: rebuild wasm mods against the new SDK.

## Branded loading overlay (native client)

Implement `ModHooks::loading_overlay_style` to change the branded startup,
resource-pack reload, or settings preview. `LoadingOverlayStyleEvent` carries the
phase, normalized progress, and the mutable style for that frame. The hook is
not called for the unbranded bootstrap or world-joining screens.

```rust,ignore
fn loading_overlay_style(&self, event: &mut LoadingOverlayStyleEvent<'_>) {
    event.style.background = 0xFF10_1420;
    event.style.bar_background = Some(0xFF1A_2234);
    event.style.bar_fill = 0xFF85_D7FF;
    event.style.bar_border = 0xFFB4_D8F5;
    event.style.logo = LoadingOverlayLogo::ChannelDeltas {
        darken: 0xFF00_0010,
        lighten: 0xFF50_759B,
    };
}
```

Colors are straight-alpha ARGB. The two logo sprites keep their original
positions and UVs. `ChannelDeltas` subtracts RGB first, then adds RGB; each pass
is scaled by the texel and the supplied alpha. `Tint` uses ordinary alpha
blending, `Hidden` omits the logo, and `Original` leaves it unchanged.

From an in-game settings callback, call `host.preview_loading_overlay(ms)` to
show the same overlay with synthetic progress. It returns `false` when a preview
cannot start, for example when a real reload is underway. Its lifetime is
bounded; Escape or the controller's Back action dismisses it and returns to the
underlying settings screen. A preview does not reload resources or save settings.
