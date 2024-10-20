use std::borrow::Cow;
use std::num::NonZeroU64;
use std::ops::Range;

use dotrix_gpu as gpu;
use dotrix_gpu::backend as wgpu;
use dotrix_log as log;
use gpu::backend::BindGroupEntry;

use crate::font;
use crate::overlay::VertexAttributes;

pub struct Render {
    pub render_pipeline: gpu::RenderPipeline,
    pub shader_module: gpu::ShaderModule,
    pub vertex_buffer: SlicedBuffer,
    pub index_buffer: SlicedBuffer,
    pub uniform_buffer: gpu::Buffer,
    pub uniform_bind_group_layout: gpu::backend::BindGroupLayout,
    pub texture_bind_group_layout: gpu::backend::BindGroupLayout,
    pub bind_group: gpu::backend::BindGroup,
    pub default_texture: gpu::Texture,
    pub default_sampler: gpu::backend::Sampler,
    pub default_texture_bind_group: gpu::backend::BindGroup,
}

impl Render {
    pub fn new(gpu: &gpu::Gpu, initial_vertex_count: u64) -> Self {
        use dotrix_mesh::VertexBufferLayout;
        let shader_module = Self::create_shader_module(gpu);
        let uniform_buffer = gpu
            .buffer("dotrix::ui::uniform_buffer")
            .size(std::mem::size_of::<Uniform>() as u64)
            .allow_copy_dst()
            .use_as_uniform()
            .create();

        let size = VertexAttributes::vertex_size() as u64 * 3 * initial_vertex_count;
        let vertex_buffer = SlicedBuffer {
            buffer: Self::create_vertex_buffer(gpu, size),
            slices: Vec::with_capacity(64),
            size,
        };

        let size = std::mem::size_of::<u32>() as u64 * 3 * initial_vertex_count;
        let index_buffer = SlicedBuffer {
            buffer: Self::create_index_buffer(gpu, size),
            slices: Vec::with_capacity(64),
            size,
        };

        let uniform_bind_group_layout = Self::create_uniform_bind_group_layout(gpu);
        let texture_bind_group_layout = Self::create_texture_bind_group_layout(gpu);
        let render_pipeline = Self::create_render_pipeline(
            gpu,
            &shader_module,
            None,
            &[&uniform_bind_group_layout, &texture_bind_group_layout],
        );

        let bind_group = gpu.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dotrix::ui::uniform"),
            layout: &&uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.inner.as_entire_binding(),
            }],
        });

        let default_texture = Self::create_default_texture(gpu);
        let default_sampler = Self::create_default_sampler(gpu);
        let default_texture_bind_group = Self::create_default_texture_bind_group(
            gpu,
            &texture_bind_group_layout,
            &default_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            &default_sampler,
        );

        Self {
            render_pipeline,
            shader_module,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group_layout,
            texture_bind_group_layout,
            bind_group,
            default_texture,
            default_sampler,
            default_texture_bind_group,
        }
    }

    pub fn clear_vertex_buffer(&mut self, gpu: &gpu::Gpu, size: u64) {
        self.vertex_buffer.slices.clear();
        if self.vertex_buffer.size < size {
            self.vertex_buffer.buffer = Self::create_vertex_buffer(gpu, size);
            self.vertex_buffer.size = size;
        }
    }

    pub fn clear_index_buffer(&mut self, gpu: &gpu::Gpu, size: u64) {
        self.index_buffer.slices.clear();
        if self.index_buffer.size < size {
            self.index_buffer.buffer = Self::create_index_buffer(gpu, size);
            self.index_buffer.size = size;
        }
    }

    pub fn write_uniform(&self, gpu: &gpu::Gpu, frame_width: f32, frame_height: f32) {
        gpu.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[Uniform {
                frame_width,
                frame_height,
                padding: [0; 2],
            }]),
        );
    }

    fn create_shader_module(gpu: &gpu::Gpu) -> gpu::ShaderModule {
        gpu.create_shader_module("dotrix::ui::shader", Cow::Borrowed(include_str!("ui.wgsl")))
    }

    fn create_vertex_buffer(gpu: &gpu::Gpu, size: u64) -> gpu::Buffer {
        gpu.buffer("dotrix::ui::vertex_buffer")
            .size(size)
            .allow_copy_dst()
            .use_as_vertex()
            .create()
    }

    fn create_index_buffer(gpu: &gpu::Gpu, size: u64) -> gpu::Buffer {
        gpu.buffer("dotrix::ui::index_buffer")
            .size(size)
            .allow_copy_dst()
            .use_as_index()
            .create()
    }

    fn create_uniform_bind_group_layout(gpu: &gpu::Gpu) -> wgpu::BindGroupLayout {
        gpu.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dotrix::ui::uniform_bind_group_layout"),
            entries: &[gpu::backend::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: gpu::backend::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<Uniform>() as _),
                    ty: wgpu::BufferBindingType::Uniform,
                },
                count: None,
            }],
        })
    }

    fn create_texture_bind_group_layout(gpu: &gpu::Gpu) -> wgpu::BindGroupLayout {
        gpu.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dotrix::ui::texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn create_render_pipeline(
        gpu: &gpu::Gpu,
        shader_module: &gpu::ShaderModule,
        depth_buffer_format: Option<wgpu::TextureFormat>,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
    ) -> gpu::RenderPipeline {
        use dotrix_mesh::VertexBufferLayout;
        let pipeline_layout = gpu.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dotirx::ui::pipeline_layout"),
            bind_group_layouts,
            push_constant_ranges: &[],
        });

        let depth_stencil = depth_buffer_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });

        let surface_color_format = gpu.surface_format();
        let attributes = VertexAttributes::attributes()
            .map(
                |(vertex_format, offset, shader_location)| wgpu::VertexAttribute {
                    format: gpu::map_vertex_format(vertex_format),
                    offset,
                    shader_location,
                },
            )
            .collect::<Vec<_>>();

        gpu.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dotrix::ui::render_pipeline"),
            layout: Some(&pipeline_layout.inner),
            vertex: wgpu::VertexState {
                entry_point: "vs_main",
                module: &shader_module.inner,
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: VertexAttributes::vertex_size() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                unclipped_depth: false,
                conservative: false,
                cull_mode: None,
                front_face: wgpu::FrontFace::default(),
                polygon_mode: wgpu::PolygonMode::default(),
                strip_index_format: None,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState {
                alpha_to_coverage_enabled: false,
                count: gpu.sample_count(),
                mask: !0,
            },

            fragment: Some(wgpu::FragmentState {
                module: &shader_module.inner,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        })
    }

    fn create_default_texture(gpu: &gpu::Gpu) -> gpu::Texture {
        let default_texture_size = 1;
        let default_texture_bytes = [0xFF, 0xFF, 0xFF, 0xFF];
        gpu.texture("dotrix::ui::default_texture")
            .size(default_texture_size, default_texture_size)
            .allow_copy_dst()
            .dimension_d2()
            .format_rgba_u8_norm_srgb()
            .use_as_texture_binding()
            .data(&default_texture_bytes)
            .create()
        /* TODO: copy that code to application level
        let charsets = [
            font::Charset::Latin,
            font::Charset::Cyrillic,
            font::Charset::Greek,
        ];
        let font_bytes = include_bytes!("../../resources/fonts/Jura-Regular.ttf") as &[u8];
        let font = font::Font::from_bytes(28.0, &charsets, font_bytes);
        let atlas = font.atlas();

        gpu.texture("dotrix::ui::default_texture")
            .size(atlas.width(), atlas.height())
            .allow_copy_dst()
            .dimension_d2()
            .format_alpha_u8()
            .use_as_texture_binding()
            .data(atlas.bitmap())
            .create()
        */
    }

    fn create_default_sampler(gpu: &gpu::Gpu) -> wgpu::Sampler {
        gpu.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,

            ..Default::default()
        })
    }

    fn create_default_texture_bind_group(
        gpu: &gpu::Gpu,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        default_texture_view: &gpu::TextureView,
        default_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        gpu.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dotrix::ui::texture_bind_group"),
            layout: texture_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_texture_view.inner),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&default_sampler),
                },
            ],
        })
    }

    pub fn create_texture_bind_group(
        &self,
        gpu: &gpu::Gpu,
        texture: &gpu::TextureView,
    ) -> wgpu::BindGroup {
        Self::create_default_texture_bind_group(
            gpu,
            &self.texture_bind_group_layout,
            texture,
            &self.default_sampler,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct Uniform {
    frame_width: f32,
    frame_height: f32,
    padding: [u32; 2],
}

unsafe impl bytemuck::Pod for Uniform {}
unsafe impl bytemuck::Zeroable for Uniform {}

pub struct SlicedBuffer {
    pub buffer: gpu::Buffer,
    pub slices: Vec<Range<u64>>,
    pub size: u64,
}

impl SlicedBuffer {
    pub fn write(&mut self, gpu: &gpu::Gpu, slices: &[Vec<u8>]) {
        let mut offset = 0;
        for slice in slices.iter() {
            let next_offset = offset + slice.len() as u64;
            gpu.write_buffer(&self.buffer, offset, slice);
            self.slices.push(offset..next_offset);
            offset = next_offset;
        }
    }
}
