use eeks::prelude::*;
use krender::MeshKey;
use std::{ops::{Deref, DerefMut}, sync::Arc};
use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Mat4, Vec4, Vec2};
pub use krender::{prelude::*, BufferKey, MaterialKey, RenderContextKey, TextureKey};
use rand::Rng;
use crate::transform::*;



#[derive(Debug, Resource)]
pub struct DeviceResource(pub Arc<wgpu::Device>);
impl Deref for DeviceResource {
	type Target = Arc<wgpu::Device>;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}


#[derive(Debug, Resource)]
pub struct QueueResource(pub Arc<wgpu::Queue>);
impl Deref for QueueResource {
	type Target = Arc<wgpu::Queue>;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}


#[derive(Debug, Resource, Default)]
#[sda(lua = true)]
pub struct MaterialResource (pub MaterialManager);
impl Deref for MaterialResource {
	type Target = MaterialManager;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl DerefMut for MaterialResource {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}
impl mlua::UserData for MaterialResource {
	fn add_fields<'lua, F: mlua::UserDataFields<'lua, Self>>(_fields: &mut F) {}
	fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(_methods: &mut M) {}
}


#[derive(Debug, Resource)]
pub struct BufferResource (pub BufferManager);
impl Deref for BufferResource {
	type Target = BufferManager;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl DerefMut for BufferResource {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}


#[derive(Debug, Resource, Default)]
pub struct TextureResource (pub TextureManager);
impl Deref for TextureResource {
	type Target = TextureManager;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl DerefMut for TextureResource {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}


#[derive(Debug, Resource, Default)]
pub struct MeshResource (pub MeshManager);
impl Deref for MeshResource {
	type Target = MeshManager;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl DerefMut for MeshResource {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}


/// Contexts are stored as a resource (viewport->context->entity) rather than a component (viewport->entity->context) becuase we might want to have multiple contexts for one entity (example: rendering to two different resolutions). 
#[derive(Debug, Resource, Default)]
pub struct ContextResource (pub RenderContextManager);
impl Deref for ContextResource {
	type Target = RenderContextManager;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl DerefMut for ContextResource {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}


/// The render work that will be exectuted for a frame. 
/// We cannot pass it to the systems as an argument, so it is a resource. 
/// Also stores the key of the context of this frame. 
#[derive(Debug, Resource)]
pub struct RenderFrame {
	pub input: RenderInput2,
	pub context: RenderContextKey,
}
impl Deref for RenderFrame {
	type Target = RenderInput2;
	fn deref(&self) -> &Self::Target {
		&self.input
	}
}
impl DerefMut for RenderFrame {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.input
	}
}


#[derive(Debug, Component)]
pub struct CameraComponent {
	pub fovy: f32, // In radians, don't forget
	pub near: f32,
	pub far: f32,
}
impl CameraComponent {
	pub fn new() -> Self {
		Self {
			fovy: 45.0_f32.to_radians(),
			near: 0.1, // Self::near_from_fovy_degrees(45.0),
			far: 500.0,
		}
	}

	pub fn with_fovy_degrees(self, degrees: f32) -> Self {
		Self {
			fovy: degrees.to_radians(),
			near: Self::near_from_fovy_degrees(degrees),
			..self
		}
	}

	pub fn with_far(self, far: f32) -> Self {
		Self {
			far,
			..self
		}
	}

	fn near_from_fovy_degrees(fovy: f32) -> f32 {
		1.0 / (fovy.to_radians() / 2.0).tan()
	}

