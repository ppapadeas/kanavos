//! GPU resources: uniform layout, pipelines and render targets.

use bytemuck::{Pod, Zeroable};

use crate::prep::{EdgeInstance, SurfaceVertex};

/// Colour format of the scene targets. Shading happens in linear light and the hardware encodes to sRGB.
pub(crate) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Non-sRGB view format of the output texture: sampling it returns the stored (gamma-encoded) bytes, which is
/// what egui expects from user textures.
pub(crate) const OUTPUT_VIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Depth format (reverse-Z: clear to 0, nearer is greater).
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Ambient occlusion target.
pub(crate) const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// Width of the object state texture (texels per row).
pub(crate) const STATE_TEXTURE_WIDTH: u32 = 1024;

/// Per-frame uniforms. Must match `struct Frame` in `shaders/frame.wgsl` field for field.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub inv_proj: [[f32; 4]; 4],
    pub light_view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub forward: [f32; 4],
    pub viewport: [f32; 4],
    pub sun_dir: [f32; 4],
    pub sun_color: [f32; 4],
    pub sky_color: [f32; 4],
    pub ground_color: [f32; 4],
    pub bg_top: [f32; 4],
    pub bg_bottom: [f32; 4],
    pub edge_color: [f32; 4],
    pub select_color: [f32; 4],
    pub hover_color: [f32; 4],
    pub cut_color: [f32; 4],
    pub grid_color: [f32; 4],
    pub grid_a: [f32; 4],
    pub grid_b: [f32; 4],
    pub edge_params: [f32; 4],
    pub shadow_params: [f32; 4],
    pub light_params: [f32; 4],
    pub ao_params: [f32; 4],
    pub clip: [[f32; 4]; 6],
    pub counts: [u32; 4],
}

/// Flag bits in `FrameUniforms::counts[1]`.
pub(crate) const FLAG_EDGES: u32 = 1;
/// Silhouettes of smooth surfaces.
pub(crate) const FLAG_SILHOUETTES: u32 = 2;

/// Object state bits (one `u32` per slot in the state texture).
pub(crate) const ST_HIDDEN: u32 = 1;
/// Ghosted (drawn faint, not pickable).
pub(crate) const ST_GHOST: u32 = 2;
/// Selected.
pub(crate) const ST_SELECTED: u32 = 4;
/// Under the pointer.
pub(crate) const ST_HOVER: u32 = 8;

const SURFACE_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm16x4, 2 => Unorm8x4, 3 => Uint32];
const EDGE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32x3, 2 => Snorm8x4, 3 => Snorm8x4, 4 => Uint32, 5 => Uint32
];

fn surface_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<SurfaceVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &SURFACE_ATTRIBUTES,
    }
}

fn edge_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<EdgeInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &EDGE_ATTRIBUTES,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<FrameUniforms>() as u64),
        },
        count: None,
    }
}

fn texture_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
    multisampled: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled,
        },
        count: None,
    }
}

/// Depth-stencil state with the given test.
fn depth(compare: wgpu::CompareFunction, write: bool) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    })
}

/// All render pipelines and bind group layouts.
#[derive(Debug)]
pub(crate) struct Pipelines {
    pub scene_layout: wgpu::BindGroupLayout,
    pub background: wgpu::RenderPipeline,
    pub opaque: wgpu::RenderPipeline,
    pub ghost: wgpu::RenderPipeline,
    pub transparent: wgpu::RenderPipeline,
    pub edges: wgpu::RenderPipeline,
    pub edges_occluded: wgpu::RenderPipeline,
    pub grid: wgpu::RenderPipeline,
    pub shadow: wgpu::RenderPipeline,
    pub pick: wgpu::RenderPipeline,
    /// Ambient occlusion; absent when the device cannot read multisampled depth in shaders.
    pub post: Option<PostPipelines>,
}

/// Ambient occlusion and composite pipelines.
#[derive(Debug)]
pub(crate) struct PostPipelines {
    pub ao_layout: wgpu::BindGroupLayout,
    pub composite_layout: wgpu::BindGroupLayout,
    pub ao: wgpu::RenderPipeline,
    pub composite: wgpu::RenderPipeline,
}

