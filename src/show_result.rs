use bytemuck;
use std::{
    iter,
    sync::{Arc, mpsc::Sender},
};

use rand::Rng;
use wesl::include_wesl;
use wgpu::{Buffer, Device, Queue, util::DeviceExt};
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::ActiveEventLoop,
    window::{Window},
};

#[derive(Clone, Debug)]
pub struct DataBundle {
    pub(crate) data: Vec<i8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug)]
pub struct BufferBundle {
    pub(crate) buffer: Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug)]
pub enum UserEvent {
    CreateDevice(Sender<(Device, Queue)>),
    UpdateBuffer(BufferBundle),
}
// uniform buffers need to be 16 byte aligned. the fields are not necessary, but are more obvious
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct Vertex {
    corner: [f32; 2], // x: along segment (-1..1), y: offset (-1..1)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct Instance {
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct ScreenUniform {
    size: [f32; 2], // width, height in pixels
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
struct TextureDimsUniform {
    dims: [f32; 2], // width, height in pixels
    _pad: [f32; 2],
}

pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    is_surface_configured: bool,

    window: Arc<Window>,

    screen_ubo: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,

    input_bind_group: Option<wgpu::BindGroup>,

    render_pipeline: wgpu::RenderPipeline,
}

impl State {
    async fn new(window: Arc<Window>, sender: Sender<(Device, Queue)>) -> anyhow::Result<State> {
        let win_size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        sender.send((device.clone(), queue.clone())).unwrap();

        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            // Request compatibility with the sRGB-format texture view we‘re going to create later.
            view_formats: vec![surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: win_size.width,
            height: win_size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        surface.configure(&device, &surface_config);

        // Vertex buffer layouts
        // getting the screen
        let screen = ScreenUniform {
            size: [win_size.width as f32, win_size.height as f32],
            _pad: [0.0, 0.0],
        };
        let screen_ubo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("screen ubo"),
            contents: bytemuck::bytes_of(&screen),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Bind group for screen uniform
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("screen bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen bind group"),
            layout: &screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_ubo.as_entire_binding(),
            }],
        });

        let shader_string = include_wesl!("compute_to_render");
        let shader_source = wgpu::ShaderSource::Wgsl(shader_string.into());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Show Result Shader"),
            source: shader_source,
        });

        let input_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("input_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None, // or Some(NonZeroU64::new(labels_size).unwrap())
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&screen_bind_group_layout, &input_bind_group_layout],
                immediate_size: 0,
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface.get_configuration().unwrap().format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // indicates how many array layers the attachments will have.
            multiview_mask: None,
            // Useful for optimizing shader compilation on Android
            cache: None,
        });

        /*
        let dims = TextureDimsUniform {
            dims: [buffer_bundle.width as f32, buffer_bundle.height as f32],
            _pad: [0.0, 0.0],
        };
        let dims_ubo = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dims ubo"),
            contents: bytemuck::bytes_of(&dims),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });


        let input_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("input_bind_group"),
            layout: &input_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer_bundle.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dims_ubo.as_entire_binding(),
                },
            ],
        });
        */

        Ok(Self {
            surface,
            device,
            queue,
            is_surface_configured: false,
            window,

            screen_ubo,
            screen_bind_group,

            input_bind_group: None,

            render_pipeline,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.is_surface_configured = true;
            let mut config = self.surface.get_configuration().unwrap();
            config.width = width;
            config.height = height;
            self.surface.configure(&self.device, &config);
            let screen = ScreenUniform {
                size: [width as f32, height as f32],
                _pad: [0.0, 0.0],
            };
            self.queue
                .write_buffer(&self.screen_ubo, 0, bytemuck::bytes_of(&screen));
        }
    }
    pub fn change_buffer(&mut self, buffer_bundle: BufferBundle) {
        let dims = TextureDimsUniform {
            dims: [buffer_bundle.width as f32, buffer_bundle.height as f32],
            _pad: [0.0, 0.0],
        };
        let dims_ubo = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("screen ubo"),
                contents: bytemuck::bytes_of(&dims),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        self.input_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("input_bind_group"),
            // TODO carefull with hard coded indices
            layout: &self.render_pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer_bundle.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dims_ubo.as_entire_binding(),
                },
            ],
        }));
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.window.request_redraw();

        // We can't render unless the surface is configured
        if !self.is_surface_configured {
            return Ok(());
        }
        let Some(input_bind_group) = &self.input_bind_group else {
            return Ok(());
        };

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.3,
                            g: 0.3,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.screen_bind_group, &[]);
            render_pass.set_bind_group(1, input_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

pub struct App {
    state: Option<State>,
}

impl App {
    pub fn new() -> Self {
        Self { state: None }
    }
}

// kann ich irgendwie in state userevent reinpacken?
// wenn es ein feld von state ist, würde es nicht direkt übernommen werden
// wie wird state hier weiter gegeben?
impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    #[allow(unused_mut)]
    fn user_event(&mut self, event_loop: &ActiveEventLoop, mut event: UserEvent) {
        match event {
            UserEvent::CreateDevice(sender) => {
                // this might need to be in resumed??
                // get window
                let window = Arc::new(
                    event_loop
                        .create_window(Window::default_attributes())
                        .unwrap(),
                );
                self.state = Some(pollster::block_on(State::new(window, sender)).unwrap());
            }
            UserEvent::UpdateBuffer(buffer_bundle) => {
                if let Some(state) = self.state.as_mut() {
                    state.change_buffer(buffer_bundle);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                // TODO if state
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if it's lost or outdated
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.window.inner_size();
                        state.resize(size.width, size.height);
                    }
                    Err(e) => {
                        log::error!("Unable to render {e}");
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn random_buffer_bundle() -> DataBundle {
    let mut rng = rand::rng();
    let mut data = vec![];
    let height = rng.random::<u32>() % 100 + 1;
    let width = rng.random::<u32>() % 100 + 1;
    for _ in 0..height {
        for _ in 0..width {
            data.push(rng.random::<i8>() % 2);
        }
    }
    DataBundle {
        data,
        width,
        height,
    }
}

// struct for buffer dims+data

// fn for randomizing data in a buffer
