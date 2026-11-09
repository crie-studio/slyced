//! wgpu volume ray caster integrated into egui through paint callbacks.
//!
//! Each 3D viewport renders into its own offscreen texture (at reduced
//! resolution while the user interacts) which is then blitted into egui's
//! render pass. Nothing is re-rendered when the parameters do not change,
//! except with the cinematic light, which keeps adding samples to a running
//! average until [`ACCUM_FRAMES`] are reached.

use crate::config::Lighting;
use crate::imaging::volume::Volume;
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use glam::{DVec3, Mat3, Mat4, Vec3};
use half::f16;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::tf::TF_SIZE;

/// Set when the GPU reports an error at runtime; the 3D view then stops rendering.
pub static GPU_FAULT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const VOLUME_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;
// Half floats: the cinematic average of many frames needs more than 8 bits.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Frames averaged by the cinematic light.
pub const ACCUM_FRAMES: u32 = 64;

/// Volume samples converted to normalized half floats, ready for upload.
pub struct GpuVolumeData {
    pub source_id: u64,
    pub dims: [u32; 3],
    pub data: Vec<f16>,
    /// `real = texel * value_scale + value_offset`
    pub value_offset: f32,
    pub value_scale: f32,
    pub spacing: DVec3,
    pub axes: [DVec3; 3],
    pub downsampled: bool,
}

impl GpuVolumeData {
    pub fn prepare(vol: &Volume, max_dim: u32, max_voxels: usize) -> Self {
        let mut factor = [1usize; 3];
        for (f, &d) in factor.iter_mut().zip(&vol.dims) {
            *f = d.div_ceil(max_dim as usize).max(1);
        }
        let count = |f: &[usize; 3]| (0..3).map(|i| vol.dims[i].div_ceil(f[i])).product::<usize>();
        while count(&factor) > max_voxels {
            // Halve the axis with the most samples left.
            let i = (0..3).max_by_key(|&i| vol.dims[i] / factor[i]).unwrap_or(0);
            factor[i] += 1;
        }
        let small;
        let src = if factor != [1, 1, 1] {
            small = vol.downsampled(factor);
            &small
        } else {
            vol
        };
        let (lo, hi) = src.real_range();
        let span = (hi - lo).max(1e-6);
        let q = src.quant;
        let data: Vec<f16> = src.data.par_iter().map(|&s| f16::from_f32((q.real(s) - lo) / span)).collect();
        Self {
            source_id: vol.id,
            dims: src.dims.map(|d| d as u32),
            data,
            value_offset: lo,
            value_scale: span,
            spacing: src.spacing,
            axes: src.axes,
            downsampled: factor != [1, 1, 1],
        }
    }

    pub fn extent_mm(&self) -> DVec3 {
        DVec3::new(
            self.dims[0] as f64 * self.spacing.x,
            self.dims[1] as f64 * self.spacing.y,
            self.dims[2] as f64 * self.spacing.z,
        )
    }