	pub fn set_fovy(&mut self, degrees: f32) {
		self.fovy = degrees.to_radians();
		self.near = Self::near_from_fovy_degrees(self.fovy);
	}
}


/// Writes camera buffer for the active context. 
pub fn context_camera_system(
	frame: Res<RenderFrame>,
	mut contexts: ResMut<ContextResource>,
	transforms: Comp<TransformComponent>,
	mut cameras: CompMut<CameraComponent>,
	mut buffers: ResMut<BufferResource>,
	textures: Res<TextureResource>,
) {
	let context = contexts.get_mut(frame.context).unwrap();

	#[repr(C)]
	#[derive(Debug, Pod, Zeroable, Clone, Copy)]
	struct CameraUniformData {
		near: f32,
		far: f32,
		fovy: f32,
		aspect: f32,
		position: Vec4,
		rotation: Mat4,
		view: Mat4,
		view_i: Mat4,
		projection: Mat4,
		projection_i: Mat4,
		view_projection: Mat4,
	}

	// let opengl_wgpu_matrix = Mat4 {
	// 	x_axis: Vec4::new(1.0, 0.0, 0.0, 0.0),
	// 	y_axis: Vec4::new(0.0, 1.0, 0.0, 0.0),
	// 	z_axis: Vec4::new(0.0, 0.0, 0.5, 0.5),
	// 	w_axis: Vec4::new(0.0, 0.0, 0.0, 1.0),
	// };

	if let Some(entity) = context.entity {
		if !cameras.contains(entity) {
			warn!("Insert camera component");
			cameras.insert(entity, CameraComponent::new());
		}
		let c = cameras.get(entity).unwrap();
		let t = transforms.get(entity).cloned().unwrap_or_default();
		
		let aspect_ratio = {
			let size = context.textures.get("output_texture")
				.cloned()
				.and_then(|k| textures.get(k))
				.and_then(|t| Some(t.size))
				.unwrap();
			size.width as f32 / size.height as f32
		};

		// opengl_wgpu_matrix * 
		let projection = Mat4::perspective_lh(c.fovy, aspect_ratio, c.near, c.far);
		let view = Mat4::from_rotation_translation(t.rotation, t.translation).inverse();
		let uniform = CameraUniformData {
			near: c.near,
			far: c.far,
			fovy: c.fovy,
			aspect: aspect_ratio,
			position: Vec4::new(t.translation.x, t.translation.y, t.translation.z, 1.0),
			rotation: Mat4::from_quat(t.rotation),
			view: Mat4::from_rotation_translation(t.rotation, t.translation).inverse(),
			view_i: Mat4::from_rotation_translation(t.rotation, t.translation),
			projection,
			projection_i: projection.inverse(),
			view_projection: projection * view,
		};
		let data = bytemuck::bytes_of(&uniform);

		if let Some(&key) = context.buffers.get(&"camera".to_string()) {
			// Write to buffer
			let buffer = buffers.get_mut(key).unwrap();
			buffer.write_queued(0, data);
		} else {
			let name = format!("RenderContext '{}' camera buffer", context.name);
			info!("Initialize {name}");
			// Create buffer init
			let buffer = Buffer::new_init(
				name, 
				data, 
				false,
				true,
				false,
			);
			let key = buffers.insert(buffer);
			context.buffers.insert("camera".to_string(), key);
		}
	}
}


#[derive(Component, Debug)]
pub struct RenderTargetSizeComponent {
	pub size: [u32; 2],
} 
// And then have a system that resizes the context's result texture?


#[derive(Debug, Component)]
pub struct OutputResolutionComponent {
	pub width: u32,
	pub height: u32,
}


pub fn output_texture_system(
	frame: Res<RenderFrame>,
	mut contexts: ResMut<ContextResource>,
	mut textures: ResMut<TextureResource>,
	output_resolutions: Comp<OutputResolutionComponent>,
) {
	let context = contexts.get_mut(frame.context).unwrap();
	
	if let Some(entity) = context.entity {
		if let Some(resolution) = output_resolutions.get(entity) {
			if let Some(key) = context.texture("output_texture") {
				// If resolution matches then terminate
				let t = textures.get_mut(key).unwrap();
				if resolution.width == t.size.width && resolution.height == t.size.height {
					return
				}

				info!("Rebuild output texure to size {}x{}", resolution.width, resolution.height);
				t.set_size(resolution.width, resolution.height, 1);

				// This is bad
				let k = textures.key_by_name(&"depth".to_string()).unwrap();
				let d = textures.get_mut(k).unwrap();
				d.set_size(resolution.width, resolution.height, 1);
			} else {
				let t = Texture::new_d2(
					"output_texture", 
					wgpu::TextureFormat::Rgba8UnormSrgb.into(), 
					resolution.width, resolution.height, 
					1, false, false, false, 
				).with_usages(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT);
				let key = textures.insert(t);
				context.insert_texture("output_texture", key);

				let d = textures.insert(Texture::new_d2(
					"depth", 
					wgpu::TextureFormat::Depth32Float.into(), 
					resolution.width, resolution.height, 
					1, false, false, false, 
				).with_usages(wgpu::TextureUsages::RENDER_ATTACHMENT));
				context.insert_texture("depth", d);
			}
		}
	}
}


#[derive(Debug, Component, Default)]
pub struct SSAOComponent {
	// No kernel settings because we can't adjust the sample count
	// Fixed size unifiorm buffer issue!
	pub kernel: Option<BufferKey>,

