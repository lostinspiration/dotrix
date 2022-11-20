mod buffer;
mod pipeline;
mod shader;

use std::any::Any;
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use dotrix_core as dotrix;
use dotrix_log as log;
use dotrix_types as types;
use dotrix_window as window;

use types::vertex;
use types::Id;

pub use buffer::Buffer;
pub use pipeline::{PipelineLayout, RenderPipeline};
pub use shader::ShaderModule;

pub use wgpu as backend;

const FPS_MEASURE_INTERVAL: u32 = 5; // seconds

pub struct Descriptor<'a> {
    pub window_handle: &'a window::Handle,
    pub fps_request: f32,
    pub surface_size: [u32; 2],
    pub sample_count: u32,
}

pub struct Gpu {
    /// Desired FPS
    fps_request: f32,
    /// Sample Count
    sample_count: u32,
    /// Log of frames duration
    frames_duration: VecDeque<Duration>,
    /// Last frame timestamp
    last_frame: Option<Instant>,
    /// Real fps
    fps: f32,
    /// WGPU Adapter
    adapter: wgpu::Adapter,
    /// WGPU Device
    device: wgpu::Device,
    /// WGPU Queue
    queue: wgpu::Queue,
    /// WGPU Surface
    surface: wgpu::Surface,
    /// WGPU surface configuration
    surface_conf: wgpu::SurfaceConfiguration,
    /// Surface resize request
    resize_request: Option<[u32; 2]>,
    /// Storage for GPU related objects: Buffers, Textures, Shaders, Pipelines, etc
    storage: HashMap<uuid::Uuid, Box<dyn Any>>,
}

pub struct Frame {
    pub inner: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
    pub delta: std::time::Duration,
    pub instant: std::time::Instant,
}

pub struct CommandEncoder {
    pub inner: wgpu::CommandEncoder,
}

impl CommandEncoder {
    pub fn finish(mut self, priority: u32) -> Commands {
        Commands {
            inner: self.inner.finish(),
            priority,
        }
    }
}

pub struct Commands {
    pub priority: u32,
    pub inner: wgpu::CommandBuffer,
}

pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// Submit Report
///
/// As a task output identifies, that commands queue was executed
pub struct Submit {
    /// How long did it take to prepare the frame
    pub duration: Duration,
}

impl Gpu {
    pub fn new(descriptor: Descriptor) -> Self {
        let (adapter, device, queue, surface) =
            futures::executor::block_on(init(descriptor.window_handle));

        let surface_conf = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_supported_formats(&adapter)[0],
            width: descriptor.surface_size[0],
            height: descriptor.surface_size[1],
            present_mode: wgpu::PresentMode::Mailbox,
        };

        surface.configure(&device, &surface_conf);
        let sample_count = descriptor.sample_count;
        let fps_request = descriptor.fps_request;
        let frame_duration = Duration::from_secs_f32(1.0 / fps_request);
        let fps_samples = (FPS_MEASURE_INTERVAL * fps_request.ceil() as u32) as usize;
        let mut frames_duration = VecDeque::with_capacity(fps_samples);

