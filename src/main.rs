use std::ops::SubAssign;

use anyhow::Context as _;
use bytemuck::{Pod, Zeroable};
use raw_window_handle::HasRawWindowHandle;
use wgpu::util::{BufferInitDescriptor, DeviceExt as _};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{
        ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
        WindowEvent,
    },
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    platform::unix::{WindowBuilderExtUnix as _, WindowExtUnix as _},
    window::WindowBuilder,
};
use x11_dl::xlib::Xlib;

fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    pollster::block_on(main_async())
}

async fn main_async() -> anyhow::Result<()> {
    let colors = [
        Vec4F32([1.0, 0.0, 0.0, 1.0]),
        Vec4F32([0.0, 1.0, 0.0, 1.0]),
        Vec4F32([0.0, 0.0, 1.0, 1.0]),
        Vec4F32([1.0, 1.0, 0.0, 1.0]),
        Vec4F32([0.0, 1.0, 1.0, 1.0]),
        Vec4F32([1.0, 0.0, 1.0, 1.0]),
        Vec4F32([1.0, 1.0, 1.0, 1.0]),
        Vec4F32([f32::INFINITY, f32::INFINITY, f32::INFINITY, 1.0]),
    ];
    let mut color = 0;

    let event_loop = EventLoop::new();

    let wallpaper = std::env::args().nth(1).as_deref() == Some("wallpaper");

    let window = if wallpaper {
        let xlib = Xlib::open().context("could not open Xlib")?;

        let (position, size) = total_screen(&event_loop).context("no monitors")?;

        let window = WindowBuilder::new()
            .with_override_redirect(true)
            .with_position(position)
            .with_inner_size(size)
            .build(&event_loop)
            .context("failed to build wallpaper window")?;

        window.set_maximized(true);

        if let (Some(x_display), Some(x_window)) = (window.xlib_display(), window.xlib_window()) {
            unsafe {
                (xlib.XLowerWindow)(x_display.cast(), x_window);
            }
        }

        window
    } else {
        WindowBuilder::new()
            .with_title("Mandelbrot Set")
            .build(&event_loop)
            .context("failed to build window")?
    };

    let mut renderer = Renderer::new(&window, window.inner_size().into(), colors[color]).await?;

    let mut ctrl_key = false;
    let mut dragging_from: Option<PhysicalPosition<f64>> = None;
    let mut mouse_pos: Option<PhysicalPosition<f64>> = None;
    let mut left_mouse = ElementState::Released;
    let mut right_mouse = ElementState::Released;

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            ref event,
            window_id,
        } if window_id == window.id() => match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::Escape | VirtualKeyCode::Q),
                        ..
                    },
                ..
            } if !wallpaper => *control_flow = ControlFlow::Exit,
            &WindowEvent::Resized(size) => renderer.resize(size.into()),
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                renderer.resize((**new_inner_size).into())
            }
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(VirtualKeyCode::R),
                        ..
                    },
                ..
            }
            | WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state: ElementState::Pressed,
                ..
            } => renderer.reset(),
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(key @ (VirtualKeyCode::Up | VirtualKeyCode::Down)),
                        ..
                    },
                ..
            } => {
                let mut delta = if ctrl_key { 10 } else { 1 };
                if *key == VirtualKeyCode::Down {
                    delta *= -1;
                }
                renderer.change_precision(delta);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if left_mouse == ElementState::Pressed {
                    if let Some(previous) = dragging_from {
                        renderer.change_offset(Vec2F32([
                            (position.x - previous.x) as f32,
                            (position.y - previous.y) as f32,
                        ]));
                    }
                    dragging_from = Some(*position);
                } else {
                    dragging_from = None;
                }
                mouse_pos = Some(*position);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if right_mouse == ElementState::Pressed {
                    let multiplier = if ctrl_key { 10 } else { 1 };
                    let change = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y as i32,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as i32,
                    };
                    renderer.change_precision(change * multiplier);
                } else {
                    let multiplier = if ctrl_key { 3.0 } else { 1.0 };

                    if let Some(mouse_pos) = mouse_pos {
                        renderer.change_scale(
                            match delta {
                                MouseScrollDelta::LineDelta(_, y) => *y * 100.0,
                                MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                            } * multiplier,
                            mouse_pos.x as f32,
                            mouse_pos.y as f32,
                        );
                    }
                }
            }
            WindowEvent::ModifiersChanged(state) => {
                ctrl_key = state.ctrl();
            }
            WindowEvent::MouseInput { button, state, .. } => {
                match button {
                    MouseButton::Left => left_mouse = *state,
                    MouseButton::Right => right_mouse = *state,
                    _ => {}
                }
                if *button == MouseButton::Left
                    && *state == ElementState::Pressed
                    && right_mouse == ElementState::Pressed
                {
                    color = (color + 1) % colors.len();
                    renderer.set_color(colors[color]);
                }
            }
            _ => {}
        },
        Event::RedrawRequested(_) => match renderer.render() {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost) => renderer.reconfigure(),
            Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
            Err(e) => {
                log::error!("render error: {}", e);
            }
        },
        Event::MainEventsCleared => window.request_redraw(),
        _ => {}
    })
}

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C, align(8))]
struct Vec2U32([u32; 2]);