/// Runs `f` inside a validation error scope and returns its result, or the first validation error.
fn validated<T>(device: &wgpu::Device, f: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = f();
    match pollster::block_on(scope.pop()) {
        None => Ok(value),
        Some(e) => Err(e.to_string()),
    }
}

/// The scene shader source (frame uniforms + scene passes).
pub(crate) fn scene_shader_source() -> String {
    format!("{}\n{}", include_str!("shaders/frame.wgsl"), include_str!("shaders/scene.wgsl"))
}

/// The post-processing shader source for a sample count.
pub(crate) fn post_shader_source(msaa: u32) -> String {
    let depth_type = if msaa > 1 { "texture_depth_multisampled_2d" } else { "texture_depth_2d" };
    format!("{}\n{}", include_str!("shaders/frame.wgsl"), include_str!("shaders/post.wgsl"))
        .replace("DEPTH_TEXTURE", depth_type)
}

struct PipelineSpec<'a> {
    label: &'a str,
    module: &'a wgpu::ShaderModule,
    layout: &'a wgpu::PipelineLayout,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    targets: &'a [Option<wgpu::ColorTargetState>],
    depth: Option<wgpu::DepthStencilState>,
    samples: u32,
}

fn pipeline(device: &wgpu::Device, s: PipelineSpec<'_>) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(s.label),
        layout: Some(s.layout),
        vertex: wgpu::VertexState {
            module: s.module,
            entry_point: Some(s.vs),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: s.buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: s.depth,
        multisample: wgpu::MultisampleState { count: s.samples, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module: s.module,
            entry_point: Some(s.fs),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: s.targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn pipeline_layout(device: &wgpu::Device, label: &str, bgl: &wgpu::BindGroupLayout) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    })
}

const OPAQUE_TARGET: [Option<wgpu::ColorTargetState>; 1] =
    [Some(wgpu::ColorTargetState { format: COLOR_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })];