        Self {
            fps_request,
            sample_count,
            frames_duration,
            fps: fps_request,
            last_frame: None,
            adapter,
            device,
            queue,
            surface,
            surface_conf,
            resize_request: None,
            storage: HashMap::new(),
        }
    }

    pub fn store<T: Any>(&mut self, data: T) -> Id<T> {
        let raw_id = uuid::Uuid::new_v4();
        self.storage.insert(raw_id, Box::new(data));
        Id::from(raw_id)
    }

    pub fn store_as<T: Any>(&mut self, id: Id<T>, data: T) {
        self.storage.insert(id.uuid().clone(), Box::new(data));
    }

    pub fn get<T: Any>(&self, id: &Id<T>) -> Option<&T> {
        self.storage
            .get(id.uuid())
            .and_then(|data| data.downcast_ref::<T>())
    }

    pub fn get_mut<T: Any>(&mut self, id: &Id<T>) -> Option<&mut T> {
        self.storage
            .get_mut(id.uuid())
            .and_then(|data| data.downcast_mut::<T>())
    }

    pub fn buffer<'a, 'b>(&'a self, label: &'b str) -> buffer::Builder<'a, 'b> {
        buffer::Builder {
            gpu: self,
            descriptor: wgpu::BufferDescriptor {
                label: Some(label),
                usage: wgpu::BufferUsages::empty(),
                size: 0,
                mapped_at_creation: false,
            },
        }
    }

    pub fn create_buffer(&self, desc: &wgpu::BufferDescriptor) -> Buffer {
        Buffer {
            inner: self.device.create_buffer(desc),
        }
    }

    pub fn write_buffer(&self, buffer: &Buffer, offset: u64, data: &[u8]) {
        self.queue.write_buffer(&buffer.inner, offset, data);
    }

    pub fn write_buffer_by_id(&self, id: &Id<Buffer>, offset: u64, data: &[u8]) {
        if let Some(buffer) = self.get(id) {
            self.queue.write_buffer(&buffer.inner, offset, data);
        }
    }

    pub fn create_bind_group_layout(
        &self,
        desc: &wgpu::BindGroupLayoutDescriptor,
    ) -> wgpu::BindGroupLayout {
        self.device.create_bind_group_layout(desc)
    }

    pub fn create_bind_group(&self, desc: &wgpu::BindGroupDescriptor) -> wgpu::BindGroup {
        self.device.create_bind_group(&desc)
    }

    pub fn create_pipeline_layout(&self, desc: &wgpu::PipelineLayoutDescriptor) -> PipelineLayout {
        PipelineLayout {
            inner: self.device.create_pipeline_layout(desc),
        }
    }

    pub fn create_render_pipeline(&self, desc: &wgpu::RenderPipelineDescriptor) -> RenderPipeline {
        RenderPipeline {
            inner: self.device.create_render_pipeline(desc),
        }
    }

    pub fn create_shader_module(&self, name: &str, source: Cow<str>) -> ShaderModule {
        ShaderModule {
            inner: self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(name),
                    source: wgpu::ShaderSource::Wgsl(source),
                }),
        }
    }

    pub fn encoder(&self, label: Option<&str>) -> CommandEncoder {
        let command_encoder_descriptor = wgpu::CommandEncoderDescriptor { label };
        CommandEncoder {
            inner: self
                .device
                .create_command_encoder(&command_encoder_descriptor),
        }
    }

    pub fn resize_request(&mut self, width: u32, height: u32) {
        self.resize_request = Some([width, height]);
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_conf.format
    }
}

pub fn map_vertex_format(attr_format: vertex::AttributeFormat) -> wgpu::VertexFormat {
    match attr_format {
        vertex::AttributeFormat::Float32 => wgpu::VertexFormat::Float32,
        vertex::AttributeFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        vertex::AttributeFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        vertex::AttributeFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
        vertex::AttributeFormat::Uint16x2 => wgpu::VertexFormat::Uint16x2,
        vertex::AttributeFormat::Uint16x4 => wgpu::VertexFormat::Uint16x4,
        vertex::AttributeFormat::Uint32 => wgpu::VertexFormat::Uint32,
        vertex::AttributeFormat::Uint32x2 => wgpu::VertexFormat::Float32x2,
        vertex::AttributeFormat::Uint32x3 => wgpu::VertexFormat::Float32x3,
        vertex::AttributeFormat::Uint32x4 => wgpu::VertexFormat::Float32x4,
    }
}

impl Frame {
    pub fn delta(&self) -> Duration {
        self.delta
    }
}

#[derive(Default)]
pub struct CreateFrame;

impl dotrix::Task for CreateFrame {
    type Context = (dotrix::Mut<Gpu>,);
    type Output = Frame;

    fn run(&mut self, (mut renderer,): Self::Context) -> Self::Output {
        let delta = renderer
            .last_frame
            .replace(Instant::now())
            .map(|i| i.elapsed())
            .unwrap_or_else(|| Duration::from_secs_f32(1.0 / renderer.fps_request));

        if renderer.frames_duration.len() == renderer.frames_duration.capacity() {
            renderer.frames_duration.pop_back();
        }

        renderer.frames_duration.push_front(delta);

        let frames = renderer.frames_duration.len() as f32;
        let duration: f32 = renderer
            .frames_duration
            .iter()
            .map(|d| d.as_secs_f32())
            .sum();
        let fps = frames / duration;

        renderer.fps = fps;

        if let Some(resize_request) = renderer.resize_request.take() {
            let [width, height] = resize_request;
            if width > 0 && height > 0 {
                renderer.surface_conf.width = width;
                renderer.surface_conf.height = height;
                renderer
                    .surface
                    .configure(&renderer.device, &renderer.surface_conf);
            }
        }

        let wgpu_frame = match renderer.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                renderer
                    .surface
                    .configure(&renderer.device, &renderer.surface_conf);
                renderer
                    .surface
                    .get_current_texture()
                    .expect("Failed to acquire next surface texture")
            }
        };

        let view = wgpu_frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        Frame {
            inner: wgpu_frame,
            view,
            delta,
            instant: Instant::now(),
        }
    }
}

unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

#[derive(Default)]
pub struct ResizeSurface;

impl dotrix::Task for ResizeSurface {
    type Context = (dotrix::Take<SurfaceSize>, dotrix::Mut<Gpu>);
    type Output = ();

    fn run(&mut self, (surface_size, mut renderer): Self::Context) -> Self::Output {
        log::info!(
            "create surface resize request for: {}x{}",
            surface_size.width,
            surface_size.height
        );
        renderer.resize_request = Some([surface_size.width, surface_size.height]);
    }
}

