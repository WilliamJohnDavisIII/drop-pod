// SPDX-License-Identifier: GPL-3.0-or-later
// Screens drawn without a window: the same egui and wgpu renderer the window uses, into a texture, saved as PNG.
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

pub struct Headless {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
}

// (as the window: egui blends in gamma space, on a plain, non-sRGB framebuffer)
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Headless {
    pub fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("no graphics adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("no graphics device");
        let renderer = egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        Self { device, queue, renderer }
    }

    /// Runs `frames` frames of `ui` (1/60 s apart, so animations settle) at `size` points and `ppp` pixels per point,
    /// and returns the last one.
    pub fn render(&mut self, ctx: &egui::Context, size: egui::Vec2, ppp: f32, frames: usize,
                  mut ui: impl FnMut(&mut egui::Ui)) -> image::RgbaImage {
        let px = [(size.x * ppp).round() as u32, (size.y * ppp).round() as u32];
        let mut output = None;
        for f in 0..frames {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(f as f64 / 60.0),
                predicted_dt: 1.0 / 60.0,
                ..Default::default()
            };
            input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
            let out = ctx.run_ui(input, |u| ui(u));
            for (id, deltas) in &out.textures_delta.set { for delta in deltas {
                self.renderer.update_texture(&self.device, &self.queue, *id, delta); }
            }
            for id in &out.textures_delta.free {
                self.renderer.free_texture(id);
            }
            output = Some(out);
        }
        let out = output.expect("at least one frame");
        let prims = ctx.tessellate(out.shapes, ppp);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: px, pixels_per_point: ppp };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screen"),
            size: wgpu::Extent3d { width: px[0], height: px[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let extra = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, &prims, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &prims, &screen);
        }
        let row = (px[0] * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * px[1]) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            wgpu::Extent3d { width: px[0], height: px[1], depth_or_array_layers: 1 },
        );
        self.queue.submit(extra.into_iter().chain([encoder.finish()]));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("gpu");
        let data = slice.get_mapped_range().expect("mapped");
        let mut img = image::RgbaImage::new(px[0], px[1]);
        for y in 0..px[1] {
            let start = (y * row) as usize;
            img.as_mut()[(y * px[0] * 4) as usize..((y + 1) * px[0] * 4) as usize]
                .copy_from_slice(&data[start..start + (px[0] * 4) as usize]);
        }
        img
    }
}