    /// Texture space ([0,1]^3) -> normalized world space (patient orientation,
    /// centered, bounding sphere of radius 1).
    pub fn model_matrix(&self) -> Mat4 {
        let e = self.extent_mm();
        let radius = (e.length() * 0.5).max(1e-3) as f32;
        let a = self.axes.map(|v| Vec3::new(v.x as f32, v.y as f32, v.z as f32));
        let rot = Mat4::from_mat3(Mat3::from_cols(a[0], a[1], a[2]));
        Mat4::from_scale(Vec3::splat(1.0 / radius))
            * rot
            * Mat4::from_scale(Vec3::new(e.x as f32, e.y as f32, e.z as f32))
            * Mat4::from_translation(Vec3::splat(-0.5))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderMode {
    #[default]
    Composite,
    Mip,
    MinIp,
    Iso,
}

impl RenderMode {
    pub const ALL: [RenderMode; 4] = [RenderMode::Composite, RenderMode::Mip, RenderMode::MinIp, RenderMode::Iso];

    pub fn label(self) -> &'static str {
        match self {
            RenderMode::Composite => "VR",
            RenderMode::Mip => "MIP",
            RenderMode::MinIp => "MinIP",
            RenderMode::Iso => "Superfície",
        }
    }

    fn code(self) -> f32 {
        match self {
            RenderMode::Composite => 0.0,
            RenderMode::Mip => 1.0,
            RenderMode::MinIp => 2.0,
            RenderMode::Iso => 3.0,
        }
    }
}

/// Everything that affects the rendered image. Compared frame to frame to
/// decide whether the offscreen image must be refreshed.
#[derive(Clone, PartialEq)]
pub struct RenderParams {
    pub inv_mvp: Mat4,
    pub crop_min: Vec3,
    pub crop_max: Vec3,
    pub mode: RenderMode,
    /// Transfer function range (composite) or display window (MIP/MinIP).
    pub range: (f32, f32),
    pub iso: f32,
    pub opacity: f32,
    pub shading: bool,
    pub light: [f32; 4],
    pub samples_per_voxel: f32,
    pub bg_top: [f32; 3],
    pub bg_bottom: [f32; 3],
    pub iso_color: [f32; 3],
    pub tf_version: u64,
    pub lighting: Lighting,
}

impl RenderParams {
    /// Lighting actually used: MIP/MinIP and unshaded VR keep the plain look.
    pub fn lighting(&self) -> Lighting {
        match self.mode {
            RenderMode::Iso => self.lighting,
            RenderMode::Composite if self.shading => self.lighting,
            _ => Lighting::Simple,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    inv_mvp: [[f32; 4]; 4],
    crop_min: [f32; 4],
    crop_max: [f32; 4],
    extent: [f32; 4],
    dims: [f32; 4],
    value: [f32; 4],
    light: [f32; 4],
    params: [f32; 4],
    bg_top: [f32; 4],
    bg_bottom: [f32; 4],
    iso_color: [f32; 4],
    look: [f32; 4],
}

struct GpuVolume {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    dims: [u32; 3],
    extent: DVec3,
    min_spacing: f32,
    value_offset: f32,
    value_scale: f32,
    last_used: u64,
}

struct Target {
    texture_view: wgpu::TextureView,
    size: [u32; 2],
    uniform: wgpu::Buffer,
    tf_texture: wgpu::Texture,
    tf_view: wgpu::TextureView,
    tf_version: u64,
    volume_bind: Option<(u64, wgpu::BindGroup)>,
    blit_bind: wgpu::BindGroup,
    last: Option<(u64, [u32; 2], RenderParams)>,
    /// Frames already in the image (cinematic average).
    samples: u32,
    last_used: u64,
}

pub struct VolumeRenderer {
    vr_pipeline: wgpu::RenderPipeline,
    /// Same shader, blending each new frame into the running average.
    accum_pipeline: wgpu::RenderPipeline,
    vr_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    volumes: HashMap<u64, GpuVolume>,
    targets: HashMap<u64, Target>,
    frame: u64,
}

impl VolumeRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let vr_shader = device.create_shader_module(wgpu::include_wgsl!("volume.wgsl"));
        let blit_shader = device.create_shader_module(wgpu::include_wgsl!("blit.wgsl"));

        let tex_entry = |binding, dim| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dim,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };

