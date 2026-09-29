//! The Linux viewer's picture: decoded frames on the GPU (wgpu: Vulkan, or
//! OpenGL where there's no Vulkan), converted from YUV in a shader, fitted to
//! the window like the Mac's layer presenter; fast-lane tiles on top until a
//! newer video frame retires them; and, over everything, the stats bar,
//! notices, the ••• button and the session menu (egui).

use std::collections::VecDeque;
use std::sync::Arc;

use sunna_capture::PixelFormat;
use sunna_codec::{DecodedFrame, Matrix};
use sunna_proto::tiles::TileBatch;
use winit::event::WindowEvent;
use egui_wgpu::wgpu;
use winit::window::Window;

use crate::menu_model::{Codec, MenuAction, MenuEvent, MenuState, BITRATES_MBPS, FRAME_RATES, SCALES};
use crate::viewer::stream_scale;

/// Tiles kept on screen at most (a burst beyond this drops the oldest).
const MAX_TILES: usize = 512;
/// The menu button: from the window's top left, in points (as on the Mac).
pub const BUTTON_ORIGIN: (f32, f32) = (14.0, 14.0);
pub const BUTTON_SIZE: (f32, f32) = (36.0, 24.0);

const SHADER: &str = r#"
struct Params {
    // Where the picture goes, in clip space: left, top, right, bottom.
    rect: vec4<f32>,
    // 0 NV12, 1 I420, 2 BGRA.
    kind: u32,
    full_range: u32,
    // 0 BT.601, 1 BT.709, 2 BT.2020.
    matrix: u32,
    // Write linear light (the target is an sRGB format).
    linear_out: u32,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var plane0: texture_2d<f32>;
@group(0) @binding(2) var plane1: texture_2d<f32>;
@group(0) @binding(3) var plane2: texture_2d<f32>;
@group(0) @binding(4) var pick: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let corner = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    var o: Out;
    o.pos = vec4<f32>(mix(p.rect.x, p.rect.z, corner.x), mix(p.rect.y, p.rect.w, corner.y), 0.0, 1.0);
    o.uv = corner;
    return o;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    var rgb: vec3<f32>;
    if (p.kind == 2u) {
        rgb = textureSample(plane0, pick, in.uv).rgb;
    } else {
        var y = textureSample(plane0, pick, in.uv).r;
        var uv: vec2<f32>;
        if (p.kind == 0u) {
            uv = textureSample(plane1, pick, in.uv).rg;
        } else {
            uv = vec2<f32>(textureSample(plane1, pick, in.uv).r, textureSample(plane2, pick, in.uv).r);
        }
        if (p.full_range == 1u) {
            uv = uv - vec2<f32>(128.0 / 255.0);
        } else {
            y = (y - 16.0 / 255.0) * (255.0 / 219.0);
            uv = (uv - vec2<f32>(128.0 / 255.0)) * (255.0 / 224.0);
        }
        // Kr, Kb per matrix: R = Y + a·V, G = Y − b·U − c·V, B = Y + d·U.
        var k = vec4<f32>(1.5748, 0.1873, 0.4681, 1.8556);
        if (p.matrix == 0u) { k = vec4<f32>(1.402, 0.344136, 0.714136, 1.772); }
        if (p.matrix == 2u) { k = vec4<f32>(1.4746, 0.16455, 0.57135, 1.8814); }
        rgb = vec3<f32>(y + k.x * uv.y, y - k.y * uv.x - k.z * uv.y, y + k.w * uv.x);
    }
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    if (p.linear_out == 1u) { rgb = to_linear(rgb); }
    return vec4<f32>(rgb, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    rect: [f32; 4],
    layout: u32,
    full_range: u32,
    matrix: u32,
    linear_out: u32,
}

/// One picture's GPU planes and the bind group that reads them.
struct Planes {
    size: (u32, u32),
    format: PixelFormat,
    textures: Vec<wgpu::Texture>,
    bind: wgpu::BindGroup,
    params: wgpu::Buffer,
}

/// A fast-lane tile waiting for a video frame to catch up with it.
struct TileQuad {
    capture_ts_us: u64,
    rect: (u32, u32, u32, u32),
    planes: Planes,
}

/// What the overlay shows, and what it has to say back.
#[derive(Default)]
struct Overlay {
    stats: Option<String>,
    notice: Option<String>,
    button_visible: bool,
    menu: Option<MenuState>,
    /// The menu opened on this frame's click: that click isn't a click away.
    fresh: bool,
    /// The submenu showing, and the top of its row.
    sub: Option<Submenu>,
    sub_top: f32,
    events: Vec<MenuEvent>,
}

pub struct GpuPresenter {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    smooth: wgpu::Sampler,
    linear_out: bool,
    video: Option<Planes>,
    color: sunna_codec::Color,
    video_ts_us: Option<u64>,
    tiles: VecDeque<TileQuad>,
    stream_size: (u32, u32),
    egui: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    overlay: Overlay,
    /// The pointer is over the menu button.
    button_hover: bool,
}

impl GpuPresenter {
    pub fn new(window: Arc<Window>, stream_size: (u32, u32)) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let info = adapter.get_info();
        let backend = format!("{:?} · {}", info.backend, info.name);
        tracing::info!(backend = %backend, "GPU presenter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("sunna viewer"),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("this GPU can't draw to the window"))?;
        let caps = surface.get_capabilities(&adapter);
        // A plain (non-sRGB) 8-bit format: the video's values go straight
        // to the screen. With only sRGB formats, the shader writes linear.
        let plain = caps.formats.iter().copied().find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm));
        config.format = plain.unwrap_or(config.format);
        let linear_out = config.format.is_srgb();
        // Show each frame as it arrives; wait for the display only if nothing else works.
        config.present_mode = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo]
            .into_iter()
            .find(|mode| caps.present_modes.contains(mode))
            .unwrap_or(config.present_mode);
        config.desired_maximum_frame_latency = 1;
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&device, &config);
        tracing::info!(format = ?config.format, present = ?config.present_mode, "surface");

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("video"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                texture_entry(1),
                texture_entry(2),
                texture_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("video"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("video"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("video"),
                mag_filter: filter,
                min_filter: filter,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            })
        };
        // Linear filtering: exact at 1:1 (texel centres line up with pixel
        // centres), smooth when the picture is scaled.
        let smooth = sampler(wgpu::FilterMode::Linear);

        let egui = egui::Context::default();
        style(&egui);
        let egui_state = egui_winit::State::new(egui.clone(), egui::ViewportId::ROOT, &window, Some(window.scale_factor() as f32), None, None);
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            layout,
            pipeline,
            smooth,
            linear_out,
            video: None,
            color: Default::default(),
            video_ts_us: None,
            tiles: VecDeque::new(),
            stream_size,
            egui,
            egui_state,
            egui_renderer,
            overlay: Overlay { button_visible: true, ..Default::default() },
            button_hover: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn stream_size(&self) -> (u32, u32) {
        self.stream_size
    }

    /// New planes for a picture of `size` and `format` (and a bind group).
    fn planes(&self, size: (u32, u32), format: PixelFormat) -> Planes {
        let (w, h) = size;
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let texture = |width, height, format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("plane"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        use wgpu::TextureFormat::{Bgra8Unorm, R8Unorm, Rg8Unorm};
        let textures = match format {
            PixelFormat::Nv12 => vec![texture(w, h, R8Unorm), texture(cw, ch, Rg8Unorm)],
            PixelFormat::I420 => vec![texture(w, h, R8Unorm), texture(cw, ch, R8Unorm), texture(cw, ch, R8Unorm)],
            _ => vec![texture(w, h, Bgra8Unorm)],
        };
        let views: Vec<_> = textures.iter().map(|t| t.create_view(&Default::default())).collect();
        let params = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = |i: usize| &views[i.min(views.len() - 1)];
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("planes"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view(0)) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view(1)) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(view(2)) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.smooth) },
            ],
        });
        Planes { size, format, textures, bind, params }
    }

    fn upload(&self, planes: &Planes, bytes: &[u8]) {
        let (w, h) = planes.size;
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let write = |texture: &wgpu::Texture, data: &[u8], row: u32, width: u32, height: u32| {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        };
        let (luma, chroma) = ((w * h) as usize, (cw * ch) as usize);
        match planes.format {
            PixelFormat::Nv12 if bytes.len() >= luma + chroma * 2 => {
                write(&planes.textures[0], &bytes[..luma], w, w, h);
                write(&planes.textures[1], &bytes[luma..luma + chroma * 2], cw * 2, cw, ch);
            }
            PixelFormat::I420 if bytes.len() >= luma + chroma * 2 => {
                write(&planes.textures[0], &bytes[..luma], w, w, h);
                write(&planes.textures[1], &bytes[luma..luma + chroma], cw, cw, ch);
                write(&planes.textures[2], &bytes[luma + chroma..luma + chroma * 2], cw, cw, ch);
            }
            PixelFormat::Bgra8 if bytes.len() >= luma * 4 => write(&planes.textures[0], &bytes[..luma * 4], w * 4, w, h),
            _ => tracing::warn!(format = ?planes.format, len = bytes.len(), "picture of the wrong size; skipped"),
        }
    }

    /// Put `frame` up (retiring tiles it already contains). Returns whether
    /// the stream changed size.
    pub fn show(&mut self, frame: &DecodedFrame) -> bool {
        let Ok(bytes) = frame.data.to_cpu() else { return false };
        let size = (frame.width.max(1), frame.height.max(1));
        let resized = size != self.stream_size;
        if resized {
            self.stream_size = size;
            self.tiles.clear();
        }
        if self.video.as_ref().is_none_or(|v| v.size != size || v.format != frame.format) {
            self.video = Some(self.planes(size, frame.format));
        }
        if let Some(video) = &self.video {
            self.upload(video, &bytes);
        }
        self.color = frame.color;
        self.video_ts_us = Some(frame.capture_ts_us);
        while self.tiles.front().is_some_and(|tile| tile.capture_ts_us <= frame.capture_ts_us) {
            self.tiles.pop_front();
        }
        resized
    }

    /// Draw a fast-lane batch over the video, unless the video on screen is
    /// already as new.
    pub fn add_tiles(&mut self, batch: TileBatch) {
        let Some(video_ts) = self.video_ts_us else { return };
        if batch.capture_ts_us <= video_ts || (batch.stream_width, batch.stream_height) != self.stream_size {
            return;
        }
        for tile in &batch.tiles {
            let Ok((header, pixels)) = qoi::decode_to_vec(&tile.qoi) else { continue };
            if (header.width, header.height) != (tile.width, tile.height) || header.channels.as_u8() != 4 {
                continue;
            }
            let planes = self.planes((tile.width, tile.height), PixelFormat::Bgra8);
            self.upload(&planes, &pixels);
            self.tiles.push_back(TileQuad { capture_ts_us: batch.capture_ts_us, rect: (tile.x, tile.y, tile.width, tile.height), planes });
        }
        while self.tiles.len() > MAX_TILES {
            self.tiles.pop_front();
        }
    }

    /// Where the stream sits in the window, in physical pixels: x, y, w, h.
    /// 1:1 when it was sized for this screen (pixel-exact text); else fitted.
    pub fn content_rect(&self) -> (f64, f64, f64, f64) {
        let (win_w, win_h) = (self.config.width as f64, self.config.height as f64);
        let (stream_w, stream_h) = (self.stream_size.0 as f64, self.stream_size.1 as f64);
        let scale = stream_scale((stream_w, stream_h), (win_w, win_h));
        let (w, h) = (stream_w * scale, stream_h * scale);
        (((win_w - w) / 2.0).floor(), ((win_h - h) / 2.0).floor(), w, h)
    }

    /// A rectangle in physical pixels, in clip space.
    fn clip(&self, x: f64, y: f64, w: f64, h: f64) -> [f32; 4] {
        let (win_w, win_h) = (self.config.width as f64, self.config.height as f64);
        [
            (x / win_w * 2.0 - 1.0) as f32,
            (1.0 - y / win_h * 2.0) as f32,
            ((x + w) / win_w * 2.0 - 1.0) as f32,
            (1.0 - (y + h) / win_h * 2.0) as f32,
        ]
    }

    fn params(&self, planes: &Planes, rect: [f32; 4]) {
        let params = Params {
            rect,
            layout: match planes.format {
                PixelFormat::Nv12 => 0,
                PixelFormat::I420 => 1,
                _ => 2,
            },
            full_range: self.color.full_range as u32,
            matrix: match self.color.matrix {
                Matrix::Bt601 => 0,
                Matrix::Bt709 => 1,
                Matrix::Bt2020 => 2,
            },
            linear_out: self.linear_out as u32,
        };
        self.queue.write_buffer(&planes.params, 0, bytemuck::bytes_of(&params));
    }

    // ---- the overlay ----------------------------------------------------------

    pub fn show_stats(&mut self, text: Option<&str>) {
        self.overlay.stats = text.map(str::to_string);
    }

    pub fn show_notice(&mut self, text: Option<&str>) {
        self.overlay.notice = text.map(str::to_string);
    }

    pub fn set_button_visible(&mut self, visible: bool) {
        self.overlay.button_visible = visible;
    }

    pub fn open_menu(&mut self, state: MenuState) {
        self.overlay.menu = Some(state);
        self.overlay.fresh = true;
        self.window.request_redraw();
    }

    pub fn menu_open(&self) -> bool {
        self.overlay.menu.is_some()
    }

    pub fn take_events(&mut self) -> Vec<MenuEvent> {
        std::mem::take(&mut self.overlay.events)
    }

    /// `point` (window points) is on the menu button.
    pub fn button_contains(&self, point: (f64, f64)) -> bool {
        let (x, y) = (point.0 as f32, point.1 as f32);
        self.overlay.button_visible
            && x >= BUTTON_ORIGIN.0
            && x <= BUTTON_ORIGIN.0 + BUTTON_SIZE.0
            && y >= BUTTON_ORIGIN.1
            && y <= BUTTON_ORIGIN.1 + BUTTON_SIZE.1
    }

    pub fn set_button_hover(&mut self, hover: bool) {
        if hover != self.button_hover {
            self.button_hover = hover;
            self.window.request_redraw();
        }
    }

    /// The overlay sees window events first; true if it took this one (a
    /// click or key meant for the open menu).
    pub fn on_window_event(&mut self, event: &WindowEvent) -> bool {
        let response = self.egui_state.on_window_event(&self.window, event);
        if response.repaint {
            self.window.request_redraw();
        }
        self.menu_open() && response.consumed
    }

    /// Draw everything now.
    pub fn render(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let (x, y, w, h) = self.content_rect();
        if let Some(video) = &self.video {
            self.params(video, self.clip(x, y, w, h));
        }
        let scale = w / self.stream_size.0.max(1) as f64;
        for tile in &self.tiles {
            let (tx, ty, tw, th) = tile.rect;
            let rect = self.clip(x + tx as f64 * scale, y + ty as f64 * scale, tw as f64 * scale, th as f64 * scale);
            self.params(&tile.planes, rect);
        }

        // The overlay.
        let raw = self.egui_state.take_egui_input(&self.window);
        let mut overlay = std::mem::take(&mut self.overlay);
        let button_hover = self.button_hover;
        let mut output = self.egui.run_ui(raw, |ui| draw_overlay(ui, &mut overlay, button_hover));
        let mut textures = std::mem::take(&mut output.textures_delta);
        self.overlay = overlay;
        self.egui_state.handle_platform_output(&self.window, output.platform_output);
        let pixels_per_point = output.pixels_per_point;
        let jobs = self.egui.tessellate(output.shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point };
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let extra = self.egui_renderer.update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            pass.set_pipeline(&self.pipeline);
            if let Some(video) = &self.video {
                pass.set_bind_group(0, &video.bind, &[]);
                pass.draw(0..4, 0..1);
            }
            for tile in &self.tiles {
                pass.set_bind_group(0, &tile.planes.bind, &[]);
                pass.draw(0..4, 0..1);
            }
            self.egui_renderer.render(&mut pass, &jobs, &screen);
        }
        for id in &textures.free {
            self.egui_renderer.free_texture(id);
        }
        textures.clear();
        self.queue.submit(extra.into_iter().chain([encoder.finish()]));
        self.window.pre_present_notify();
        self.queue.present(frame);
        if self.overlay.menu.is_some() || output.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.repaint_delay.is_zero()) {
            self.window.request_redraw();
        }
    }
}

