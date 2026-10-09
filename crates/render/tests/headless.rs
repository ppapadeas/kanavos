//! Renders the sample house without a window, checks pixels and picking, and writes PNGs to
//! `target/render-tests/` for visual inspection.
//!
//! The tests skip (pass with a message) when no GPU adapter, not even a software one, is available.

// Test helpers fail loudly on purpose.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};

use kanavos_render::demo::{self, ids};
use kanavos_render::wgpu;
use kanavos_render::{
    HeadlessGpu, Image, OrbitCamera, Projection, Renderer, RendererConfig, StandardView, ViewParams, Viewport,
};

use kanavos_geom::{Bbox3, Point3};

const SIZE: [u32; 2] = [960, 640];

fn gpu() -> Option<HeadlessGpu> {
    match HeadlessGpu::new() {
        Ok(g) => {
            eprintln!("adapter: {}", g.adapter_description());
            Some(g)
        }
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn out_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let dir = target.join("render-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_png(img: &Image, name: &str) {
    let path = out_dir().join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    enc.write_header().unwrap().write_image_data(&img.rgba).unwrap();
    eprintln!("wrote {}", path.display());
}

fn house(g: &HeadlessGpu) -> Renderer {
    let mut r = Renderer::new(&g.device, RendererConfig::default()).unwrap();
    let update = r.set_scene(&demo::sample_house());
    assert!(update.changed >= 15, "{update:?}");
    r
}

fn fitted(r: &Renderer, view: StandardView, projection: Projection) -> OrbitCamera {
    let mut c = OrbitCamera::looking_at(Point3::ORIGIN, view, 10.0);
    c.projection = projection;
    // Fit to the building rather than the whole site.
    let b = r
        .bounds_of([ids::SLAB, ids::ROOF_SOUTH, ids::ROOF_NORTH, ids::TERRACE, ids::PERGOLA])
        .expect("house bounds");
    c.fit_to(&b, Viewport::new(f64::from(SIZE[0]), f64::from(SIZE[1])));
    c
}

fn world(x: f64, y: f64, z: f64) -> Point3 {
    let o = demo::HOUSE_ORIGIN;
    Point3::new(o.x + x, o.y + y, o.z + z)
}

fn screen(params: &ViewParams, p: Point3) -> [f64; 2] {
    let vp = Viewport::new(f64::from(params.size[0]), f64::from(params.size[1]));
    let s = params.camera.project(p, vp).expect("in front of the camera");
    [s.x, s.y]
}

fn close_rgb(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(a[i]) - i32::from(b[i])).abs() <= tol)
}

fn distinct_colours(img: &Image) -> usize {
    let mut set = std::collections::HashSet::new();
    for px in img.rgba.chunks_exact(4).step_by(7) {
        set.insert([px[0], px[1], px[2]]);
    }
    set.len()
}