        let vr_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slyced-vr-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1, wgpu::TextureViewDimension::D3),
                tex_entry(2, wgpu::TextureViewDimension::D2),
                sampler_entry(3),
            ],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("slyced-blit-layout"),
            entries: &[tex_entry(0, wgpu::TextureViewDimension::D2), sampler_entry(1)],
        });

        let pipeline = |label, layout: &wgpu::BindGroupLayout, module, fs: &str, format, blend| {
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let vr_pipeline = pipeline("slyced-vr", &vr_layout, &vr_shader, "fs_volume", TARGET_FORMAT, None);
        // new = frame * a + old * (1 - a), with a = 1 / (n + 1) written by the shader.
        let average = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let accum_pipeline = pipeline("slyced-vr-accum", &vr_layout, &vr_shader, "fs_volume", TARGET_FORMAT, Some(average));
        let blit_fs = if target_format.is_srgb() { "fs_blit_srgb" } else { "fs_blit" };
        let blit_pipeline = pipeline("slyced-blit", &blit_layout, &blit_shader, blit_fs, target_format, None);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("slyced-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            vr_pipeline,
            accum_pipeline,
            vr_layout,
            blit_pipeline,
            blit_layout,
            sampler,
            volumes: HashMap::new(),
            targets: HashMap::new(),
            frame: 0,
        }
    }

    fn upload_volume(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &GpuVolumeData) {
        let [w, h, d] = data.dims;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slyced-volume"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: d },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: VOLUME_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Upload in slabs to keep staging allocations small.
        let slice_len = (w * h) as usize;
        let slab = (32 * 1024 * 1024 / (slice_len * 2)).clamp(1, d as usize);
        let bytes: &[u8] = bytemuck::cast_slice(&data.data);
        for z0 in (0..d as usize).step_by(slab) {
            let n = slab.min(d as usize - z0);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: z0 as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes[z0 * slice_len * 2..(z0 + n) * slice_len * 2],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 2), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: n as u32 },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Keep at most two volumes resident.
        while self.volumes.len() >= 2 {
            let Some((&oldest, _)) = self.volumes.iter().min_by_key(|(_, v)| v.last_used) else { break };
            self.volumes.remove(&oldest);
            for t in self.targets.values_mut() {
                if t.volume_bind.as_ref().is_some_and(|(id, _)| *id == oldest) {
                    t.volume_bind = None;
                }
            }
        }
        self.volumes.insert(
            data.source_id,
            GpuVolume {
                _texture: texture,
                view,
                dims: data.dims,
                extent: data.extent_mm(),
                min_spacing: data.spacing.min_element() as f32,
                value_offset: data.value_offset,
                value_scale: data.value_scale,
                last_used: self.frame,
            },
        );
    }

    fn ensure_target(&mut self, device: &wgpu::Device, view_id: u64, size: [u32; 2]) {
        if self.targets.get(&view_id).is_some_and(|t| t.size == size) {
            return;
        }
        let old = self.targets.remove(&view_id);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slyced-vr-target"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("slyced-blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&texture_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let (uniform, tf_texture, tf_view, tf_version) = match old {
            Some(t) => (t.uniform, t.tf_texture, t.tf_view, t.tf_version),
            None => {
                let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("slyced-vr-uniforms"),
                    size: std::mem::size_of::<Uniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let tf_texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("slyced-tf"),
                    size: wgpu::Extent3d { width: TF_SIZE as u32, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let tf_view = tf_texture.create_view(&wgpu::TextureViewDescriptor::default());
                (uniform, tf_texture, tf_view, u64::MAX)
            }
        };
        self.targets.insert(
            view_id,
            Target {
                texture_view,
                size,
                uniform,
                tf_texture,
                tf_view,
                tf_version,
                volume_bind: None,
                blit_bind,
                last: None,
                samples: 0,
                last_used: self.frame,
            },
        );
    }

    pub fn has_volume(&self, id: u64) -> bool {
        self.volumes.contains_key(&id)
    }
}

/// Per-frame paint callback for one 3D viewport.
pub struct VolumeCallback {
    pub view_id: u64,
    pub volume: Arc<GpuVolumeData>,
    pub params: RenderParams,
    pub tf_table: Arc<Vec<f16>>,
    pub size_px: [u32; 2],
    /// Receives how many frames the image has (cinematic average).
    pub progress: Arc<AtomicU32>,
}

