use std::time::Instant;
use glam::*;
// use shipyard::*;
use eks::prelude::*;



#[derive(ResourceIdent, Debug)]
pub struct TimeResource {
	pub this_tick_start: Instant,
	pub last_tick_start: Instant,
}
impl TimeResource {
	pub fn new() -> Self {
		Self {
			this_tick_start: Instant::now(),
			last_tick_start: Instant::now(),
		}
	}

	pub fn next_tick(&mut self) {
		self.last_tick_start = self.this_tick_start;
		self.this_tick_start = Instant::now();
	}
}


// Todo: Rename to WorldTransform
#[repr(C)]
#[derive(ComponentIdent, Debug, Clone, Copy)]
pub struct TransformComponent {
	pub translation: Vec3,
	pub rotation: Quat,
	pub scale: Vec3,
}
impl TransformComponent {
	pub fn new() -> Self {
		Self {
			translation: Vec3::ZERO,
			rotation: Quat::IDENTITY,
			scale: Vec3::ONE,
		}
	}
	pub fn with_position(self, position: Vec3) -> Self {
		Self {
			translation: position,
			rotation: self.rotation,
			scale: self.scale,
		}
	}
	pub fn with_rotation(self, rotation: Quat) -> Self {
		Self {
			translation: self.translation,
			rotation,
			scale: self.scale,
		}
	}
	pub fn with_scale(self, scale: Vec3) -> Self {
		Self {
			translation: self.translation,
			rotation: self.rotation,
			scale,
		}
	}
	pub fn matrix(&self) -> Mat4 {
		Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
	}
}
impl Default for TransformComponent {
	fn default() -> Self {
		Self {
			translation: Vec3::ZERO,
			rotation: Quat::IDENTITY,
			scale: Vec3::ONE,
		}
	}
}
