mod entity;
mod material;

use std::{borrow::Cow, collections::HashMap};

use dotrix_assets as assets;
use dotrix_core as dotrix;
use dotrix_ecs as ecs;
use dotrix_gpu as gpu;
use dotrix_log as log;
use dotrix_math as math;
use dotrix_mesh::{Armature, Mesh};
use dotrix_types::{vertex, Id, Transform};

use gpu::backend as wgpu;

pub use entity::Entity;
pub use material::{Material, MaterialUniform};

const DEAFULT_MESH_BUFFER_SIZE: u64 = 64 * 1024 * 1024;
const DEAFULT_TRANSFORM_BUFFER_SIZE: u64 = 8 * 1024 * 1024;
const DEAFULT_INDIRECT_BUFFER_SIZE: u64 = 8 * 1024 * 1024;
const DEAFULT_INSTANCES_BUFFER_SIZE: u64 = 1000 * std::mem::size_of::<Instance>() as u64;
const DEAFULT_MATERIALS_BUFFER_SIZE: u64 = 50 * std::mem::size_of::<MaterialUniform>() as u64;

/// Contains PBR related buffer IDs
#[derive(Default, Debug, Clone, Copy)]
pub struct Buffers {
    /// Buffer for meshes
    pub mesh: Id<gpu::Buffer>,
    /// Buffer for transformations
    pub transform: Id<gpu::Buffer>,
    /// Materials buffer
    pub materials: Id<gpu::Buffer>,
    /// Solid models rendering pipeline
    pub solid_render_pipeline: Id<gpu::RenderPipeline>,
    /// Indirect buffer
    pub indirect: Id<gpu::Buffer>,
    /// Instances buffer (contains indices to transformations and materials by instance_id)
    pub instances: Id<gpu::Buffer>,
    /// Shader module
    pub shader_module: Id<gpu::ShaderModule>,

    // TODO: add wrapper
    pub bind_group: Id<wgpu::BindGroup>,

    // TODO: remove when camera is implemented
    pub camera_mockup: Id<gpu::Buffer>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraUniform {
    proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
}

unsafe impl bytemuck::Pod for CameraUniform {}
unsafe impl bytemuck::Zeroable for CameraUniform {}

pub struct Allocator {
    mesh_buffer_size: u64,
    transform_buffer_size: u64,
    indirect_buffer_size: u64,
    instances_buffer_size: u64,
    materials_buffer_size: u64,
    buffers: Option<Buffers>,
}

impl Allocator {
    pub fn new(
        mesh_buffer_size: u64,
        transform_buffer_size: u64,
        indirect_buffer_size: u64,
        instances_buffer_size: u64,
        materials_buffer_size: u64,
    ) -> Self {
        Self {
            mesh_buffer_size,
            transform_buffer_size,
            indirect_buffer_size,
            instances_buffer_size,
            materials_buffer_size,
            buffers: None,
        }
    }
}

impl dotrix::Task for Allocator {
    type Context = (dotrix::Mut<gpu::Gpu>,);

    type Output = Buffers;