pub struct ClearFrame {
    color: types::Color,
}

impl Default for ClearFrame {
    fn default() -> Self {
        Self {
            color: types::Color::black(),
        }
    }
}

impl dotrix::Task for ClearFrame {
    type Context = (dotrix::Any<Frame>, dotrix::Ref<Gpu>);
    // The task uses itself as output as a zero-cost abstraction
    type Output = Commands;
    fn run(&mut self, (frame, renderer): Self::Context) -> Self::Output {
        let mut encoder = renderer.encoder(Some("dotrix::gpu::clear_frame"));
        encoder
            .inner
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.color.r as f64,
                            g: self.color.g as f64,
                            b: self.color.b as f64,
                            a: self.color.a as f64,
                        }),
                        store: true,
                    },
                })],
                // We still need to use the depth buffer here
                // since the pipeline requires it.
                depth_stencil_attachment: None,
            });

        encoder.finish(1000)
    }
}

#[derive(Default)]
pub struct SubmitCommands;

impl dotrix::Task for SubmitCommands {
    type Context = (
        dotrix::Any<Frame>,
        dotrix::Collect<Commands>,
        dotrix::Ref<Gpu>,
    );
    // The task uses itself as output as a zero-cost abstraction
    type Output = SubmitCommands;
    fn run(&mut self, (_, commands, renderer): Self::Context) -> Self::Output {
        let mut commands = commands.collect();

        commands.sort_by(|a, b| a.priority.cmp(&b.priority));

        let index = renderer.queue.submit(commands.into_iter().map(|c| c.inner));

        while !renderer
            .device
            .poll(wgpu::Maintain::WaitForSubmissionIndex(index))
        {}
        SubmitCommands
    }
}

#[derive(Default)]
pub struct PresentFrame;

impl dotrix::Task for PresentFrame {
    type Context = (dotrix::Take<Frame>, dotrix::Take<SubmitCommands>);

    type Output = PresentFrame;

    fn output_channel(&self) -> dotrix::task::OutputChannel {
        dotrix::task::OutputChannel::Scheduler
    }

    fn run(&mut self, (frame, _): Self::Context) -> Self::Output {
        frame.unwrap().inner.present();
        PresentFrame
    }
}

async fn init(
    window_handle: &window::Handle,
) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue, wgpu::Surface) {
    let backend = wgpu::util::backend_bits_from_env().unwrap_or_else(wgpu::Backends::all);

    let instance = wgpu::Instance::new(backend);
    let surface = unsafe { instance.create_surface(&window_handle) };
    let adapter =
        wgpu::util::initialize_adapter_from_env_or_default(&instance, backend, Some(&surface))
            .await
            .expect("No suitable GPU adapters found on the system!");

    #[cfg(not(target_arch = "wasm32"))]
    {
        let adapter_info = adapter.get_info();
        log::info!("Adapter: {}", adapter_info.name);
        log::info!("Backend: {:?}", adapter_info.backend);
    }

    // TODO: implement features control
    let optional_features = wgpu::Features::empty();
    let required_features =
        wgpu::Features::MULTI_DRAW_INDIRECT | wgpu::Features::INDIRECT_FIRST_INSTANCE;
    let adapter_features = adapter.features();
    assert!(
        adapter_features.contains(required_features),
        "Not supported: {:?}",
        required_features - adapter_features
    );

    let required_downlevel_capabilities = wgpu::DownlevelCapabilities {
        flags: wgpu::DownlevelFlags::empty(),
        shader_model: wgpu::ShaderModel::Sm5,
        ..wgpu::DownlevelCapabilities::default()
    };
    let downlevel_capabilities = adapter.get_downlevel_capabilities();
    assert!(
        downlevel_capabilities.shader_model >= required_downlevel_capabilities.shader_model,
        "Shader model {:?} requiered, but {:?} supported ",
        required_downlevel_capabilities.shader_model,
        downlevel_capabilities.shader_model,
    );
    assert!(
        downlevel_capabilities
            .flags
            .contains(required_downlevel_capabilities.flags),
        "Adapter does not support the downlevel capabilities required to run: {:?}",
        required_downlevel_capabilities.flags - downlevel_capabilities.flags
    );

    // Make sure we use the texture resolution limits from the adapter, so we can support images
    // the size of the surface.
    let mut gpu_limits =
        wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
    gpu_limits.max_storage_buffers_per_shader_stage = 5;
    gpu_limits.max_storage_buffer_binding_size = 1 * 1024 * 1024 * 1024;

    let trace_dir = std::env::var("WGPU_TRACE");
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: (optional_features & adapter_features) | required_features,
                limits: gpu_limits,
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        )
        .await
        .expect("Unable to find a suitable GPU adapter!");

    (adapter, device, queue, surface)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
