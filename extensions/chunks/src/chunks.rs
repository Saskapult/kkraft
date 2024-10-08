use std::{sync::Arc, collections::HashMap};
use eeks::prelude::*;
use glam::{IVec3, UVec3, Vec3};
use parking_lot::RwLock;
use slotmap::{SlotMap, new_key_type};
use pinecore::transform::TransformComponent;

use crate::{chunk_of_point, VoxelCube};


new_key_type! {
	pub struct ChunkKey;
}


/// The chunks need to be shared across many different systems. 
/// Some of those systems, such as meshing, happen in long-running jobs. 
/// As such, we need some way to share the chunks between them. 
/// 
/// It would be best to have a hybrid approach. 
/// Clone iff something else is being read from. 
/// I do not, however, want to bother doing that. 
/// 
/// Arc:
/// - Can make_mut and just get on with things
/// - Might be a large clone
///   - 15x15x15 8-byte keys with 12-byte positions is 67.5KB, but the size scales cubically and that scares me a little
/// 
/// RwLock:
/// - Need to keep locking and unlocking when looking for chunk existence
/// 
/// I've chosen to use an Arc for this. 
/// This may cause problems as volumes increase in size. 
/// The meshing system holds a copy of this resource for the duration of its meshing. 
/// Be aware of this. 
#[derive(Debug, Resource, Clone)]
pub struct ChunksResource(Arc<RwLock<Chunks>>);
impl ChunksResource {
	pub fn new() -> Self {
		Self(Arc::new(RwLock::new(Chunks::new())))
	}
}
impl std::ops::Deref for ChunksResource {
	type Target = Arc<RwLock<Chunks>>;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}


/// The chunks resouce describes the chunks of the world that are loaded. 
/// Other resources, such as for terrain and lighting, are based on this. 
#[derive(Debug, Clone)]
pub struct Chunks {
	// We could reduce memory usage by not storing positions here but it is 
	// useful to iterate over these
	pub chunks: SlotMap<ChunkKey, IVec3>,
	/// All chunk keys, but sorted by distance to a loader. 
	/// Be aware that distance is not linear distance. 
	pub chunks_by_distance: Vec<(ChunkKey, i32)>,
	// pub min: IVec3,
	// pub max: IVec3,

	// This will always work and the memory usage scales linearly
	pub hm: HashMap<IVec3, ChunkKey>,
	
	// // If the volume of av is less than or eqal to av_max, this is some
	// pub av: Option<(ArrayVolume<ChunkKey>, IVec3)>, // volume and its min corner
	// // Limit is used to not use a large amout of memory for this
	// pub av_max: usize,

	// // Octree will use less memory than arrayvolume when mostly empty
	// pub octree: bool,
	// pub octree_min: IVec3,
	// pub octree_max: IVec3,
}
impl Chunks {
	pub fn new() -> Self {
		Self {
			chunks: SlotMap::with_key(),
			chunks_by_distance: Vec::new(),
			hm: HashMap::new(),
		}
	}

	/// Fetch the [ChunkKey] of a position if it exists. 
	pub fn get_position(&self, pos: IVec3) -> Option<ChunkKey> {
		// Perfer array volume
		// Perfer octree
		// Fall back on hashmap
		self.hm.get(&pos).copied()
	}

	/// Create a key for this position and 
	pub fn load(&mut self, pos: IVec3) -> ChunkKey {
		match self.get_position(pos) {
			Some(k) => k,
			None => {
				let k = self.chunks.insert(pos);
				self.hm.insert(pos, k);
				k
			},
		}
	}

	pub fn post_load(&mut self) {
		
	}

	pub fn unload(&mut self, key: ChunkKey) {
		if let Some(p) = self.chunks.remove(key) {
			self.hm.remove(&p);
		}
	}

	pub fn post_unload(&mut self) {

	}
}



#[derive(Debug, Component)]
pub struct ChunkLoadingComponent {
	pub radius: i32,
	pub tolerence: i32,
}
impl ChunkLoadingComponent {
	pub fn new(radius: i32) -> Self { 
		assert!(radius > 0);
		Self { radius, tolerence: 2, }
	}

	pub fn loading_volume(&self, pos: Vec3) -> VoxelCube {
		VoxelCube::new(chunk_of_point(pos), UVec3::splat(self.radius as u32))
	}

	// Volume but expanded by tolerence
	pub fn un_loading_volume(&self, pos: Vec3) -> VoxelCube {
		VoxelCube::new(chunk_of_point(pos), UVec3::splat((self.radius + self.tolerence) as u32))
	}
}