impl egui_wgpu::CallbackTrait for VolumeCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<VolumeRenderer>() else { return Vec::new() };
        r.frame += 1;
        let vid = self.volume.source_id;
        if !r.volumes.contains_key(&vid) {
            r.upload_volume(device, queue, &self.volume);
        }
        let size = [self.size_px[0].max(1), self.size_px[1].max(1)];
        r.ensure_target(device, self.view_id, size);

        let frame = r.frame;
        let (vr_layout, sampler) = (&r.vr_layout, &r.sampler);
        let Some(vol) = r.volumes.get_mut(&vid) else { return Vec::new() };
        vol.last_used = frame;
        let Some(t) = r.targets.get_mut(&self.view_id) else { return Vec::new() };
        t.last_used = frame;

        if t.tf_version != self.params.tf_version {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.tf_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&self.tf_table),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(TF_SIZE as u32 * 8), rows_per_image: Some(1) },
                wgpu::Extent3d { width: TF_SIZE as u32, height: 1, depth_or_array_layers: 1 },
            );
            t.tf_version = self.params.tf_version;
            t.last = None;
        }
        if t.volume_bind.as_ref().is_none_or(|(id, _)| *id != vid) {
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("slyced-vr"),
                layout: vr_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: t.uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&vol.view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.tf_view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            });
            t.volume_bind = Some((vid, bind));
            t.last = None;
        }

        let key = (vid, size, self.params.clone());
        let lighting = self.params.lighting();
        let sample = if t.last.as_ref() == Some(&key) {
            if lighting != Lighting::Cinematic || t.samples >= ACCUM_FRAMES {
                self.progress.store(ACCUM_FRAMES, Ordering::Relaxed);
                return Vec::new();
            }
            t.samples
        } else {
            0
        };

        let p = &self.params;
        let (lo, hi) = p.range;
        let ext = vol.extent;
        let u = Uniforms {
            inv_mvp: p.inv_mvp.to_cols_array_2d(),
            crop_min: [p.crop_min.x, p.crop_min.y, p.crop_min.z, 0.0],
            crop_max: [p.crop_max.x, p.crop_max.y, p.crop_max.z, 0.0],
            extent: [ext.x as f32, ext.y as f32, ext.z as f32, vol.min_spacing.max(0.1)],
            dims: [vol.dims[0] as f32, vol.dims[1] as f32, vol.dims[2] as f32, p.samples_per_voxel],
            value: [vol.value_offset, vol.value_scale, lo, 1.0 / (hi - lo).max(1e-3)],
            light: p.light,
            params: [p.mode.code(), p.iso, p.opacity, if p.shading { 1.0 } else { 0.0 }],
            bg_top: [p.bg_top[0], p.bg_top[1], p.bg_top[2], 1.0],
            bg_bottom: [p.bg_bottom[0], p.bg_bottom[1], p.bg_bottom[2], 1.0],
            iso_color: [p.iso_color[0], p.iso_color[1], p.iso_color[2], 1.0],
            look: [lighting.code(), sample as f32, 0.0, 0.0],
        };
        queue.write_buffer(&t.uniform, 0, bytemuck::bytes_of(&u));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("slyced-vr") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("slyced-vr"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if sample > 0 { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if sample > 0 { &r.accum_pipeline } else { &r.vr_pipeline });
            if let Some((_, bind)) = &t.volume_bind {
                pass.set_bind_group(0, bind, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        t.last = Some(key);
        t.samples = sample + 1;
        let done = if lighting == Lighting::Cinematic { t.samples } else { ACCUM_FRAMES };
        self.progress.store(done, Ordering::Relaxed);

        // Forget targets of viewports that disappeared.
        let frame = r.frame;
        r.targets.retain(|_, t| frame.saturating_sub(t.last_used) < 600);
        vec![encoder.finish()]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<VolumeRenderer>() else { return };
        let Some(t) = r.targets.get(&self.view_id) else { return };
        if t.last.is_none() {
            return;
        }
        render_pass.set_pipeline(&r.blit_pipeline);
        render_pass.set_bind_group(0, &t.blit_bind, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
