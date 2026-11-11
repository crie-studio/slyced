//! Orbit camera for the 3D view, expressed in patient (LPS) space.

use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    Anterior,
    Posterior,
    Left,
    Right,
    Superior,
    Inferior,
}

impl ViewPreset {
    pub const ALL: [ViewPreset; 6] = [
        ViewPreset::Anterior,
        ViewPreset::Posterior,
        ViewPreset::Left,
        ViewPreset::Right,
        ViewPreset::Superior,
        ViewPreset::Inferior,
    ];

    pub fn short(self) -> &'static str {
        match self {
            ViewPreset::Anterior => "A",
            ViewPreset::Posterior => "P",
            ViewPreset::Left => "L",
            ViewPreset::Right => "R",
            ViewPreset::Superior => "S",
            ViewPreset::Inferior => "I",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewPreset::Anterior => "Anterior",
            ViewPreset::Posterior => "Posterior",
            ViewPreset::Left => tr!("Esquerda", "Left"),
            ViewPreset::Right => tr!("Direita", "Right"),
            ViewPreset::Superior => "Superior",
            ViewPreset::Inferior => "Inferior",
        }
    }

    fn eye_up(self) -> (Vec3, Vec3) {
        match self {
            ViewPreset::Anterior => (-Vec3::Y, Vec3::Z),
            ViewPreset::Posterior => (Vec3::Y, Vec3::Z),
            ViewPreset::Left => (Vec3::X, Vec3::Z),
            ViewPreset::Right => (-Vec3::X, Vec3::Z),
            ViewPreset::Superior => (Vec3::Z, -Vec3::Y),
            ViewPreset::Inferior => (-Vec3::Z, -Vec3::Y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    /// World -> camera rotation.
    pub rotation: Quat,
    /// Distance from the target in units of the volume's bounding radius.
    pub distance: f32,
    /// Screen-space pan in units of the bounding radius.
    pub pan: Vec2,
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let mut c = Self { rotation: Quat::IDENTITY, distance: 3.6, pan: Vec2::ZERO, fov_y: 30f32.to_radians() };
        c.set_preset(ViewPreset::Anterior);
        c
    }
}

impl OrbitCamera {
    pub fn set_preset(&mut self, p: ViewPreset) {
        let (eye, up) = p.eye_up();
        let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, up);
        self.rotation = Quat::from_mat4(&view).normalize();
        self.pan = Vec2::ZERO;
    }

    /// Trackball rotation from a mouse drag (pixels).
    pub fn rotate(&mut self, delta: Vec2) {
        let k = 0.008;
        let q = Quat::from_axis_angle(Vec3::Y, delta.x * k) * Quat::from_axis_angle(Vec3::X, delta.y * k);
        self.rotation = (q * self.rotation).normalize();
    }

    /// Rotation around the viewing axis (roll).
    pub fn roll(&mut self, radians: f32) {
        self.rotation = (Quat::from_axis_angle(Vec3::Z, radians) * self.rotation).normalize();
    }

    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(1.05, 20.0);
    }

    pub fn pan_by(&mut self, delta_px: Vec2, viewport_h: f32) {
        let world_per_px = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_h.max(1.0);
        self.pan += Vec2::new(delta_px.x, -delta_px.y) * world_per_px;
    }

    pub fn view(&self) -> Mat4 {
        Mat4::from_translation(Vec3::new(self.pan.x, self.pan.y, -self.distance)) * Mat4::from_quat(self.rotation)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(self.fov_y, aspect.max(1e-3), (self.distance - 1.5).max(0.05), self.distance + 1.5)
    }
}
