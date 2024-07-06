pub mod light;

use controls::ControlMap;
use eeks::prelude::*;
use light::{torchlight_chunk_init_system, torchlight_debug_place_system, torchlight_update_system, TorchLightChunksResource, TorchLightModifierComponent};
use player::PlayerSpawnResource;

#[macro_use]
extern crate log;



fn player_light_modifier(
	psr: Res<PlayerSpawnResource>,
	mut controls: ResMut<ControlMap>,
	mut vm: CompMut<TorchLightModifierComponent>,
) {
	for entity in psr.entities.iter().copied() {
		vm.insert(entity, TorchLightModifierComponent::new(&mut controls));
	}
}


#[info]
pub fn dependencies() -> Vec<String> {
	env_logger::init();
	vec![
		"chunks".into(),
		"terrain".into(),
	]
}


#[systems]
pub fn systems(loader: &mut ExtensionSystemsLoader) {
	loader.system("client_tick", "torchlight_chunk_init_system", torchlight_chunk_init_system)
		.run_after("chunk_loading_system");

	loader.system("client_tick", "torchlight_update_system", torchlight_update_system)
		.run_after("torchlight_chunk_init_system");

	loader.system("client_tick", "torchlight_debug_place_system", torchlight_debug_place_system)
		.run_before("torchlight_update_system");

	loader.system("client_tick", "player_light_modifier", player_light_modifier)
		.run_after("player_spawn")
		.run_before("player_spawned");
}


#[load]
pub fn load(storages: &mut eeks::ExtensionStorageLoader) {
	storages.resource(TorchLightChunksResource::default());
	storages.component::<TorchLightModifierComponent>();
}