impl PostPipelines {
    fn new(device: &wgpu::Device, msaa: u32) -> PostPipelines {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let ms = msaa > 1;
        let ao_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kanavos ambient occlusion"),
            entries: &[
                uniform_entry(0, fragment),
                texture_entry(1, fragment, wgpu::TextureSampleType::Depth, ms),
            ],
        });
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kanavos composite"),
            entries: &[
                uniform_entry(0, fragment),
                texture_entry(1, fragment, wgpu::TextureSampleType::Depth, ms),
                texture_entry(2, fragment, float, false),
                texture_entry(3, fragment, float, false),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kanavos post shader"),
            source: wgpu::ShaderSource::Wgsl(post_shader_source(msaa).into()),
        });
        let ao_pl = pipeline_layout(device, "kanavos ambient occlusion", &ao_layout);
        let composite_pl = pipeline_layout(device, "kanavos composite", &composite_layout);
        let ao = pipeline(
            device,
            PipelineSpec {
                label: "kanavos ambient occlusion",
                module: &module,
                layout: &ao_pl,
                vs: "vs_fullscreen",
                fs: "fs_ao",
                buffers: &[],
                targets: &[Some(wgpu::ColorTargetState {
                    format: AO_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                depth: None,
                samples: 1,
            },
        );
        let composite = pipeline(
            device,
            PipelineSpec {
                label: "kanavos composite",
                module: &module,
                layout: &composite_pl,
                vs: "vs_fullscreen",
                fs: "fs_composite",
                buffers: &[],
                targets: &OPAQUE_TARGET,
                depth: None,
                samples: 1,
            },
        );
        PostPipelines { ao_layout, composite_layout, ao, composite }
    }
}

impl Pipelines {
    /// Builds every pipeline for a sample count (1 or 4). Fails when the device rejects the scene shaders;
    /// ambient occlusion is left out (not an error) when only its shaders are rejected.
    pub fn new(device: &wgpu::Device, msaa: u32) -> Result<Pipelines, String> {
        let mut pipelines = validated(device, || Pipelines::scene(device, msaa))?;
        pipelines.post = validated(device, || PostPipelines::new(device, msaa)).ok();
        Ok(pipelines)
    }

    fn scene(device: &wgpu::Device, msaa: u32) -> Pipelines {
        let vf = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kanavos scene"),
            entries: &[
                uniform_entry(0, vf),
                texture_entry(1, vf, wgpu::TextureSampleType::Uint, false),
                texture_entry(2, fragment, wgpu::TextureSampleType::Depth, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: fragment,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let scene_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kanavos scene shader"),
            source: wgpu::ShaderSource::Wgsl(scene_shader_source().into()),
        });
        let scene_pl = pipeline_layout(device, "kanavos scene", &scene_layout);
        let opaque_target = OPAQUE_TARGET;
        let blended_target = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let surface = [Some(surface_layout())];
        let edge = [Some(edge_layout())];
        use wgpu::CompareFunction as C;

        let scene = |label, vs, fs, buffers, targets, depth| {
            pipeline(
                device,
                PipelineSpec {
                    label,
                    module: &scene_module,
                    layout: &scene_pl,
                    vs,
                    fs,
                    buffers,
                    targets,
                    depth,
                    samples: msaa,
                },
            )
        };
        let background = scene(
            "kanavos background",
            "vs_background",
            "fs_background",
            &[],
            &opaque_target,
            depth(C::Always, false),
        );
        let opaque = scene(
            "kanavos opaque",
            "vs_opaque",
            "fs_opaque",
            &surface,
            &opaque_target,
            depth(C::GreaterEqual, true),
        );
        let ghost = scene(
            "kanavos ghost",
            "vs_ghost",
            "fs_ghost",
            &surface,
            &blended_target,
            depth(C::GreaterEqual, false),
        );
        let transparent = scene(
            "kanavos transparent",
            "vs_transparent",
            "fs_transparent",
            &surface,
            &blended_target,
            depth(C::GreaterEqual, false),
        );
        let edges = scene(
            "kanavos edges",
            "vs_edge",
            "fs_edge",
            &edge,
            &blended_target,
            depth(C::GreaterEqual, false),
        );
        let edges_occluded = scene(
            "kanavos occluded edges",
            "vs_edge_occluded",
            "fs_edge_dashed",
            &edge,
            &blended_target,
            depth(C::Less, false),
        );
        let grid =
            scene("kanavos grid", "vs_grid", "fs_grid", &[], &blended_target, depth(C::GreaterEqual, false));

        let shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kanavos shadow"),
            layout: Some(&scene_pl),
            vertex: wgpu::VertexState {
                module: &scene_module,
                entry_point: Some("vs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &surface,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(C::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_module,
                entry_point: Some("fs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });

        let pick_targets = [
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R32Uint,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
        ];
        let pick = pipeline(
            device,
            PipelineSpec {
                label: "kanavos pick",
                module: &scene_module,
                layout: &scene_pl,
                vs: "vs_pick",
                fs: "fs_pick",
                buffers: &surface,
                targets: &pick_targets,
                depth: depth(C::GreaterEqual, true),
                samples: 1,
            },
        );

        Pipelines {
            scene_layout,
            background,
            opaque,
            ghost,
            transparent,
            edges,
            edges_occluded,
            grid,
            shadow,
            pick,
            post: None,
        }
    }
}

/// A texture with its default view.
#[derive(Debug)]
pub(crate) struct Target {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Target {
    pub fn new(
        device: &wgpu::Device,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        samples: u32,
        usage: wgpu::TextureUsages,
        view_formats: &[wgpu::TextureFormat],
    ) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Target { texture, view }
    }
}

/// Size-dependent render targets.
#[derive(Debug)]
pub(crate) struct Targets {
    pub size: [u32; 2],
    /// Multisampled colour (absent without MSAA).
    pub color_ms: Option<Target>,
    /// Depth (multisampled with MSAA).
    pub depth: Target,
    /// Final image (sRGB storage).
    pub output: Target,
    /// Non-sRGB view of the final image for egui.
    pub output_unorm: wgpu::TextureView,
    /// Ambient occlusion targets, when supported.
    pub post: Option<PostTargets>,
}

/// Targets of the ambient occlusion passes.
#[derive(Debug)]
pub(crate) struct PostTargets {
    /// Resolved scene colour before ambient occlusion.
    pub scene: Target,
    /// Ambient occlusion factor.
    pub ao: Target,
    pub ao_bind_group: wgpu::BindGroup,
    pub composite_bind_group: wgpu::BindGroup,
}

impl PostTargets {
    fn new(
        device: &wgpu::Device,
        post: &PostPipelines,
        frame: &wgpu::Buffer,
        depth: &Target,
        size: [u32; 2],
    ) -> PostTargets {
        use wgpu::TextureUsages as U;
        let scene = Target::new(
            device,
            "kanavos scene colour",
            size,
            COLOR_FORMAT,
            1,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
            &[],
        );
        let ao = Target::new(
            device,
            "kanavos ambient occlusion",
            size,
            AO_FORMAT,
            1,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
            &[],
        );
        let view = |v| wgpu::BindingResource::TextureView(v);
        let ao_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kanavos ambient occlusion"),
            layout: &post.ao_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: frame.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: view(&depth.view) },
            ],
        });
        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kanavos composite"),
            layout: &post.composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: frame.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: view(&depth.view) },
                wgpu::BindGroupEntry { binding: 2, resource: view(&scene.view) },
                wgpu::BindGroupEntry { binding: 3, resource: view(&ao.view) },
            ],
        });
        PostTargets { scene, ao, ao_bind_group, composite_bind_group }
    }
}

