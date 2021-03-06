use specs::prelude::*;
use winit::event_loop::*;
use nalgebra::*;
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::{Instant, Duration};
use std::sync::{Arc, RwLock};
use rapier3d::prelude::*;
use crate::ecs::*;
use crate::window::*;
use crate::mesh::*;
use crate::texture::*;
use crate::material::*;




pub struct Game {
	pub world: World,
	
	blocks_manager: Arc<RwLock<crate::world::BlockManager>>,
	
	window_manager: WindowManager,

	tick_dispatcher: Dispatcher<'static, 'static>,
	last_tick: Instant,
	entity_names_map: HashMap<Entity, String>,

	marker_entity: Option<Entity>,
	can_modify_block: bool,
}
impl Game {
	pub fn new(
		event_loop_proxy: EventLoopProxy<EventLoopEvent>, 
		event_loop_receiver: Receiver<ResponseFeed>,
	) -> Self {
		let instance = wgpu::Instance::new(wgpu::Backends::all());
		let adapter = pollster::block_on(instance.request_adapter(
			&wgpu::RequestAdapterOptions {
				power_preference: wgpu::PowerPreference::HighPerformance, // Dedicated GPU
				compatible_surface: None, // Some(&surface)
				force_fallback_adapter: false, // Don't use software renderer
			},
		)).unwrap();

		let adapter_info = adapter.get_info();
		info!("Kkraft using device {} ({:?})", adapter_info.name, adapter_info.backend);
		info!("Features: {:?}", adapter.features());
		info!("Limits: {:?}", adapter.limits());

		let blocks_manager = Arc::new(RwLock::new(crate::world::BlockManager::new()));

		let mut world = World::new();

		// Register components
		world.register::<TransformComponent>();
		world.register::<MovementComponent>();
		world.register::<ModelComponent>();
		world.register::<MapComponent>();
		world.register::<CameraComponent>();
		world.register::<DynamicPhysicsComponent>();
		world.register::<StaticPhysicsComponent>();
		world.register::<MarkerComponent>();

		// Attach resources
		let step_resource = StepResource::new();
		world.insert(step_resource);

		let gpu_resource = GPUResource::new(
			&adapter,
			&Arc::new(RwLock::new(TextureManager::new())),
			&Arc::new(RwLock::new(MeshManager::new())),
			&Arc::new(RwLock::new(MaterialManager::new())),
		);
		world.insert(gpu_resource);

		let input_resource = InputResource::new();
		world.insert(input_resource);

		let physics_resource = PhysicsResource::new();
		world.insert(physics_resource);

		let tick_dispatcher = DispatcherBuilder::new()
			// .with(MovementSystem, "movement", &[])
			.with(MapLoadingSystem, "map loading", &[])
			.build();

		let window_manager = WindowManager::new(
			instance,
			adapter,
			event_loop_proxy,
			event_loop_receiver,
		);

		Self {
			world,
			blocks_manager,
			window_manager,
			tick_dispatcher,
			last_tick: Instant::now(),
			entity_names_map: HashMap::new(),
			marker_entity: None,
			can_modify_block: true,
		}
	}

	pub fn setup(&mut self) {
		// Material loading
		{
			let mut gpu = self.world.write_resource::<GPUResource>();

			let mut matm = gpu.data.materials.data_manager.write().unwrap();
			let mut texm = gpu.data.textures.data_manager.write().unwrap();

			// Load some materials
			load_materials_file(
				"resources/materials/kmaterials.ron",
				&mut texm,
				&mut matm,
			).unwrap();

			// Load my thingy
			drop(matm);
			drop(texm);
			gpu.data.shaders.register_path(&std::path::PathBuf::from("./resources/shaders/ray_test.ron"));
			gpu.data.shaders.register_path(&std::path::PathBuf::from("./resources/shaders/blit.ron"));
		}

		// Block loading
		{
			let mut bm = self.blocks_manager.write().unwrap();

			let gpu = self.world.write_resource::<GPUResource>();
			let mut mm = gpu.data.materials.data_manager.write().unwrap();
			let mut tm = gpu.data.textures.data_manager.write().unwrap();

			crate::world::blocks::load_blocks_file(
				"resources/kblocks.ron",
				&mut bm,
				&mut tm,
				&mut mm,
			).unwrap();
		}

		
		// // Map
		// self.world.create_entity()
		// 	.with(TransformComponent::new())
		// 	.with(MapComponent::new(&self.blocks_manager))
		// 	.build();
		
	}

	pub fn tick(&mut self) {
		// Run window update
		self.window_manager.read_input();
		for (_, window) in self.window_manager.windows.iter_mut() {
			window.game_widget.tracked_entity.get_or_insert_with(|| {
				self.world.create_entity()
					.with(CameraComponent::new())
					.with(
						TransformComponent::new()
						.with_position(Vector3::new(0.5, 0.5, -10.5))
					)
					.with(MovementComponent{speed: 4.0})
					.build()
			});

			window.game_input.end();

			// Move things
			let mut tcs = self.world.write_component::<TransformComponent>();
			let mcs = self.world.read_component::<MovementComponent>();
			if let Some(entity) = window.game_widget.tracked_entity {
				if let Some(mv) = mcs.get(entity) {
					if let Some(tc) = tcs.get_mut(entity) {
						crate::ecs::apply_input(&window.game_input, tc, mv);
					}
				}
			}
		}

		// Do ticking stuff
		if self.last_tick.elapsed() > Duration::from_secs_f32(1.0 / 30.0) {
			self.last_tick = Instant::now();
			self.tick_dispatcher.dispatch(&self.world);
			let mut input_resource = self.world.write_resource::<InputResource>();
			input_resource.last_read = Instant::now();
		}
		
		// Show windows
		{
			let mut gpu_resource = self.world.write_resource::<GPUResource>();
			for (_, window) in self.window_manager.windows.iter_mut() {
				window.game_input.reset();
				window.update(
					&mut gpu_resource,
					&self.world,
				);
			}
		}
	}

	

	pub fn new_window(&mut self) {
		info!("Requesting new game window");

		self.window_manager.request_new_window();
	}
}