#[test]
fn renders_and_picks_the_sample_house() {
    let Some(g) = gpu() else {
        return;
    };
    let mut r = house(&g);
    let camera = fitted(&r, StandardView::IsoSouthWest, Projection::Perspective);
    let params = ViewParams::new(camera, SIZE, 1.0);
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_iso.png");
    assert_eq!((img.width, img.height), (SIZE[0], SIZE[1]));

    // The top-left corner shows the background gradient's top colour.
    let top = params.style.background_top;
    let corner = img.pixel(2, 2).unwrap();
    assert!(close_rgb(corner, [top.r, top.g, top.b, 255], 6), "corner {corner:?}");
    assert_eq!(corner[3], 255, "opaque output");
    assert!(distinct_colours(&img) > 200, "a shaded scene, not a flat image");

    // Pick a point on the outer face of the south wall, left of the first window.
    let target = world(0.7, 0.0, 1.6);
    let px = screen(&params, target);
    let hit = r.pick(&g.device, &g.queue, &params, px).unwrap();
    assert_eq!(hit.id, Some(ids::SOUTH_WALL), "{hit:?}");
    let pos = hit.position.unwrap();
    assert!(pos.distance(target) < 0.05, "picked {pos:?}, expected {target:?}");

    // The pixel under the pick shows the wall, not the background.
    let wall_px = img.pixel(px[0] as u32, px[1] as u32).unwrap();
    assert!(!close_rgb(wall_px, corner, 4));

    // Background.
    let miss = r.pick(&g.device, &g.queue, &params, [3.0, 3.0]).unwrap();
    assert_eq!(miss.id, None);
    assert!(miss.position.is_none());

    // Glass is pickable and belongs to its window.
    let glass = r.pick(&g.device, &g.queue, &params, screen(&params, world(2.2, 0.1, 1.55))).unwrap();
    assert_eq!(glass.id, Some(ids::WINDOW_SOUTH_WEST), "{glass:?}");

    // Hidden objects are skipped by picking.
    r.set_hidden([ids::SOUTH_WALL]);
    let through = r.pick(&g.device, &g.queue, &params, px).unwrap();
    assert_ne!(through.id, Some(ids::SOUTH_WALL));
    r.set_hidden([]);

    // Asynchronous picking.
    let ticket = r.request_pick(&g.device, &g.queue, &params, px).unwrap();
    let mut answer = None;
    for _ in 0..200 {
        if let Some((_, res)) = r.poll_picks(&g.device).into_iter().find(|(t, _)| *t == ticket) {
            answer = Some(res);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(answer.unwrap().id, Some(ids::SOUTH_WALL));
}

#[test]
fn section_box_selection_and_ghosting() {
    let Some(g) = gpu() else {
        return;
    };
    let mut r = house(&g);
    let camera = fitted(&r, StandardView::IsoSouthWest, Projection::Perspective);
    let mut params = ViewParams::new(camera, SIZE, 1.0);

    // Before cutting, the roof is under this pixel.
    let roof_px = screen(&params, world(5.0, 2.0, 4.2));
    assert_eq!(r.pick(&g.device, &g.queue, &params, roof_px).unwrap().id, Some(ids::ROOF_SOUTH));

    // A section box at 1.5 m: the roof and everything above is cut away.
    let o = demo::HOUSE_ORIGIN;
    params.set_section_box(&Bbox3 {
        min: Point3::new(o.x - 20.0, o.y - 20.0, o.z - 5.0),
        max: Point3::new(o.x + 30.0, o.y + 30.0, o.z + 1.5),
    });
    assert_eq!(params.clip_planes.len(), 6);
    let below = r.pick(&g.device, &g.queue, &params, roof_px).unwrap();
    assert_ne!(below.id, Some(ids::ROOF_SOUTH), "{below:?}");
    if let Some(p) = below.position {
        assert!(p.z <= o.z + 1.5 + 0.01, "hit above the cut: {p:?}");
    }

    r.set_selection([ids::PARTITION]);
    r.set_hovered(Some(ids::WEST_WALL));
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_section_selected.png");

    // Close to the cut: walls show a flat cut colour outlined by cut lines.
    let mut close = params.clone();
    close.camera = OrbitCamera::looking_at(world(1.5, 0.6, 1.2), StandardView::IsoSouthWest, 4.0);
    close.camera.pitch = 0.9;
    let img = r.render_to_image(&g.device, &g.queue, &close).unwrap();
    write_png(&img, "house_section_close_up.png");
    let cut_px = screen(&close, world(0.6, 0.125, 1.5));
    let cut_colour = params.style.cut_color;
    let seen = img.pixel(cut_px[0] as u32, cut_px[1] as u32).unwrap();
    assert!(
        close_rgb(seen, [cut_colour.r, cut_colour.g, cut_colour.b, 255], 40),
        "cut face of the south wall shows the cut colour, got {seen:?}"
    );

    // Selection colour appears somewhere (thick teal edges and tint).
    let sel = params.style.selection_color;
    let teal = img
        .rgba
        .chunks_exact(4)
        .filter(|p| close_rgb([p[0], p[1], p[2], 255], [sel.r, sel.g, sel.b, 255], 30))
        .count();
    assert!(teal > 50, "selection visible: {teal} pixels");

    // Ghost the roof instead of cutting it.
    params.clip_planes.clear();
    r.set_selection([]);
    r.set_hovered(None);
    r.set_ghosted([ids::ROOF_SOUTH, ids::ROOF_NORTH]);
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_ghosted_roof.png");
    let through = r.pick(&g.device, &g.queue, &params, roof_px).unwrap();
    assert_ne!(through.id, Some(ids::ROOF_SOUTH), "ghosted objects are not pickable");
    r.set_ghosted([]);
}

#[test]
fn other_views_and_settings() {
    let Some(g) = gpu() else {
        return;
    };
    let mut r = house(&g);

    let top = fitted(&r, StandardView::Top, Projection::Parallel);
    let params = ViewParams::new(top, SIZE, 1.0);
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_top_parallel.png");
    // Looking straight down at the ridge.
    let ridge = r.pick(&g.device, &g.queue, &params, screen(&params, world(5.0, 1.0, 5.0))).unwrap();
    assert_eq!(ridge.id, Some(ids::ROOF_SOUTH));

    let mut front = fitted(&r, StandardView::IsoSouthEast, Projection::Perspective);
    front.zoom_at(
        0.6,
        [f64::from(SIZE[0]) * 0.5, f64::from(SIZE[1]) * 0.55],
        Viewport::new(960.0, 640.0),
        None,
    );
    let mut params = ViewParams::new(front, SIZE, 1.0);
    params.style.grid = true;
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_south_east.png");

    // Without multisampling, shadows, ambient occlusion and edges: the plain path must work too.
    let mut plain =
        Renderer::new(&g.device, RendererConfig { msaa_samples: 1, ..RendererConfig::default() }).unwrap();
    plain.set_scene(&demo::sample_house());
    let mut params =
        ViewParams::new(fitted(&plain, StandardView::IsoNorthWest, Projection::Perspective), SIZE, 1.0);
    params.style.shadows = false;
    params.style.ambient_occlusion = false;
    params.style.edges = false;
    let img = plain.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_plain_north_west.png");
    assert!(distinct_colours(&img) > 50);

    // Resizing recreates the targets and bumps the generation.
    let a = r.render(&g.device, &g.queue, &ViewParams::new(top, [320, 200], 1.0));
    let b = r.render(&g.device, &g.queue, &ViewParams::new(top, [320, 200], 1.0));
    let c = r.render(&g.device, &g.queue, &ViewParams::new(top, [321, 200], 2.0));
    assert_eq!(a.generation, b.generation);
    assert_ne!(b.generation, c.generation);
    assert_eq!(c.size, [321, 200]);
    assert_eq!(c.texture.format(), wgpu::TextureFormat::Rgba8UnormSrgb);
    assert!(c.stats.draw_calls > 3 && c.stats.triangles > 500, "{:?}", c.stats);
}

#[test]
fn interior_close_up() {
    let Some(g) = gpu() else {
        return;
    };
    let mut r = house(&g);
    r.set_hidden([ids::ROOF_SOUTH, ids::ROOF_NORTH]);
    // Looking into the room corner where the partition meets the north wall.
    let mut camera = OrbitCamera::looking_at(world(5.6, 6.6, 0.6), StandardView::IsoSouthWest, 1.0);
    camera.yaw = -2.0;
    camera.pitch = 0.75;
    camera.distance = 6.5;
    let mut params = ViewParams::new(camera, SIZE, 1.0);
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_interior.png");

    if !r.ambient_occlusion_supported() {
        eprintln!("ambient occlusion is not supported on this adapter; skipping the comparison");
        return;
    }
    params.style.ambient_occlusion = false;
    let flat = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&flat, "house_interior_no_ao.png");
    // Ambient occlusion only darkens.
    let (mut darker, mut lighter) = (0usize, 0usize);
    for (a, b) in img.rgba.chunks_exact(4).zip(flat.rgba.chunks_exact(4)) {
        let (la, lb) = (
            u32::from(a[0]) + u32::from(a[1]) + u32::from(a[2]),
            u32::from(b[0]) + u32::from(b[1]) + u32::from(b[2]),
        );
        if la + 6 < lb {
            darker += 1;
        } else if la > lb + 6 {
            lighter += 1;
        }
    }
    assert!(darker > 1000, "occlusion darkens corners: {darker}");
    assert!(lighter < darker / 20, "occlusion should not brighten: {lighter} vs {darker}");
}

#[test]
fn incremental_updates() {
    let Some(g) = gpu() else {
        return;
    };
    let mut r = house(&g);
    let scene = demo::sample_house();
    // Same scene again: nothing changes.
    let u = r.set_scene(&scene);
    assert_eq!(u.changed, 0);
    assert_eq!(u.removed, 0);
    // Remove the roof by updating with a scene that lacks it.
    let mut no_roof = scene.clone();
    no_roof.meshes.retain(|m| m.id != ids::ROOF_SOUTH && m.id != ids::ROOF_NORTH);
    let u = r.set_scene(&no_roof);
    assert_eq!((u.changed, u.removed), (0, 2));
    let camera = fitted(&r, StandardView::IsoSouthWest, Projection::Perspective);
    let params = ViewParams::new(camera, SIZE, 1.0);
    let img = r.render_to_image(&g.device, &g.queue, &params).unwrap();
    write_png(&img, "house_without_roof.png");
    let px = screen(&params, world(5.0, 2.0, 4.2));
    assert_ne!(r.pick(&g.device, &g.queue, &params, px).unwrap().id, Some(ids::ROOF_SOUTH));
    // Recolour one object through the single-object API.
    let mut wall: Vec<_> = scene.meshes.iter().filter(|m| m.id == ids::EAST_WALL).cloned().collect();
    for m in &mut wall {
        m.color = kanavos_drawing::Rgba::rgb(200, 120, 90);
    }
    assert!(r.update_object(ids::EAST_WALL, &wall));
    assert!(!r.update_object(ids::EAST_WALL, &wall));
    assert!(r.remove_object(ids::EAST_WALL));
    assert!(!r.contains(ids::EAST_WALL));
}

/// A scene the size of a detailed house: 5,000 objects and about 470,000 triangles. Prints preparation and frame
/// times; run with `cargo test -p kanavos-render --release --test headless -- --ignored --nocapture`.
#[test]
#[ignore = "performance measurement"]
fn large_scene_timings() {
    let Some(g) = gpu() else {
        return;
    };
    let origin = Point3::new(360_000.0, 4_100_000.0, 0.0);
    let mut scene = kanavos_render::Scene::new(origin);
    for i in 0..5000u64 {
        let (x, y) = ((i % 100) as f64 * 1.5, (i / 100) as f64 * 1.5);
        let c = kanavos_geom::Point2::new(origin.x + x, origin.y + y);
        let column = kanavos_solid::cylinder(c, 0.3, 0.0, 2.0 + (i % 7) as f64 * 0.3, 24).unwrap();
        let color = kanavos_drawing::Rgba::rgb(180 + (i % 50) as u8, 170, 160);
        scene.push_solid(i, &column, color, (i % 4) as u32);
    }
    let triangles: usize = scene.meshes.iter().map(|m| m.triangle_count()).sum();
    let mut r = Renderer::new(&g.device, RendererConfig::default()).unwrap();
    let t = std::time::Instant::now();
    r.set_scene(&scene);
    let prepare = t.elapsed();
    let t = std::time::Instant::now();
    r.prepare(&g.device, &g.queue);
    g.queue.submit([]);
    let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
    let upload = t.elapsed();

    let mut camera = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 10.0);
    camera.fit_to(&r.scene_bounds().unwrap(), Viewport::new(1920.0, 1080.0));
    let mut params = ViewParams::new(camera, [1920, 1080], 1.0);
    let mut frame = |params: &ViewParams| {
        let t = std::time::Instant::now();
        let out = r.render(&g.device, &g.queue, params);
        let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
        (t.elapsed(), out.stats)
    };
    frame(&params);
    let mut times = Vec::new();
    for i in 0..30 {
        params.camera.orbit(0.01 * f64::from(i), 0.0);
        times.push(frame(&params).0);
    }
    times.sort();
    let (median, stats) = (times[times.len() / 2], frame(&params).1);
    params.style.ambient_occlusion = false;
    params.style.shadows = false;
    let mut plain: Vec<_> = (0..30).map(|_| frame(&params).0).collect();
    plain.sort();

    // Editing one object rebuilds one chunk only.
    let t = std::time::Instant::now();
    let moved = kanavos_solid::cuboid(origin, Point3::new(origin.x + 1.0, origin.y + 1.0, 3.0));
    r.update_object(
        42,
        &[kanavos_render::SceneMesh::from_solid_mesh(42, &moved, origin, kanavos_drawing::Rgba::WHITE, 0)],
    );
    r.prepare(&g.device, &g.queue);
    let edit = t.elapsed();

    println!("adapter: {}", g.adapter_description());
    println!("objects 5000, triangles {triangles}, chunks {}, draw calls {}", stats.chunks, stats.draw_calls);
    println!("set_scene (CPU preparation): {prepare:?}");
    println!("first upload: {upload:?}");
    println!("frame 1920×1080, median (MSAA 4×, shadows, AO, edges): {median:?}");
    println!("frame 1920×1080, median (MSAA 4×, edges only): {:?}", plain[plain.len() / 2]);
    println!("single-object edit + upload: {edit:?}");
    let pick_start = std::time::Instant::now();
    let hit = r.pick(&g.device, &g.queue, &params, [960.0, 540.0]).unwrap();
    println!("blocking pick: {:?} → {:?}", pick_start.elapsed(), hit.id);

    // Close up, most chunks are outside the view and skipped.
    params.camera = OrbitCamera::looking_at(world_of(origin, 3.0, 3.0, 1.0), StandardView::IsoSouthWest, 6.0);
    let t = std::time::Instant::now();
    let stats = r.render(&g.device, &g.queue, &params).stats;
    let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
    println!("close-up frame: {:?}, draw calls {}", t.elapsed(), stats.draw_calls);
    assert!(stats.draw_calls < 20, "culling skips chunks: {stats:?}");
}

fn world_of(origin: Point3, x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(origin.x + x, origin.y + y, origin.z + z)
}
