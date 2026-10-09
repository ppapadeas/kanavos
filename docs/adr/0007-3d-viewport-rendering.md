# ADR-0007: How the 3D viewport is rendered

- Status: accepted
- Date: 2026-10-10

## Context

The 3D view must stay interactive (60 frames per second) for a detailed house of a few thousand elements and
a few hundred thousand triangles, on integrated graphics as well as discrete cards. It must pick elements under
the cursor, cut the model with a section box, show selection, hover and ghosted storeys, and read as a calm,
drawing-like model view rather than a game scene. Models sit at projected ΕΓΣΑ87 coordinates (hundreds of
kilometres from the origin), which `f32` cannot hold to millimetres. The interface is egui (ADR-0004), but the
renderer should not depend on it so that the command-line tools can render images too.

## Decision

`kanavos-render` renders with wgpu into **its own offscreen texture** and returns it; the interface shows it as an
image (with egui: `egui_wgpu::Renderer::register_native_texture`). The crate has no egui dependency.

- **Precision.** Mesh positions are `f32` relative to a scene origin chosen near the model; the subtraction
  happens in `f64`. Cameras, picking results and clip planes are `f64` world coordinates.
- **Depth.** Reverse-Z with a 32-bit float depth buffer and an infinite far plane for perspective, which keeps
  depth precision roughly constant at every distance. Parallel projection fits its depth range to the scene.
- **Batching.** Objects are packed into chunks of up to 256 objects, each with one vertex, index and edge buffer,
  so a house draws in a few dozen calls and editing one element rebuilds one chunk. Chunks outside the view
  frustum are skipped, which also makes picks cheap (their frustum covers a few pixels). Mesh preparation for a
  whole scene runs on all cores. Each vertex carries a dense
  object slot; per-object state (hidden, ghosted, selected, hovered) lives in a small integer texture, so
  selection and visibility changes never touch geometry. Transparent meshes share one buffer with a draw range
  each and are sorted back to front every frame.
- **Edges.** Feature edges (creases above an angle, open boundaries) and silhouette candidates (edges between
  smooth faces, drawn only where one side faces the viewer) are computed once per mesh on the CPU and drawn as
  instanced screen-space quads of constant pixel width with multisampling. Geometric edges were preferred over
  post-process edge detection: they are crisp at any resolution and stay correct under clipping.
- **Lighting.** Sky/ground ambient, a sun with a cached shadow map (re-rendered only when geometry, visibility,
  clipping or the sun change), a soft viewer fill light, and screen-space ambient occlusion following the
  published "alchemy" obscurance estimator (McGuire et al., HPG 2011/2012), blurred with a depth-aware filter.
- **Picking.** A pick renders object slots and depth into a 7 × 7 target using a projection narrowed to the
  pixels around the cursor, then reads it back asynchronously (a blocking variant waits for the GPU). The nearest
  hit in the window wins, which makes thin objects easy to pick, and the depth gives the world point for
  orbit-about-cursor and zoom-to-cursor.
- **Section box.** Up to six clip planes are tested in the fragment shaders. Back faces of closed solids are drawn
  in a flat cut colour, so cut walls read as solid without computing cap polygons, and the contour of the cut is
  outlined with lines computed from triangle–plane intersections (only for objects whose bounds straddle a plane,
  and only when the planes, the geometry or the visibility change).
- **Robustness.** Pipelines are created inside a validation error scope: when the scene shaders are rejected the
  renderer returns an error the interface can show; when only ambient occlusion is unsupported it is switched off.

## Consequences

- One renderer instance serves one viewport; a second 3D viewport needs its own instance (and its own copy of the
  meshes on the GPU).
- Picking costs one extra vertex pass over the scene; hover picking every frame is affordable for houses.
- The cut colour is a visual approximation: cut faces are not real polygons, so they cannot be hatched by
  material. Section and plan drawings come from `kanavos-views`, not from this renderer.
- Shadows use one map fitted to the whole visible scene; very large sites get softer shadows. Fitting the map to
  the view frustum (cascades) can be added behind the same interface.
- Measured on an integrated GPU (AMD Radeon 780M, Vulkan) on an otherwise idle machine: 5,000 objects / 460,000
  triangles at 1920 × 1080 with MSAA 4×, shadows, ambient occlusion and edges render in about 10 ms per frame;
  preparing the scene takes about 30 ms (130 ms on one core), editing one object about 1 ms, and a blocking pick
  about 1 ms. `cargo test -p kanavos-render --release --test headless -- --ignored --nocapture` repeats the
  measurement.
