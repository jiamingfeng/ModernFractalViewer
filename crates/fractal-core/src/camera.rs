//! Camera module for 3D navigation
//!
//! Provides an orbital camera that can be controlled via mouse/touch input.

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

const DEFAULT_DISTANCE: f32 = 3.0;

/// Orbital camera for 3D fractal exploration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    /// Camera position in world space
    pub position: Vec3,
    /// Point the camera is looking at
    pub target: Vec3,
    /// Up vector
    pub up: Vec3,
    /// Field of view in radians
    pub fov: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
    /// Distance from target (orbital radius). Lower = closer/more zoomed in.
    #[serde(alias = "zoom")]
    pub distance: f32,
    /// Rotation around Y axis (azimuth) in radians
    pub azimuth: f32,
    /// Rotation from XZ plane (elevation) in radians
    pub elevation: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov: 60.0_f32.to_radians(),
            near: 0.001,
            far: 100.0,
            distance: DEFAULT_DISTANCE,
            azimuth: 0.0,
            elevation: 0.0,
        }
    }
}

impl Camera {
    /// Create a new camera with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Update camera position based on orbital parameters
    pub fn update_position(&mut self) {
        let x = self.distance * self.elevation.cos() * self.azimuth.sin();
        let y = self.distance * self.elevation.sin();
        let z = self.distance * self.elevation.cos() * self.azimuth.cos();
        self.position = self.target + Vec3::new(x, y, z);
    }

    /// Rotate the camera by delta angles
    pub fn orbit(&mut self, delta_azimuth: f32, delta_elevation: f32) {
        self.azimuth += delta_azimuth;
        self.elevation = (self.elevation + delta_elevation).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
        self.update_position();
    }

    /// Zoom in/out by a factor (operates on orbital distance)
    pub fn zoom_by(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(0.001, 100.0);
        self.update_position();
    }

    /// Compute an adaptive near clip distance that scales with camera distance.
    /// Prevents the ray origin from starting inside the SDF surface when zoomed in.
    pub fn adaptive_near_clip(&self) -> f32 {
        let d = self.distance.min(DEFAULT_DISTANCE);
        (d * 0.001).max(0.00001)
    }

    /// Pan the camera (move target)
    pub fn pan(&mut self, delta: Vec3) {
        let right = self.right();
        let up = self.up();
        self.target += right * delta.x + up * delta.y;
        self.update_position();
    }

    /// Get the view matrix
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Get the projection matrix for a given aspect ratio
    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect_ratio, self.near, self.far)
    }

    /// Get the right vector
    pub fn right(&self) -> Vec3 {
        let forward = (self.target - self.position).normalize();
        forward.cross(self.up).normalize()
    }

    /// Get the actual up vector (perpendicular to view direction)
    pub fn up(&self) -> Vec3 {
        let forward = (self.target - self.position).normalize();
        let right = forward.cross(self.up).normalize();
        right.cross(forward).normalize()
    }

    /// Get the forward direction
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position).normalize()
    }

    /// Reset camera to default position
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn test_orbit_updates_position() {
        let mut cam = Camera::default();
        let orig_distance = cam.distance;
        cam.orbit(0.5, 0.3);
        // Distance from target should be preserved
        let actual_dist = (cam.position - cam.target).length();
        assert!(approx_eq(actual_dist, orig_distance));
        // Position should have changed
        assert!(!approx_eq(cam.position.z, 3.0) || !approx_eq(cam.position.x, 0.0));
    }

    #[test]
    fn test_orbit_elevation_clamp() {
        let mut cam = Camera::default();
        // Try to orbit far past the pole
        cam.orbit(0.0, 100.0);
        let max_elev = std::f32::consts::FRAC_PI_2 - 0.01;
        assert!(cam.elevation <= max_elev + EPS);

        // Try the other direction
        cam.orbit(0.0, -200.0);
        let min_elev = -std::f32::consts::FRAC_PI_2 + 0.01;
        assert!(cam.elevation >= min_elev - EPS);
    }

    #[test]
    fn test_zoom_by_in() {
        let mut cam = Camera::default();
        let orig = cam.distance;
        cam.zoom_by(0.5);
        assert!(approx_eq(cam.distance, orig * 0.5));
    }

    #[test]
    fn test_zoom_by_clamp_min() {
        let mut cam = Camera::default();
        cam.zoom_by(0.0001);
        assert!(cam.distance >= 0.001);
    }

    #[test]
    fn test_zoom_by_clamp_max() {
        let mut cam = Camera::default();
        cam.zoom_by(1000.0);
        assert!(cam.distance <= 100.0);
    }

    #[test]
    fn test_adaptive_near_clip_at_default() {
        let cam = Camera::default();
        let clip = cam.adaptive_near_clip();
        assert!(approx_eq(clip, 3.0 * 0.001));
    }

    #[test]
    fn test_adaptive_near_clip_zoomed_in() {
        let mut cam = Camera::default();
        cam.distance = 0.01;
        let clip = cam.adaptive_near_clip();
        assert!(approx_eq(clip, 0.01 * 0.001));
    }

    #[test]
    fn test_adaptive_near_clip_zoomed_out() {
        let mut cam = Camera::default();
        cam.distance = 50.0;
        let clip = cam.adaptive_near_clip();
        // Should cap at DEFAULT_DISTANCE * 0.001
        assert!(approx_eq(clip, DEFAULT_DISTANCE * 0.001));
    }

    #[test]
    fn test_pan_moves_target() {
        let mut cam = Camera::default();
        let orig_target = cam.target;
        cam.pan(Vec3::new(1.0, 0.0, 0.0));
        assert_ne!(cam.target, orig_target);
    }

    #[test]
    fn test_reset_restores_defaults() {
        let mut cam = Camera::default();
        cam.orbit(1.0, 0.5);
        cam.zoom_by(0.1);
        cam.pan(Vec3::new(5.0, 5.0, 5.0));
        cam.reset();
        let def = Camera::default();
        assert!(approx_eq(cam.distance, def.distance));
        assert!(approx_eq(cam.azimuth, def.azimuth));
        assert!(approx_eq(cam.elevation, def.elevation));
        assert_eq!(cam.target, def.target);
    }

    #[test]
    fn test_view_matrix_not_degenerate() {
        let cam = Camera::default();
        let mat = cam.view_matrix();
        assert!(mat.determinant().abs() > EPS);
    }

    #[test]
    fn test_right_forward_up_orthogonal() {
        let mut cam = Camera::default();
        cam.orbit(0.3, 0.2);
        let r = cam.right();
        let f = cam.forward();
        let u = cam.up();
        assert!(r.dot(f).abs() < EPS);
        assert!(r.dot(u).abs() < EPS);
        assert!(f.dot(u).abs() < EPS);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut cam = Camera::default();
        cam.orbit(0.5, 0.3);
        cam.zoom_by(2.0);
        let json = serde_json::to_string(&cam).unwrap();
        let cam2: Camera = serde_json::from_str(&json).unwrap();
        assert!(approx_eq(cam.distance, cam2.distance));
        assert!(approx_eq(cam.azimuth, cam2.azimuth));
        assert!(approx_eq(cam.elevation, cam2.elevation));
    }
}