impl Targets {
    pub fn new(
        device: &wgpu::Device,
        pipelines: &Pipelines,
        frame: &wgpu::Buffer,
        size: [u32; 2],
        msaa: u32,
    ) -> Targets {
        use wgpu::TextureUsages as U;
        let color_ms = (msaa > 1).then(|| {
            Target::new(device, "kanavos colour (msaa)", size, COLOR_FORMAT, msaa, U::RENDER_ATTACHMENT, &[])
        });
        let depth = Target::new(
            device,
            "kanavos depth",
            size,
            DEPTH_FORMAT,
            msaa,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
            &[],
        );
        let output = Target::new(
            device,
            "kanavos viewport",
            size,
            COLOR_FORMAT,
            1,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
            &[OUTPUT_VIEW_FORMAT],
        );
        let output_unorm = output.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("kanavos viewport (unorm view)"),
            format: Some(OUTPUT_VIEW_FORMAT),
            ..wgpu::TextureViewDescriptor::default()
        });
        let post = pipelines.post.as_ref().map(|p| PostTargets::new(device, p, frame, &depth, size));
        Targets { size, color_ms, depth, output, output_unorm, post }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layout_matches_shader_size() {
        // 5 matrices, 20 vectors, 6 clip planes and one uvec4.
        assert_eq!(std::mem::size_of::<FrameUniforms>(), 5 * 64 + 20 * 16 + 6 * 16 + 16);
        assert_eq!(std::mem::size_of::<FrameUniforms>() % 16, 0);
    }

    #[test]
    fn vertex_layouts_match_structs() {
        assert_eq!(std::mem::size_of::<SurfaceVertex>(), 28);
        assert_eq!(std::mem::size_of::<EdgeInstance>(), 40);
        let last = SURFACE_ATTRIBUTES[3];
        assert_eq!(last.offset + 4, 28);
        let last = EDGE_ATTRIBUTES[5];
        assert_eq!(last.offset + 4, 40);
    }

    #[test]
    fn shaders_parse_and_validate() {
        // Parsing and validation need no GPU, so WGSL mistakes show up in plain `cargo test`.
        for (name, src) in [
            ("scene", scene_shader_source()),
            ("post msaa", post_shader_source(4)),
            ("post", post_shader_source(1)),
        ] {
            let module = naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let entry_points: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
            assert!(!entry_points.is_empty(), "{name}");
        }
    }

    #[test]
    fn uniform_struct_matches_wgsl() {
        // The WGSL struct must have the same size as the Rust struct.
        let src = scene_shader_source();
        let module = naga::front::wgsl::parse_str(&src).unwrap();
        let frame = module.types.iter().find(|(_, t)| t.name.as_deref() == Some("Frame")).unwrap().1;
        let size = frame.inner.size(module.to_ctx());
        assert_eq!(size as usize, std::mem::size_of::<FrameUniforms>());
    }
}