	pub noise_settings: SSAONoiseSettings,
	old_noise_settings: SSAONoiseSettings,
	pub noise: Option<TextureKey>,

	pub render_settings: SSAORenderSettings,
	old_render_settings: SSAORenderSettings,
	pub render_settings_buffer: Option<BufferKey>,

	pub output_settings: SSAOOutputTextureSettings,
	pub output: Option<TextureKey>,
	pub generate_mtl: Option<MaterialKey>,
	pub apply_mtl: Option<MaterialKey>,
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SSAONoiseSettings {
	pub width: u32,
	pub height: u32,
}
impl Default for SSAONoiseSettings {
	fn default() -> Self {
		Self {
			width: 4,
			height: 4,
		}
	}
}


// Used to tell the shader what do do with the information it is given
#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, PartialEq)]
pub struct SSAORenderSettings {
	pub tile_scale: f32, // Should depend on output resolution
	pub contrast: f32,
	pub bias: f32,
	pub radius: f32,
	// Unless we were to store ssao kernel in a storage buffer, it is stored in a uniform buffer
	// The shader therefore has a fixed kernel size and we can't adjust it without reloading it
	// pub kenerl_size: u32,
}
impl Default for SSAORenderSettings {
	fn default() -> Self {
		Self {
			tile_scale: 0.0,
			contrast: 0.5,
			bias: 0.0,
			radius: 1.0,
		}
	}
}


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SSAOOutputTextureSettings {
	pub scale: f32
}
impl Default for SSAOOutputTextureSettings {
	fn default() -> Self {
		Self {
			scale: 1.0,
		}
	}
}


// Todo: write new data to settings instread of making new buffer
pub fn ssao_system(
	mut contexts: ResMut<ContextResource>,
	mut frame: ResMut<RenderFrame>,
	mut buffers: ResMut<BufferResource>,
	mut textures: ResMut<TextureResource>,
	mut ssaos: CompMut<SSAOComponent>,
	mut materials: ResMut<MaterialResource>,
) {
	let context = contexts.get_mut(frame.context).unwrap();

	if let Some(ssao) = context.entity.and_then(|entity| ssaos.get_mut(entity)) {
		let kernel_dirty = ssao.kernel.is_none();

		if kernel_dirty {
			debug!("Rebuilding SSAO kernel");

			let data = make_ssao_kernel().iter().copied()
				.map(|v| [v.x, v.y, v.z, 0.0])
				.collect::<Vec<_>>();

			let key = *ssao.kernel.get_or_insert_with(|| {
				trace!("Initialize SSAO kernel");
				let key = buffers.insert(Buffer::new(
					"ssao kernel", 
					data.len() as u64 * 4 * 4, 
					false, 
					true, 
					true,
				));
				context.insert_buffer("ssao kernel", key);
				key
			});
			let b = buffers.get_mut(key).unwrap();
			
			b.write_queued(0, bytemuck::cast_slice(data.as_slice()));
		}

		let noise_dirty = ssao.noise.is_none() || ssao.noise_settings != ssao.old_noise_settings;
		
		if noise_dirty {
			debug!("Rebuilding SSAO noise");
			ssao.old_noise_settings = ssao.noise_settings;

			let key = *ssao.noise.get_or_insert_with(|| {
				trace!("Initialize SSAO noise");
				let key = textures.insert(Texture::new_d2(
					"ssao noise", 
					TextureFormat::Rgba32Float, 
					ssao.noise_settings.width, 
					ssao.noise_settings.height, 
					1, 
					false,
					true,
					false, 
				));
				context.insert_texture("ssao noise", key);
				key
			});
			let t = textures.get_mut(key).unwrap();

			let data = make_ssao_noise(ssao.noise_settings).iter()
				.copied()
				.map(|v| [v.x, v.y, 0.0, 0.0])
				.flatten()
				.collect::<Vec<_>>();
			t.set_size(ssao.noise_settings.width, ssao.noise_settings.height, 1);
			t.write_queued(0, wgpu::Origin3d::ZERO, bytemuck::cast_slice(data.as_slice()));
		}

		let render_dirty = ssao.render_settings_buffer.is_none() || ssao.render_settings != ssao.old_render_settings;

		if render_dirty {
			debug!("Rebuilding SSAO settings");
			ssao.old_render_settings = ssao.render_settings;

			let key = *ssao.render_settings_buffer.get_or_insert_with(|| {
				trace!("Initialize SSAO settings");
				let key = buffers.insert(Buffer::new(
					"ssao settings", 
					std::mem::size_of::<SSAORenderSettings>() as u64, 
					false, 
					true, 
					true,
				));
				context.insert_buffer("ssao settings", key);
				key
			});
			let b = buffers.get_mut(key).unwrap();
			b.write_queued(0, bytemuck::bytes_of(&ssao.render_settings));
		}

		let output_dirty = ssao.output
			.and_then(|k| textures.get(k))
			.and_then(|t| {
				let output_size = context.textures.get("output_texture").copied()
					.and_then(|k| textures.get(k))
					.unwrap().size;
				let width = (ssao.output_settings.scale * output_size.width as f32).round() as u32;
				let height = (ssao.output_settings.scale * output_size.height as f32).round() as u32;
				Some(t.size.width != width || t.size.height != height)
			})
			.unwrap_or(true);

		if output_dirty {
			debug!("Rebuilding SSAO output");
			let output_size = context.textures.get("output_texture").copied()
					.and_then(|k| textures.get(k))
					.unwrap().size;
			let width = (ssao.output_settings.scale * output_size.width as f32).round() as u32;
			let height = (ssao.output_settings.scale * output_size.height as f32).round() as u32;

			let key = *ssao.output.get_or_insert_with(|| {
				trace!("Initialize SSAO output");
				let key = textures.insert(Texture::new_d2(
					"ssao output", 
					TextureFormat::Rgba8Unorm, 
					width, 
					height, 
					1, 
					false,
					false,
					false, 
				).with_usages(wgpu::TextureUsages::RENDER_ATTACHMENT));
				context.insert_texture("ssao output", key);
				key
			});
			let t = textures.get_mut(key).unwrap();
			t.set_size(width, height, 1);
		}
		
		let ssao_generate_mtl = *ssao.generate_mtl.get_or_insert_with(|| {
			info!("Insert ssao generate material");
			materials.read("resources/materials/ssao_generate.ron")
		});
		frame.stage("ssao generate")
			.target(AbstractRenderTarget::new().with_colour(RRID::context("ssao output"), None))
			.pass(ssao_generate_mtl, Entity::default());

		let ssao_apply_mtl = *ssao.apply_mtl.get_or_insert_with(|| {
			info!("Insert ssao apply material");
			materials.read("resources/materials/ssao_apply.ron")
		});
		frame.stage("ssao apply")
			.run_after("ssao generate")
			.target(AbstractRenderTarget::new().with_colour(RRID::context("output_texture"), None))
			.pass(ssao_apply_mtl, Entity::default());
	}
}


// A hemispherical kernel of radius 1.0 facing +z
fn make_ssao_kernel() -> Vec<Vec3> {
	const KERNEL_SIZE: u32 = 64;

	#[inline(always)]
	fn lerp(a: f32, b: f32, f: f32) -> f32 {
		a + f * (b - a)
	}

	let mut rng = rand::thread_rng();
	(0..KERNEL_SIZE).map(|i| {
		// Hemisphere
		let v = Vec3::new(
			rng.gen::<f32>() * 2.0 - 1.0, 
			rng.gen::<f32>() * 2.0 - 1.0, 
			rng.gen::<f32>(),
		).normalize() * rng.gen::<f32>();

		// More samples closer to centre
		let scale = (i as f32) / ((KERNEL_SIZE - 1) as f32);
		let scale = lerp(0.1, 1.0, scale.powi(2));
		let v = v * scale;

		v
	}).collect()
}


// Random normal-tangent-space vectors
// These are only positive values, so do `* 2.0 - 1.0` in the shader
fn make_ssao_noise(settings: SSAONoiseSettings) -> Vec<Vec2> {
	let mut rng = rand::thread_rng();
	(0..(settings.width*settings.height)).map(|_| {
		Vec2::new(
			rng.gen::<f32>() * 2.0 - 1.0, 
			rng.gen::<f32>() * 2.0 - 1.0, 
		)
	}).collect()
}


#[derive(Debug, Component)]
pub struct AlbedoOutputComponent {
	pub width: u32,
	pub height: u32,
	texture: Option<TextureKey>,
}


// The albedo output should have a resolution equal to the output texture. 
pub fn context_albedo_system(
	frame: Res<RenderFrame>,
	mut contexts: ResMut<ContextResource>,
	mut textures: ResMut<TextureResource>,
	mut albedos: CompMut<AlbedoOutputComponent>,
	ouput_textures: Comp<OutputResolutionComponent>,
) {
	let context = contexts.get_mut(frame.context).unwrap();

	if let Some(entity) = context.entity {
		if let Some(output_texture) = ouput_textures.get(entity) {
			// Should probably do this elsewhere
			if !albedos.contains(entity) {
				albedos.insert(entity, AlbedoOutputComponent {
					width: output_texture.width,
					height: output_texture.height,
					texture: None,
				});
			}

			if let Some(albedo) = albedos.get_mut(entity) {
				let albedo_dirty = albedo.texture.is_none()
					|| albedo.width != output_texture.width
					|| albedo.height != output_texture.height;
				if albedo_dirty {
					albedo.width = output_texture.width;
					albedo.height = output_texture.height;

					let key = *albedo.texture.get_or_insert_with(|| {
						trace!("Initialize albedo texture");
						let key = textures.insert(Texture::new_d2(
							"ssao output", 
							TextureFormat::Rgba8Unorm, 
							albedo.width, 
							albedo.height, 
							1, 
							false,
							false,
							false, 
						).with_usages(wgpu::TextureUsages::RENDER_ATTACHMENT));
						context.insert_texture("albedo", key);
						key
					});
					let t = textures.get_mut(key).unwrap();
					debug!("Resize albedo texture to {}x{}", albedo.width, albedo.height);
					t.set_size(albedo.width, albedo.height, 1);
				}
			}
		}
	}
}


#[derive(Component, Debug)]
pub struct ModelComponent {
	pub material: MaterialKey,
	pub mesh: MeshKey,
}


pub fn skybox_render_system(
	mut materials: ResMut<MaterialResource>,
	mut frame: ResMut<RenderFrame>,
) {
	let stage = frame.stage("skybox")
		.run_before("models")
		.clear_depth(RRID::context("depth"), 1.0)
		.clear_texture(RRID::context("albedo"), [1.0; 4]);

	let skybox_mtl = materials.read("resources/materials/skybox.ron");
	stage
		.target(AbstractRenderTarget::new()
			.with_colour(RRID::context("albedo"), None)
			.with_depth(RRID::context("depth")))
		.pass(skybox_mtl, Entity::default());
}


pub fn model_render_system(
	// context: Res<ActiveContextResource>,
	// mut contexts: ResMut<ContextResource>, 
	models: Comp<ModelComponent>,
	mut input: ResMut<RenderFrame>,
) {
	// let context = contexts.get_mut(context.key).unwrap();

	let mut target = input
		.stage("models")
		.run_before("ssao generate")
		.target(AbstractRenderTarget::new()
			.with_colour(RRID::context("albedo"), None)
			.with_depth(RRID::context("depth")));
	for (entity, (model,)) in (&models,).iter().with_entities() {
		target.mesh(model.material, model.mesh, entity);
	}
}


pub fn spawn_test_model(
	mut entities: EntitiesMut,
	mut models: CompMut<ModelComponent>,
	mut meshes: ResMut<MeshResource>,
	mut materials: ResMut<MaterialResource>,
	mut transforms: CompMut<TransformComponent>,
) {
	let material = materials.read("resources/materials/grass.ron");
	let mesh = meshes.read_or("resources/meshes/box.obj", || Mesh::read_obj("resources/meshes/box.obj"));

	for p in [
		Vec3::new(0.0, 0.0, 0.0),
		Vec3::new(0.0, 0.0, 1.0),
		Vec3::new(0.0, 1.0, 0.0),
		Vec3::new(1.0, 0.0, 0.0),
		Vec3::new(0.0, -10.0, 0.0),
	] {
		let entity = entities.spawn();
		models.insert(entity, ModelComponent { material, mesh, });
		transforms.insert(entity, TransformComponent::new().with_position(p));
	}
}
