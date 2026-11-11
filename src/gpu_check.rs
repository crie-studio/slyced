//! Graphics probing done before the window is created, so a missing or broken
//! driver leads to a fallback instead of a silent crash.

use eframe::wgpu;

pub struct AdapterReport {
    pub info: wgpu::AdapterInfo,
    /// `None` when not tested (a better adapter already worked).
    pub device: Option<Result<(), String>>,
}

impl AdapterReport {
    pub fn line(&self) -> String {
        let kind = match self.info.device_type {
            wgpu::DeviceType::DiscreteGpu => tr!("GPU dedicada", "dedicated GPU"),
            wgpu::DeviceType::IntegratedGpu => tr!("GPU integrada", "integrated GPU"),
            wgpu::DeviceType::VirtualGpu => tr!("GPU virtual", "virtual GPU"),
            wgpu::DeviceType::Cpu => "software (CPU)",
            wgpu::DeviceType::Other => tr!("outro", "other"),
        };
        let status = match &self.device {
            None => tr!("não testado", "not tested").to_string(),
            Some(Ok(())) => "OK".to_string(),
            Some(Err(e)) => trf!("FALHOU: {e}", "FAILED: {e}"),
        };
        let driver = [self.info.driver.as_str(), self.info.driver_info.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} · {:?} · {kind} · driver {driver} → {status}", self.info.name, self.info.backend)
    }
}

/// Preference: real GPU over virtual/software; on the same GPU, DirectX 12
/// (most consistent on Windows), then Vulkan, then OpenGL.
pub fn score(info: &wgpu::AdapterInfo) -> i32 {
    let device = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 3,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 1,
        wgpu::DeviceType::Cpu => 0,
    };
    let backend = match info.backend {
        wgpu::Backend::Dx12 => 3,
        wgpu::Backend::Vulkan => 2,
        wgpu::Backend::Gl => 0,
        _ => 1,
    };
    device * 10 + backend
}

/// Limits asked for the device, used both by the probe and by eframe (so a
/// passing test means the window will open too). On OpenGL (Windows 7 and old
/// drivers) the conservative WebGL2 limits are kept, except the 3D texture
/// size: the driver's own, so the 3D view is not reduced to 256³.
pub fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    let base = if adapter.get_info().backend == wgpu::Backend::Gl {
        let webgl2 = wgpu::Limits::downlevel_webgl2_defaults();
        wgpu::Limits {
            max_texture_dimension_3d: adapter.limits().max_texture_dimension_3d.clamp(webgl2.max_texture_dimension_3d, 2048),
            ..webgl2
        }
    } else {
        wgpu::Limits::default()
    };
    wgpu::DeviceDescriptor {
        label: Some("slyced"),
        required_limits: wgpu::Limits { max_texture_dimension_2d: 8192, ..base },
        ..Default::default()
    }
}

/// Lists adapters best-first and creates a device on each until one works
/// (or on every adapter with `test_all`). Runs on its own thread with a
/// timeout, so a driver that crashes or hangs only means "no usable GPU".
pub fn probe(test_all: bool) -> Vec<AdapterReport> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("gpu-probe".into()).spawn(move || {
        let _ = tx.send(probe_now(test_all));
    });
    if spawned.is_err() {
        return Vec::new();
    }
    rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap_or_else(|_| {
        log::error!("a verificação da GPU falhou ou não respondeu (driver com problema)");
        Vec::new()
    })
}

fn probe_now(test_all: bool) -> Vec<AdapterReport> {
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle_from_env()
        });
        let mut adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        adapters.sort_by_key(|a| -score(&a.get_info()));
        let mut found = false;
        let mut out = Vec::new();
        for adapter in adapters {
            let device = if test_all || !found {
                let r = pollster::block_on(adapter.request_device(&device_descriptor(&adapter)))
                    .map(|_| ())
                    .map_err(|e| e.to_string());
                found |= r.is_ok();
                Some(r)
            } else {
                None
            };
            out.push(AdapterReport { info: adapter.get_info(), device });
        }
        out
    }
}

/// Best adapter whose device creation succeeded.
pub fn usable(reports: &[AdapterReport]) -> Option<&AdapterReport> {
    reports.iter().find(|r| matches!(r.device, Some(Ok(()))))
}
