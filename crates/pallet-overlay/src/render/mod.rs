//! The GPU loupe renderer.
//!
//! Separate from any window system: [`Renderer`] draws into whatever target it
//! is handed, which may be a compositor surface or an offscreen texture. That
//! makes the shader — where every fidelity decision lives — testable without a
//! display server.

use pallet_capture::Frame;
use pallet_color::Color;
use wgpu::util::DeviceExt;

use crate::error::{Error, Result};

/// Everything the shader needs for one frame.
///
/// `repr(C)` with explicit padding because this is copied straight into a
/// uniform buffer; WGSL aligns `vec4` to 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    cursor: [f32; 2],
    zoom: f32,
    radius: f32,
    sample: f32,
    grid: f32,
    _pad: [f32; 2],
    picked: [f32; 4],
}

/// What to draw this frame.
#[derive(Debug, Clone, Copy)]
pub struct LoupeView {
    /// Cursor position in the frame's physical pixels.
    pub cursor: (u32, u32),
    /// Magnification.
    pub zoom: u32,
    /// Loupe radius in physical pixels.
    pub radius: f32,
    /// Width of the averaged sample square in source pixels; 1 for none.
    pub sample: u32,
    /// Whether to draw the pixel grid.
    pub grid: bool,
    /// The colour under the cursor, drawn on the rim.
    pub picked: Color,
}

impl Default for LoupeView {
    fn default() -> Self {
        Self {
            cursor: (0, 0),
            zoom: 16,
            radius: 140.0,
            sample: 1,
            grid: true,
            picked: Color::new(0, 0, 0),
        }
    }
}

/// The pixel format used throughout.
///
/// Deliberately **not** an `Srgb` format. Captured bytes are already
/// sRGB-encoded and the display expects sRGB-encoded bytes, so treating them as
/// linear and letting the GPU convert would shift every colour. Passing them
/// through untouched is what makes a frozen screen pixel-identical to the live
/// one.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One monitor's frozen pixels, uploaded and ready to draw.
///
/// Separate from [`Renderer`] because a desktop has several monitors but only
/// one GPU context: the device, pipeline and shader are shared, while the
/// texture and uniforms are per display.
#[derive(Debug)]
pub struct Screen {
    #[expect(dead_code, reason = "kept alive for the bind group's texture view")]
    texture: wgpu::Texture,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl Screen {
    /// The frozen frame's size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// The shared GPU context that draws frozen frames and the loupe.
#[derive(Debug)]
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl Renderer {
    /// Build a renderer on a headless adapter.
    ///
    /// Used by tests and as the base for a surface-backed renderer.
    pub fn new_headless() -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|e| Error::NoGpu(e.to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pallet-overlay"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| Error::NoGpu(e.to_string()))?;

        Ok(Self::from_device(device, queue))
    }

    /// Build from an existing device and queue.
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("loupe"),
            source: wgpu::ShaderSource::Wgsl(include_str!("loupe.wgsl").into()),
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loupe-bind-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loupe-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loupe-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(FORMAT.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        Self {
            device,
            queue,
            pipeline,
            layout,
        }
    }

    /// The device, for callers that need to build a surface against it.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Upload a captured frame as a frozen backdrop for one monitor.
    ///
    /// Converts to RGBA once, on upload, rather than in the shader: the frame
    /// changes only when the picker opens, whereas the shader runs for every
    /// pixel of every redraw.
    pub fn create_screen(&self, frame: &Frame) -> Result<Screen> {
        let (w, h) = (frame.monitor.pixel_width, frame.monitor.pixel_height);
        if w == 0 || h == 0 {
            return Err(Error::EmptyFrame);
        }

        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        for y in 0..h {
            for x in 0..w {
                let Some(c) = frame.pixel(x, y) else { continue };
                let i = (y as usize * w as usize + x as usize) * 4;
                rgba[i] = c.r;
                rgba[i + 1] = c.g;
                rgba[i + 2] = c.b;
                rgba[i + 3] = 0xFF;
            }
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frozen-frame"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        let uniforms = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("loupe-uniforms"),
                contents: bytemuck::bytes_of(&Uniforms {
                    cursor: [0.0, 0.0],
                    zoom: 16.0,
                    radius: 0.0,
                    sample: 1.0,
                    grid: 1.0,
                    _pad: [0.0, 0.0],
                    picked: [0.0, 0.0, 0.0, 1.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loupe-bind"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniforms.as_entire_binding(),
                },
            ],
        });

        Ok(Screen {
            texture,
            uniforms,
            bind_group,
            width: w,
            height: h,
        })
    }

    /// Draw into an offscreen texture and read the pixels back.
    ///
    /// The loupe's correctness is entirely a property of the shader, so this
    /// makes it testable without a compositor: render, read back, assert on
    /// exact pixels.
    pub fn render_to_pixels(
        &self,
        screen: &Screen,
        width: u32,
        height: u32,
        view: LoupeView,
    ) -> Result<Vec<u8>> {
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(screen, &target_view, view);

        // Readback rows must be aligned, so the buffer is usually wider than
        // the image and is trimmed after mapping.
        let unpadded = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback-encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|e| Error::NoGpu(e.to_string()))?;

        let mapped = slice
            .get_mapped_range()
            .map_err(|e| Error::NoGpu(e.to_string()))?;
        let mut out = Vec::with_capacity(unpadded as usize * height as usize);
        for row in 0..height as usize {
            let start = row * padded as usize;
            out.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }

    /// Draw one screen's frozen frame, and the loupe if it is on this screen.
    pub fn draw(&self, screen: &Screen, target: &wgpu::TextureView, view: LoupeView) {
        self.queue.write_buffer(
            &screen.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                // Aim at the centre of the pixel, so the magnified texel under
                // the crosshair is the one that will be picked.
                cursor: [view.cursor.0 as f32 + 0.5, view.cursor.1 as f32 + 0.5],
                zoom: view.zoom as f32,
                radius: view.radius,
                sample: view.sample as f32,
                grid: if view.grid { 1.0 } else { 0.0 },
                _pad: [0.0, 0.0],
                picked: [
                    f32::from(view.picked.r) / 255.0,
                    f32::from(view.picked.g) / 255.0,
                    f32::from(view.picked.b) / 255.0,
                    1.0,
                ],
            }),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("loupe-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loupe-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &screen.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}