// ---- the overlay's look -----------------------------------------------------------

/// A font file fontconfig picks for `pattern`, and its index in the file.
fn system_font(pattern: &str) -> Option<egui::FontData> {
    let output = std::process::Command::new("fc-match").args(["-f", "%{file}\n%{index}", pattern]).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let mut lines = text.lines();
    let path = lines.next()?.trim();
    let index = lines.next().and_then(|index| index.trim().parse().ok()).unwrap_or(0);
    let bytes = std::fs::read(path).ok()?;
    let mut font = egui::FontData::from_owned(bytes);
    font.index = index;
    Some(font)
}

/// The desktop's own fonts (so the overlay looks like it belongs), with a
/// fallback that has ✓ ⌘ ⌥ ⌃, and egui's built-in fonts after those.
fn fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, pattern: &str, family: egui::FontFamily, at: usize| {
        if let Some(font) = system_font(pattern) {
            fonts.font_data.insert(name.into(), Arc::new(font));
            let list = fonts.families.entry(family).or_default();
            list.insert(at.min(list.len()), name.into());
        }
    };
    add("system-sans", "sans-serif", egui::FontFamily::Proportional, 0);
    add("system-mono", "monospace", egui::FontFamily::Monospace, 0);
    add("system-symbols", "sans-serif:charset=2318 2325 2303 2713", egui::FontFamily::Proportional, 1);
    add("system-symbols-mono", "sans-serif:charset=2318 2325 2303 2713", egui::FontFamily::Monospace, 1);
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    fonts(ctx);
    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 2.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.menu_margin = egui::Margin::same(6);
        let visuals = &mut style.visuals;
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.menu_corner_radius = egui::CornerRadius::same(10);
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(28, 28, 32, 245);
        visuals.panel_fill = visuals.window_fill;
        visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(28));
        visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(10, 132, 255);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(10, 110, 220);
        visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
        visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
        visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);
    });
}