impl Vec2U32 {
    fn x(self) -> u32 {
        self.0[0]
    }
    fn y(self) -> u32 {
        self.0[1]
    }
}

impl From<PhysicalSize<u32>> for Vec2U32 {
    fn from(size: PhysicalSize<u32>) -> Self {
        Self([size.width, size.height])
    }
}

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C, align(8))]
struct Vec2F32([f32; 2]);

impl Vec2F32 {
    fn x_mut(&mut self) -> &mut f32 {
        &mut self.0[0]
    }
    fn y_mut(&mut self) -> &mut f32 {
        &mut self.0[1]
    }
}

impl SubAssign<Vec2F32> for Vec2F32 {
    fn sub_assign(&mut self, rhs: Vec2F32) {
        self.0[0] -= rhs.0[0];
        self.0[1] -= rhs.0[1];
    }
}

#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C, align(16))]
struct Vec4F32([f32; 4]);

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Params {
    size: Vec2U32,
    offset: Vec2F32,
    scale: f32,
    max_iterations: u32,
    _padding: [u32; 2],
    color: Vec4F32,
}

impl Params {
    fn new(size: Vec2U32, color: Vec4F32) -> Self {
        Self {
            size,
            offset: Vec2F32::default(),
            scale: 1.0 / 400.0,
            max_iterations: 50,
            _padding: [0; 2],
            color,
        }
    }
}

const PARAMS_BIND_GROUP: u32 = 0;
const PARAMS_BINDING: u32 = 0;

struct Renderer {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    params: Params,
}

impl Renderer {
    async fn new(
        window: &impl HasRawWindowHandle,
        size: Vec2U32,
        color: Vec4F32,
    ) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("could not find a valid adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .context("could not find a valid device")?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface
                .get_preferred_format(&adapter)
                .context("could not retrieve surface's preferred format")?,
            width: size.x(),
            height: size.y(),
            present_mode: wgpu::PresentMode::Fifo,
        };

        let shader_src = include_str!("shader.wgsl");
        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let [vertex_shader, fragment_shader] = [&shader; 2];

        let params = Params::new(size, color);

        let params_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("params buffer"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let params_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("params bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: PARAMS_BINDING,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("params bind group"),
            layout: &params_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: PARAMS_BINDING,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render pipeline layout"),
                bind_group_layouts: &[&params_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: vertex_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: fragment_shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLAMPING
                clamp_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        let mut this = Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            params_buffer,
            params_bind_group,
            params,
        };

        this.reconfigure();

        Ok(this)
    }

    fn resize(&mut self, new_size: Vec2U32) {
        if new_size.x() == 0 || new_size.y() == 0 {
            return;
        }

        self.config.width = new_size.x();
        self.config.height = new_size.y();
        self.reconfigure();

        self.params.size = new_size;
        self.update_params();
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn change_precision(&mut self, by: i32) {
        self.params.max_iterations =
            u32::try_from(self.params.max_iterations as i32 + by).unwrap_or(0);
        self.update_params();
    }

    fn change_offset(&mut self, by: Vec2F32) {
        self.params.offset -= by;
        self.update_params();
    }

    fn change_scale(&mut self, by: f32, x: f32, y: f32) {
        fn update_offset(
            offset: &mut f32,
            old_scale: f32,
            new_scale: f32,
            position: f32,
            size: u32,
        ) {
            let half_size = size as f32 / 2.0;
            *offset =
                old_scale * (*offset + position - half_size) / new_scale - position + half_size;
        }

        let old_scale = self.params.scale;
        let new_scale = old_scale * (-by / 1000.0).exp2();
        let wgpu::SurfaceConfiguration { width, height, .. } = &self.config;
        update_offset(self.params.offset.x_mut(), old_scale, new_scale, x, *width);
        update_offset(self.params.offset.y_mut(), old_scale, new_scale, y, *height);
        self.params.scale = new_scale;
        self.update_params();
    }

    fn reset(&mut self) {
        self.params = Params::new(self.params.size, self.params.color);
        self.update_params();
    }

    fn set_color(&mut self, color: Vec4F32) {
        self.params.color = color;
        self.update_params();
    }

    fn update_params(&mut self) {
        self.queue
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&self.params));
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(PARAMS_BIND_GROUP, &self.params_bind_group, &[]);
            render_pass.draw(0..self.config.width * self.config.height, 0..1);
        }

        self.queue.submit([encoder.finish()]);

        output.present();

        Ok(())
    }
}

fn total_screen<T>(
    target: &EventLoopWindowTarget<T>,
) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>)> {
    let left = target.available_monitors().map(|m| m.position().x).min()?;
    let right = target
        .available_monitors()
        .map(|m| m.position().x + m.size().width as i32)
        .max()?;
    let top = target.available_monitors().map(|m| m.position().y).min()?;
    let bottom = target
        .available_monitors()
        .map(|m| m.position().y + m.size().height as i32)
        .max()?;
    Some((
        PhysicalPosition::new(left, top),
        PhysicalSize::new((right - left) as u32, (bottom - top) as u32),
    ))
}