    fn run(&mut self, (mut gpu,): Self::Context) -> Self::Output {
        if self.buffers.is_none() {
            let mesh_buffer = gpu
                .buffer("dotrix::pbr::mesh")
                .size(self.mesh_buffer_size)
                .allow_copy_dst()
                .use_as_vertex()
                .create();

            let transform_buffer = gpu
                .buffer("dotrix::pbr::transform")
                .size(self.transform_buffer_size)
                .allow_copy_dst()
                .use_as_storage()
                .create();

            let materials_buffer = gpu
                .buffer("dotrix::pbr::materials")
                .size(self.materials_buffer_size)
                .allow_copy_dst()
                .use_as_storage()
                .create();

            let indirect_buffer = gpu
                .buffer("dotrix::pbr::indirect")
                .size(self.indirect_buffer_size)
                .allow_copy_dst()
                .use_as_indirect()
                .create();

            let instances_buffer = gpu
                .buffer("dotrix::pbr::instances")
                .size(self.instances_buffer_size)
                .allow_copy_dst()
                .use_as_storage()
                .create();

            let shader_module = gpu.create_shader_module(
                "dotrix::pbr::solid_shader_module",
                Cow::Borrowed(include_str!("pbr.wgsl")),
            );

            let bind_group_layout =
                gpu.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("dotrix::pbr::bind_group_layout"),
                    entries: &[
                        // Camera Binding
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                                    CameraUniform,
                                >(
                                )
                                    as u64),
                            },
                            count: None,
                        },
                        // Instances Binding
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(self.instances_buffer_size),
                            },
                            count: None,
                        },
                        // Transform Binding
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(self.transform_buffer_size),
                            },
                            count: None,
                        },
                        // Materials Binding
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(self.materials_buffer_size),
                            },
                            count: None,
                        },
                    ],
                });

            let solid_render_pipeline =
                create_solid_render_pipeline(&gpu, &shader_module, &bind_group_layout);

            let camera_mockup = gpu
                .buffer("dotrix::pbr::camera")
                .size(std::mem::size_of::<CameraUniform>() as u64)
                .allow_copy_dst()
                .use_as_uniform()
                .create();

            let camera_uniform = create_camera_mockup();

            gpu.write_buffer(&camera_mockup, 0, bytemuck::cast_slice(&[camera_uniform]));

            let bind_group = gpu.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: camera_mockup.inner.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: instances_buffer.inner.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: transform_buffer.inner.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: materials_buffer.inner.as_entire_binding(),
                    },
                ],
                label: None,
            });

            self.buffers = Some(Buffers {
                mesh: gpu.store(mesh_buffer),
                transform: gpu.store(transform_buffer),
                materials: gpu.store(materials_buffer),
                indirect: gpu.store(indirect_buffer),
                instances: gpu.store(instances_buffer),
                solid_render_pipeline: gpu.store(solid_render_pipeline),
                bind_group: gpu.store(bind_group),
                shader_module: gpu.store(shader_module),
                camera_mockup: gpu.store(camera_mockup),
            });
        }

        self.buffers.as_ref().cloned().unwrap()
    }
}

pub struct RenderPass {
    mesh: Id<Mesh>,
    material: Id<Material>,
}

pub struct BufferLocation {
    offset: u64,
    size: u64,
}

pub struct MeshLayout {
    version: u32,
    vertex_buffer_location: BufferLocation,
    index_buffer_location: Option<BufferLocation>,
    /// Offset of the first model vertex in vertex buffer
    base_vertex: u32,
    /// Number of vertices of the model
    vertex_count: u32,
}

pub struct Render {
    meshes: HashMap<Id<Mesh>, MeshLayout>,
    meshes_layout: Vec<Id<Mesh>>,
    meshes_size: u64,
    transform_bases: HashMap<Id<ecs::Entity>, u32>,
    material_bases: HashMap<Id<Material>, u32>,
    cycle: u64,
}

pub type SolidVertexBufferLayout = (vertex::Position, vertex::Normal, vertex::TexUV);
//pub type SkeletalVertexBufferLayout = (vertex::Position, vertex::Normal, vertex::TexUV);

impl Render {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            meshes_layout: Vec::new(),
            meshes_size: 0,
            transform_bases: HashMap::new(),
            material_bases: HashMap::new(),
            cycle: 0,
        }
    }
}

impl dotrix::Task for Render {
    type Context = (
        dotrix::Any<Buffers>,
        dotrix::Any<gpu::Frame>,
        dotrix::Ref<assets::Assets>,
        dotrix::Ref<ecs::World>,
        dotrix::Ref<gpu::Gpu>,
    );

    type Output = gpu::Commands;