pub fn chunk_loading_system(
	chunks: ResMut<ChunksResource>,
	map_loaders: Comp<ChunkLoadingComponent>,
	transforms: Comp<TransformComponent>,
) { 
	// loading_volumes.iter()
	// 	.flat_map(|(loader_chunk, volume)| 
	// 		volume.iter()
	// 		.filter(|&p| chunks_read.get_position(p).is_none())
	// 		.map(|p| (p, loader_chunk.distance_squared(p)))) 
	// 	.collect::<Vec<_>>()
	// let loading_volumes = (&map_loaders, &transforms).iter()
	// 	.map(|(loader, transform)| (chunk_of_point(transform.translation), loader.loading_volume(transform.translation)))
	// 	.collect::<Vec<_>>();

	let loading_volumes = (&map_loaders, &transforms).iter()
		.map(|(loader, transform)| (chunk_of_point(transform.translation), loader.loading_volume(transform.translation)))
		.collect::<Vec<_>>();

	let un_loading_volumes = (&map_loaders, &transforms).iter()
		.map(|(loader, transform)| loader.un_loading_volume(transform.translation))
		.collect::<Vec<_>>();

	let chunks_read = chunks.read();

	// Note: This is not inexpensive!
	// In a debug build, hashmap lookups take ~278ns, 19^3 lookups in ~1.6ms
	// In a release build, hashmap lookups take ~19ns, 19^3 lookups in ~0.1ms
	// FxHash is much faster
	// In a debug build, FxHashMap lookups take ~147ns, 19^3 lookups in ~1.0ms
	// In a release build, hashmap lookups take ~4ns, 19^3 lookups in ~0.03ms
	// This data was collected using benchmarks and is reflected by profiling
	let chunks_to_load = {
		// profiling::scope!("Collect chunks to load");
		loading_volumes.iter()
			.flat_map(|(_, lv)| lv.iter())
			.filter(|&pos| chunks_read.get_position(pos).is_none()).collect::<Vec<_>>()
	};

	let chunks_to_prune = {
		// profiling::scope!("Collect chunks to prune");
		chunks_read.chunks.iter()
			.filter(|(_, &p)| !un_loading_volumes.iter()
				.any(|lv| lv.contains(p)))
			.map(|(k, &p)| (k, p))
			.collect::<Vec<_>>()
	};

	drop(chunks_read);
	let mut chunks_write = chunks.write();

	{ // Prune chunks that should not be loaded
		// profiling::scope!("Prune chunks");
		if chunks_to_prune.len() != 0 {
			debug!("Prune {} chunks", chunks_to_prune.len());
		}
		for (key, _) in chunks_to_prune {
			if let Some(p) = chunks_write.chunks.remove(key) {
				chunks_write.hm.remove(&p);
			}
		}
		let chunks = &mut *chunks_write;
		chunks.chunks_by_distance.retain(|&(key, _)| chunks.chunks.contains_key(key));
	}

	{ // Insert entries for chunks that should be in the HM but are not
		// profiling::scope!("Insert chunk entries");
		if chunks_to_load.len() != 0 {			
			debug!("Load {} chunks", chunks_to_load.len());
		}
		for position in chunks_to_load {
			let key = match chunks_write.get_position(position) {
				Some(k) => k,
				None => {
					let k = chunks_write.chunks.insert(position);
					chunks_write.hm.insert(position, k);
					k
				},
			};
			chunks_write.chunks_by_distance.push((key, 0));
		}
	}

	{ // Sort chunks by distance to a loader 
		// profiling::scope!("Sort chunks by distance");
		let chunks = &mut *chunks_write;
		for (k, d) in chunks.chunks_by_distance.iter_mut() {
			let p = *chunks.chunks.get(*k).unwrap();
			*d = loading_volumes.iter()
				.map(|(c, _)| c.distance_squared(p))
				.min().unwrap();
		}

		insertion_sort_by_key(&mut chunks_write.chunks_by_distance, |&(_, p)| p);
		assert_eq!(chunks_write.chunks.len(), chunks_write.chunks_by_distance.len());
	}
}


// https://medium.com/@spyr1014/sorting-in-rust-selection-insertion-and-counting-sort-2c4d3575e364
// It's so short! So elegant! 
fn insertion_sort_by_key<T, K: Ord + Copy, F: FnMut(&T) -> K>(data: &mut [T], mut f: F) {
	for i in 1..data.len() {
		for j in (1..i+1).rev() {
			if f(&data[j-1]) <= f(&data[j]) { 
				break 
			}
			data.swap(j-1, j);
		}
	}
}