/// A rounded translucent bar with one line of text, centred at the top or
/// bottom (the Mac's stats bar and notices).
fn bar(ui: &egui::Ui, id: &str, text: &str, top: bool) {
    let anchor = if top { egui::Align2::CENTER_TOP } else { egui::Align2::CENTER_BOTTOM };
    let offset = if top { egui::vec2(0.0, 12.0) } else { egui::vec2(0.0, -24.0) };
    egui::Area::new(egui::Id::new(id))
        .anchor(anchor, offset)
        .interactable(false)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(166))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(12.0).color(egui::Color32::from_white_alpha(242))).wrap_mode(egui::TextWrapMode::Extend));
                });
        });
}

fn draw_overlay(ui: &mut egui::Ui, overlay: &mut Overlay, button_hover: bool) {
    if let Some(text) = overlay.stats.clone() {
        bar(ui, "stats", &text, true);
    }
    if let Some(text) = overlay.notice.clone() {
        bar(ui, "notice", &text, false);
    }
    let ctx = ui.ctx().clone();
    if overlay.button_visible {
        let alpha = if button_hover || overlay.menu.is_some() { 0.95 } else { 0.35 };
        egui::Area::new(egui::Id::new("menu button"))
            .fixed_pos(egui::pos2(BUTTON_ORIGIN.0, BUTTON_ORIGIN.1))
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(BUTTON_SIZE.0, BUTTON_SIZE.1), egui::Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(rect, 7.0, egui::Color32::from_black_alpha((153.0 * alpha) as u8 + 10));
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, "•••", egui::FontId::proportional(13.0), egui::Color32::from_white_alpha((255.0 * alpha) as u8));
            });
    }
    let Some(state) = overlay.menu.clone() else { return };
    let mut chose: Option<MenuAction> = None;
    let mut sub_hover: Option<Option<(Submenu, f32)>> = None;
    let root = egui::Area::new(egui::Id::new("session menu"))
        .fixed_pos(egui::pos2(BUTTON_ORIGIN.0, BUTTON_ORIGIN.1 + BUTTON_SIZE.1 + 6.0))
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                egui::containers::menu::menu_style(ui.style_mut());
                ui.set_width(MENU_WIDTH);
                ui.label(egui::RichText::new(&state.host).strong().size(13.5));
                if !state.detail.is_empty() {
                    ui.label(egui::RichText::new(&state.detail).size(11.5).weak());
                }
                ui.separator();
                for sub in [Submenu::Keyboard, Submenu::Clipboard, Submenu::Video] {
                    let open = overlay.sub == Some(sub);
                    let row = ui.add(
                        egui::Button::new(sub.title())
                            .right_text("⏵")
                            .selected(open)
                            .frame_when_inactive(false)
                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                    );
                    if row.hovered() || row.clicked() {
                        sub_hover = Some(Some((sub, row.rect.top())));
                    }
                }
                ui.separator();
                let mut plain = |ui: &mut egui::Ui, title: &str, checked: Option<bool>, action: MenuAction| {
                    let row = item(ui, title, checked, true);
                    if row.hovered() {
                        sub_hover = Some(None);
                    }
                    if row.clicked() {
                        chose = Some(action);
                    }
                };
                plain(ui, if state.fullscreen { "Windowed" } else { "Full Screen" }, None, MenuAction::ToggleFullscreen);
                plain(ui, "Stats Bar", Some(state.stats), MenuAction::ToggleStats);
                plain(ui, "Play Sound", Some(state.audio), MenuAction::ToggleAudio);
                plain(ui, if state.button_visible { "Hide Menu Button" } else { "Show Menu Button" }, None, MenuAction::HideButton);
                ui.separator();
                plain(ui, "Disconnect", None, MenuAction::Disconnect);
            });
        });
    if let Some(hover) = sub_hover {
        match hover {
            Some((sub, top)) => {
                overlay.sub = Some(sub);
                overlay.sub_top = top;
            }
            None => overlay.sub = None,
        }
    }
    let mut sub_rect = None;
    if let Some(sub) = overlay.sub {
        let at = egui::pos2(root.response.rect.right() + 4.0, overlay.sub_top - 6.0);
        let shown = egui::Area::new(egui::Id::new("session submenu"))
            .fixed_pos(at)
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    ui.set_width(SUBMENU_WIDTH);
                    let mut pick = |ui: &mut egui::Ui, title: &str, checked: Option<bool>, enabled: bool, action: MenuAction| {
                        if item(ui, title, checked, enabled).clicked() {
                            chose = Some(action);
                        }
                    };
                    match sub {
                        Submenu::Keyboard => {
                            pick(ui, "Send System Shortcuts to Remote", Some(state.capture), state.capture_available, MenuAction::ToggleCapture);
                            ui.separator();
                            heading(ui, "Send keys");
                            for (index, shortcut) in state.shortcuts.iter().enumerate() {
                                pick(ui, shortcut.title, None, true, MenuAction::SendShortcut(index as u8));
                            }
                        }
                        Submenu::Clipboard => {
                            pick(ui, "Share Clipboard", Some(state.clipboard), true, MenuAction::ToggleClipboard);
                            pick(ui, "Type Clipboard Text", None, true, MenuAction::TypeClipboard);
                        }
                        Submenu::Video => {
                            let on = state.video_available;
                            heading(ui, "Codec");
                            for (codec, title) in [(Codec::Hevc, "HEVC (sharper per bit)"), (Codec::H264, "H.264 (most compatible)")] {
                                pick(ui, title, Some(state.codec == codec.name()), on, MenuAction::Codec(codec));
                            }
                            ui.separator();
                            heading(ui, "Resolution");
                            for percent in SCALES {
                                let title = if percent == 100 && state.scale == 100 {
                                    format!("Native ({}×{})", state.stream_size.0, state.stream_size.1)
                                } else if percent == 100 {
                                    "Native".into()
                                } else {
                                    format!("{percent}% (less to send, softer)")
                                };
                                pick(ui, &title, Some(state.scale == percent), on, MenuAction::Scale(percent));
                            }
                            ui.separator();
                            heading(ui, "Frame rate");
                            for fps in FRAME_RATES {
                                let title = if fps == 30 { "30 fps (half the work for slow computers)".to_string() } else { format!("{fps} fps") };
                                pick(ui, &title, Some(state.fps == fps), on, MenuAction::FrameRate(fps));
                            }
                            ui.separator();
                            heading(ui, "Bitrate limit");
                            for mbps in BITRATES_MBPS {
                                pick(ui, &format!("{mbps} Mbps"), Some(state.bitrate_mbps == Some(mbps)), on, MenuAction::BitrateMbps(mbps));
                            }
                            ui.separator();
                            pick(ui, "Fast Lane (lossless text while typing)", Some(state.fast_lane), on, MenuAction::ToggleFastLane);
                        }
                    }
                });
            });
        sub_rect = Some(shown.response.rect);
    }
    let pointer = ctx.input(|i| i.pointer.interact_pos());
    let inside = pointer.is_some_and(|p| root.response.rect.contains(p) || sub_rect.is_some_and(|r| r.contains(p)));
    let clicked_away = !std::mem::take(&mut overlay.fresh) && ctx.input(|i| i.pointer.any_pressed()) && !inside;
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(action) = chose {
        overlay.events.push(MenuEvent::Chose(action));
        overlay.events.push(MenuEvent::Closed);
        overlay.menu = None;
        overlay.sub = None;
    } else if clicked_away || escape {
        overlay.events.push(MenuEvent::Closed);
        overlay.menu = None;
        overlay.sub = None;
    }
}

/// The menu's submenus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Submenu {
    Keyboard,
    Clipboard,
    Video,
}

impl Submenu {
    fn title(self) -> &'static str {
        match self {
            Submenu::Keyboard => "Keyboard",
            Submenu::Clipboard => "Clipboard",
            Submenu::Video => "Video",
        }
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.5).weak());
}

const MENU_WIDTH: f32 = 230.0;
const SUBMENU_WIDTH: f32 = 300.0;

/// One menu line: the title, and a check mark on the right when on.
fn item(ui: &mut egui::Ui, title: &str, checked: Option<bool>, enabled: bool) -> egui::Response {
    let mark = if checked == Some(true) { "✓" } else { "" };
    let button = egui::Button::new(title)
        .frame_when_inactive(false)
        .min_size(egui::vec2(ui.available_width(), 0.0));
    let button = if mark.is_empty() { button } else { button.right_text(mark) };
    ui.add_enabled(enabled, button)
}