    fn run(&mut self, (buffers, frame, assets, world, gpu): Self::Context) -> Self::Output {
        // TODO: use several maps: static indexed, static non-indexed, skeletal indexed, skeletal
        // non-indexed
        let mut draw_entries = HashMap::<Id<Mesh>, DrawEntry>::new();
        let mesh_buffer = gpu.get(&buffers.mesh).expect("Buffer must exist");
        let transform_buffer = gpu.get(&buffers.transform).expect("Buffer must exist");
        let materials_buffer = gpu.get(&buffers.materials).expect("Buffer must exist");
        let indirect_buffer = gpu.get(&buffers.indirect).expect("Buffer must exist");
        let instances_buffer = gpu.get(&buffers.instances).expect("Buffer must exist");
        // let camera_buffer = gpu.get(&buffers.camera_mockup).expect("Buffer must exist");
        let bind_group = gpu.get(&buffers.bind_group).expect("BindGroup must exist");

        let solid_render_pipeline = gpu
            .get(&buffers.solid_render_pipeline)
            .expect("BindGroup must exist");

        let mut instances = 0;

        for (entity_id, mesh_id, material_id, armature_id, transform) in world.query::<(
            &Id<ecs::Entity>,
            &Id<Mesh>,
            &Id<Material>,
            &Id<Armature>,
            &Transform,
        )>() {
            // Mesh asset must be ready
            let mesh = if let Some(mesh) = assets.get(*mesh_id) {
                mesh
            } else {
                continue;
            };

            // Material asset must be ready
            let material = if let Some(material) = assets.get(*material_id) {
                material
            } else {
                continue;
            };

            // store mesh into buffer
            let (base_vertex, vertex_count) = if let Some(mesh_layout) = self.meshes.get(mesh_id) {
                // TODO: reload ?
                (mesh_layout.base_vertex, mesh_layout.vertex_count)
            } else {
                if mesh.indices::<u8>().is_some() {
                    panic!("Mesh contains indices");
                }

                if let Some(data) = mesh.buffer::<SolidVertexBufferLayout>() {
                    use dotrix_mesh::VertexBufferLayout;

                    let vertex_size = SolidVertexBufferLayout::vertex_size() as u64;
                    let data_size = data.len() as u64;
                    let offset = self.meshes_size;
                    let base_vertex = (offset / vertex_size) as u32;
                    let vertex_count = mesh.count_vertices() as u32;

                    self.meshes.insert(
                        *mesh_id,
                        MeshLayout {
                            version: mesh.version(),
                            vertex_buffer_location: BufferLocation {
                                offset: self.meshes_size,
                                size: data_size,
                            },
                            base_vertex,
                            vertex_count,
                            index_buffer_location: None,
                        },
                    );

                    self.meshes_size += data_size;

                    gpu.write_buffer(mesh_buffer, offset, data.as_slice());
                    // TODO: remove
                    self.meshes_layout.push(*mesh_id);
                    (base_vertex, vertex_count)
                } else {
                    continue;
                }
            };

            // store transformation into buffer
            let transform_bases_len = self.transform_bases.len() as u32;
            let base_transform = *self
                .transform_bases
                .entry(*entity_id)
                .or_insert(transform_bases_len);
            let transform_offset =
                base_transform as u64 * std::mem::size_of::<[[f32; 4]; 4]>() as u64;
            let transform_matrix: [[f32; 4]; 4] = transform.matrix().into();

            gpu.write_buffer(
                transform_buffer,
                transform_offset,
                bytemuck::cast_slice(&transform_matrix),
            );

            // store aterial into buffer
            let material_bases_len = self.material_bases.len() as u32;
            let base_material = *self
                .material_bases
                .entry(*material_id)
                .or_insert(material_bases_len);
            let material_offset =
                base_material as u64 * std::mem::size_of::<MaterialUniform>() as u64;
            let material_uniform = MaterialUniform {
                color: material.albedo.into(),
                options: [
                    material.ambient_occlusion,
                    material.metallic,
                    material.roughness,
                    0.0,
                ],
                maps_1: [material::MAP_DISABLED; 4],
                maps_2: [material::MAP_DISABLED; 4],
            };

            // TODO: do not rewrite same buffer several times
            gpu.write_buffer(
                materials_buffer,
                material_offset,
                bytemuck::cast_slice(&[material_uniform]),
            );

            let draw_entry = draw_entries.entry(*mesh_id).or_insert_with(|| DrawEntry {
                base_vertex,
                vertex_count,
                ..Default::default()
            });

            draw_entry.instances.push(Instance {
                base_transform,
                base_material,
                ..Default::default()
            });
            instances += 1;
        }

        let mut base_instance: u32 = 0;
        let mut instances_buffer_data = Vec::with_capacity(instances);

        let indirect_buffer_data = self
            .meshes_layout
            .iter()
            .map(|mesh_id| draw_entries.get(mesh_id).unwrap())
            // draw_entries
            //   .values()
            .map(|entry| {
                let mut bytes = [0u8; std::mem::size_of::<wgpu::util::DrawIndirect>()];
                let instance_count = entry.instances.len() as u32;
                for instance in entry.instances.iter() {
                    instances_buffer_data.push(instance.clone());
                }
                bytes.copy_from_slice(
                    wgpu::util::DrawIndirect {
                        base_vertex: entry.base_vertex,
                        vertex_count: entry.vertex_count,
                        instance_count,
                        base_instance,
                    }
                    .as_bytes(),
                );
                base_instance += instance_count;
                bytes
            })
            .collect::<Vec<_>>();

        gpu.write_buffer(
            instances_buffer,
            0,
            bytemuck::cast_slice(instances_buffer_data.as_slice()),
        );

        gpu.write_buffer(
            indirect_buffer,
            0,
            bytemuck::cast_slice(indirect_buffer_data.as_slice()),
        );

        let mut encoder = gpu.encoder(Some("dotrix::pbr::solid"));

        {
            let mut rpass = encoder
                .inner
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

            rpass.push_debug_group("dotrix::pbr::solid::set");
            rpass.set_pipeline(&solid_render_pipeline.inner);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.set_vertex_buffer(0, mesh_buffer.inner.slice(..));
            rpass.pop_debug_group();
            rpass.push_debug_group("dotrix::pbr::solid::draw");

            rpass.multi_draw_indirect(&indirect_buffer.inner, 0, indirect_buffer_data.len() as u32);
        }

        encoder.finish(2000)
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Instance {
    base_transform: u32,
    base_material: u32,
    reserve_0: u32,
    reserve_1: u32,
}

unsafe impl bytemuck::Pod for Instance {}
unsafe impl bytemuck::Zeroable for Instance {}

#[derive(Default)]
struct DrawEntry {
    /// Offset of the first model vertex in vertex buffer
    base_vertex: u32,
    /// Number of vertices of the model
    vertex_count: u32,
    /// Instances
    instances: Vec<Instance>,
}

fn create_camera_mockup() -> CameraUniform {
    let fov = 1.1;
    let near_plane = 0.0625;
    let far_plane = 524288.06;
    let position = math::Point3::new(20.0, -30.0, 20.0);
    let target = math::Point3::new(0.0, 0.0, 0.0);

    let proj = math::perspective(math::Rad(fov), 640.0 / 480.0, near_plane, far_plane);
    let view = math::Mat4::look_at_rh(position, target, math::Vec3::new(0.0, 0.0, 1.0));

    CameraUniform {
        proj: proj.into(),
        view: view.into(),
    }
}

fn create_solid_render_pipeline(
    gpu: &gpu::Gpu,
    shader: &gpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> gpu::RenderPipeline {
    use dotrix_mesh::VertexBufferLayout;

    let pipeline_layout = gpu.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("dotrix::pbr::pipeline_layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let vertex_size = SolidVertexBufferLayout::vertex_size();
    let attributes = SolidVertexBufferLayout::attributes()
        .map(
            |(vertex_format, offset, shader_location)| wgpu::VertexAttribute {
                format: gpu::map_vertex_format(vertex_format),
                offset,
                shader_location,
            },
        )
        .collect::<Vec<_>>();

    let vertex_buffer_layout = [wgpu::VertexBufferLayout {
        array_stride: vertex_size as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: attributes.as_slice(),
    }];

    let target = gpu.surface_format();

    gpu.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dotrix::pbr::render_pipeline"),
        layout: Some(&pipeline_layout.inner),
        vertex: wgpu::VertexState {
            module: &shader.inner,
            entry_point: "vs_main_solid",
            buffers: &vertex_buffer_layout,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader.inner,
            entry_point: "fs_main",
            targets: &[Some(target.into())],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            //topology: wgpu::PrimitiveTopology::PointList,
            //polygon_mode: wgpu::PolygonMode::Point,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    })
}

pub struct Extension {
    pub mesh_buffer_size: u64,
    pub transform_buffer_size: u64,
    pub indirect_buffer_size: u64,
    pub instances_buffer_size: u64,
    pub materials_buffer_size: u64,
}

impl Default for Extension {
    fn default() -> Self {
        Self {
            mesh_buffer_size: DEAFULT_MESH_BUFFER_SIZE,
            transform_buffer_size: DEAFULT_TRANSFORM_BUFFER_SIZE,
            indirect_buffer_size: DEAFULT_INDIRECT_BUFFER_SIZE,
            instances_buffer_size: DEAFULT_INSTANCES_BUFFER_SIZE,
            materials_buffer_size: DEAFULT_MATERIALS_BUFFER_SIZE,
        }
    }
}

impl dotrix::Extension for Extension {
    fn add_to(&self, manager: &mut dotrix::Manager) {
        let allocator = Allocator::new(
            self.mesh_buffer_size,
            self.transform_buffer_size,
            self.indirect_buffer_size,
            self.instances_buffer_size,
            self.materials_buffer_size,
        );
        let render = Render::new();

        manager.schedule(allocator);
        manager.schedule(render);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
