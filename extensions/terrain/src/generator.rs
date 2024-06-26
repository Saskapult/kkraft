use std::path::Path;
use chunks::{array_volume::ArrayVolume, blocks::BlockKey, cube_iterator_xyz_uvec, CHUNK_SIZE};
use glam::{IVec2, IVec3, UVec2, UVec3};
use simdnoise::FbmSettings;
use splines::Spline;
use thiserror::Error;




#[derive(Error, Debug)]
pub enum GenerationError {
	#[error("failed to find block entry for '{0}'")]
	BlockNotFoundError(String),
}


fn load_spline(path: impl AsRef<Path>) -> anyhow::Result<Spline<f32, f32>> {
	let p = path.as_ref();
	let b = std::fs::read(p)?;
	let s = ron::de::from_bytes(b.as_slice())?;
	Ok(s)
}


/// This structure is used because [simdnoise::FbmSettings] does not implment [std::fmt::Debug] and also stores volume data. 
#[derive(Debug, Clone, Copy)]
struct RawFbmSettings {
	pub seed: i32,
	pub freq: f32,
    pub lacunarity: f32,
    pub gain: f32,
    pub octaves: u8,
}
impl RawFbmSettings {
	/// Multiply by this to map the noise to [-1.0, 1.0]
	pub fn compute_scale(&self) -> f32 {
		// Magic number derived from tests, is the analytical maximum output of one-octave noise
		let mut amp = 0.027125815;
		let mut scale = amp;
		for _ in 1..self.octaves {
			amp *= self.gain;
			scale += amp
		}
		1.0 / scale
	}
}
trait ConfigureRawFbm {
	fn apply_raw_settings(&mut self, settings: RawFbmSettings) -> &mut Self;
}
impl ConfigureRawFbm for FbmSettings {
	fn apply_raw_settings(&mut self, settings: RawFbmSettings) -> &mut Self {
		self
			.with_freq(settings.freq)
			.with_gain(settings.gain)
			.with_lacunarity(settings.lacunarity)
			.with_octaves(settings.octaves)
			.with_seed(settings.seed)
	}
}


#[inline]
fn lerp(x: f32, x1: f32, x2: f32, q00: f32, q01: f32) -> f32 {
	((x2 - x) / (x2 - x1)) * q00 + ((x - x1) / (x2 - x1)) * q01
}
// #[inline]
// fn lerp2(
// 	x: f32, y: f32, 
// 	q11: f32, q12: f32, q21: f32, q22: f32, 
// 	x1: f32, x2: f32, y1: f32, y2: f32,
// ) -> f32 {
// 	let r1 = lerp(x, x1, x2, q11, q21);
// 	let r2 = lerp(x, x1, x2, q12, q22);
// 	lerp(y, y1, y2, r1, r2)
// }
#[inline]
fn lerp3(
	x: f32, y: f32, z: f32, 
	q000: f32, q001: f32, q010: f32, q011: f32, q100: f32, q101: f32, q110: f32, q111: f32, 
	x1: f32, x2: f32, y1: f32, y2: f32, z1: f32, z2: f32, 
) -> f32 {
	let x00 = lerp(x, x1, x2, q000, q100);
	let x10 = lerp(x, x1, x2, q010, q110);
	let x01 = lerp(x, x1, x2, q001, q101);
	let x11 = lerp(x, x1, x2, q011, q111);

	let r0 = lerp(y, y1, y2, x00, x01);
	let r1 = lerp(y, y1, y2, x10, x11);
   
	lerp(z, z1, z2, r0, r1)
}


pub struct InteroplatedGeneratorNoise {
	data: Vec<f32>, 
	// data must extend to st's floor / inverse_scale and en's ceil / inverse_scale
	// data_st: IVec3,
	// data_en: IVec3,
	extent: u32,
	
	inverse_scale: u32, // One sample every inverse_scale voxels 
	st: IVec3,
	en: IVec3,
}
impl InteroplatedGeneratorNoise {
	#[inline]
	fn index_of(&self, pos: UVec3) -> usize {
		let [x, y, z] = pos.to_array();
		(x * self.extent * self.extent + y * self.extent + z) as usize
	}

	#[inline]
	pub fn get(&self, pos: IVec3) -> f32 {
		assert!(pos.cmpge(self.st).all());
		assert!(pos.cmplt(self.en).all());

		// Start of data in word space
		let data_st = self.st.div_euclid(IVec3::splat(self.inverse_scale as i32)) * self.inverse_scale as i32;

		let pos_data_relative = (pos - data_st).as_uvec3();

		let base_data_cell =  pos_data_relative / self.inverse_scale;
		let q000 = self.data[self.index_of(base_data_cell)];
		let q001 = self.data[self.index_of(base_data_cell + UVec3::X)];
		let q010 = self.data[self.index_of(base_data_cell + UVec3::Y)];
		let q011 = self.data[self.index_of(base_data_cell + UVec3::X + UVec3::Y)];
		let q100 = self.data[self.index_of(base_data_cell + UVec3::Z)];
		let q101 = self.data[self.index_of(base_data_cell + UVec3::Z + UVec3::X)];
		let q110 = self.data[self.index_of(base_data_cell + UVec3::Z + UVec3::Y)];
		let q111 = self.data[self.index_of(base_data_cell + UVec3::Z + UVec3::Y + UVec3::X)];

		// pos
		let [x, y, z] = pos_data_relative.as_vec3().to_array();
		// pos of q000 
		let [x1, y1, z1] = (base_data_cell * self.inverse_scale).as_vec3().to_array();
		// pos of q111 
		let [x2, y2, z2] = ((base_data_cell + UVec3::ONE) * self.inverse_scale).as_vec3().to_array();

		lerp3(x, y, z, q000, q001, q010, q011, q100, q101, q110, q111, x1, x2, y1, y2, z1, z2)
	}
}


/// Splines are loaded from disk when calling [Self::new]. 
/// If something fails during that, the prgoram will panic.  
#[derive(Debug)]
pub struct NewTerrainGenerator {
	// The noise used to determine the base density of a voxel
	density_noise: RawFbmSettings,
	density_threshold: f32,
	// Density adjustment, difference from intended height -> density adjustment
	density_spline: Spline<f32, f32>,

	// The noise used to determine the intended height of the world
	height_noise: RawFbmSettings,
	// Maps raw height noise -> intended world terrain height
	height_spline: Spline<f32, f32>,

	// The noise used to create a multiplier for the difference from intended height
	// Think of this as a "weirdness" value
	height_difference_noise: RawFbmSettings,
	// Maps raw height difference noise -> height difference multiplier
	height_difference_spline: Spline<f32, f32>,
}
impl NewTerrainGenerator {
	pub fn new(seed: i32) -> Self {
		Self {
			density_noise: RawFbmSettings {
				seed,
				freq: 1.0 / 50.0,
				lacunarity: 2.0,
				gain: 0.5,
				octaves: 3,
			},
			density_threshold: 0.5,
			density_spline: load_spline("resources/density_spline.ron").unwrap(),
			height_noise: RawFbmSettings {
				seed: seed + 1,
				freq: 1.0 / 1000.0,
				lacunarity: 2.0,
				gain: 0.5,
				octaves: 1,
			},
			height_spline: load_spline("resources/height_spline.ron").unwrap(),
			height_difference_noise: RawFbmSettings {
				seed: seed + 2,
				freq: 1.0 / 100.0,
				lacunarity: 2.0,
				gain: 0.5, 
				octaves: 1,
			},
			height_difference_spline: load_spline("resources/difference_spline.ron").unwrap(),
			
		}
	}

	pub fn max_height(_world_position: IVec2, _extent: UVec2) -> Option<Vec<i32>> {
		// None if the max x key's y value in density adjustment is not 1
		todo!("Max height")
	}

	/// A lookahead method for knowing if a block will be solid. 
	/// Can generate single positions, columns, or whole chunks worth of solidity data! 
	fn is_solid(
		&self, 
		world_position: IVec3,
		extent: UVec3,
	) -> Vec<bool> {
		let [x_offset, y_offset, z_offset] = world_position.to_array();
		let [x_extent, y_extent, z_extent] = extent.to_array();

		// Sample height (2d fbm -> height spline)
		// Outputs in yx order
		let height_scale = self.height_noise.compute_scale();
		let heights = simdnoise::NoiseBuilder::fbm_2d_offset(
			x_offset as f32 + 0.5, x_extent as usize, 
			z_offset as f32 + 0.5, z_extent as usize,
		).apply_raw_settings(self.height_noise).generate().0.into_iter()
			.map(|d| (d * height_scale + 1.0) / 2.0) // Normalize
			.map(|height_noise| {
				self.height_spline.clamped_sample(height_noise).unwrap()
			})
			.collect::<Vec<_>>();

		let height_difference_scale = self.height_difference_noise.compute_scale();
		let height_differences = simdnoise::NoiseBuilder::fbm_2d_offset(
			x_offset as f32 + 0.5, x_extent as usize, 
			z_offset as f32 + 0.5, z_extent as usize,
		).apply_raw_settings(self.height_difference_noise).generate().0.into_iter()
			.map(|d| (d * height_difference_scale + 1.0) / 2.0) // Normalize
			.map(|noise| {
				self.height_difference_spline.clamped_sample(noise).unwrap()
			})
			.collect::<Vec<_>>();

		// This information can be used to know if we should skip (fill or leave empty) this chunk
		// If it's below the density = 1.0 cutoff (or the -1.0 one) then it can be filled 
		// Problem with that: it assumes that our spline ends with 1.0 and -1.0
		// We might not do that! (floating islands, caves)
		// Given the speed of my benchmarks, it should not be needed either

		// Outputs in zyx order
		let density_scale = self.density_noise.compute_scale();
		let densities = simdnoise::NoiseBuilder::fbm_3d_offset(
			x_offset as f32 + 0.5, x_extent as usize, 
			y_offset as f32 + 0.5, y_extent as usize, 
			z_offset as f32 + 0.5, z_extent as usize,
		).apply_raw_settings(self.density_noise).generate().0.into_iter()
			.map(|d| (d * density_scale + 1.0) / 2.0) // Normalize
			.collect::<Vec<_>>();

		// Because simd_noise outputs in zyx/yx order, we can't just zip() here
		cube_iterator_xyz_uvec(extent)
			.map(|p| (p, p.as_ivec3() + world_position))
			.map(|(p, world_pos)| {
				let density = densities[(
					p.z * y_extent * x_extent +
					p.y * x_extent +
					p.x
				) as usize];
				let height = heights[(
					p.z * x_extent +
					p.x
				) as usize];
				let height_difference = height_differences[(
					p.z * x_extent +
					p.x
				) as usize];

				let height_diff = (height - world_pos.y as f32) * height_difference;
				let density_adjustment = self.density_spline.clamped_sample(height_diff).unwrap();
				density + density_adjustment
			})
			.map(|d| d >= self.density_threshold).collect()

		// cube_iterator_xyz_uvec(extent).map(|p| {
		// 	(p.as_ivec3() + world_position).y < 0
		// }).collect()
	}

	// Generates the base solid blocks for a chunk
	pub fn base(
		&self, 
		chunk_position: IVec3, 
		volume: &mut ArrayVolume<BlockKey>,
		base: BlockKey,
	) {

		let solidity = self.is_solid(chunk_position * CHUNK_SIZE as i32, UVec3::splat(CHUNK_SIZE));

		// We could map and insert directly into the array volume, 
		// but that would require knowing the indexing implementation 
		// and I don't want to make that assumption
		for (pos, solid) in cube_iterator_xyz_uvec(UVec3::splat(CHUNK_SIZE)).zip(solidity) {
			if solid {
				volume.insert(pos, base);
			}
		}
	}

	// Carve should be split into cheese, spaghetti, and noodles
	#[deprecated]
	pub fn carve(
		&self, 
		_chunk_position: IVec3, 
		_volume: &mut ArrayVolume<BlockKey>,
	) {
		todo!()
	}

	// Uses [Self::is_solid] lookahead to place covering blocks
	// This does re-generate all of the solidity data in order to do that
	// It would be better to share the solidity data
	// But it's much easier to just do this
	pub fn cover(
		&self,
		chunk_position: IVec3, 
		volume: &mut ArrayVolume<BlockKey>,
		top: BlockKey,
		fill: BlockKey,
		fill_depth: i32, // n following top placement
	) {
		for x in 0..CHUNK_SIZE {
			for z in 0..CHUNK_SIZE {
				// Generate column solidity
				// The orderign of this might be wrong, just do .rev() if it is
				let solidity = self.is_solid(
					CHUNK_SIZE as i32 * chunk_position + IVec3::new(x as i32, 0, z as i32), 
					UVec3::new(1, CHUNK_SIZE + fill_depth as u32, 1),
				);

				let mut fill_to_place = 0;
				let mut last_was_empty = false;
				// Descend y
				for (y, solid) in solidity.into_iter().enumerate().rev() {
					// Never set an empty voxel
					if !solid {
						// Reset fill counter
						last_was_empty = true;
						fill_to_place = 0;
						continue
					} else {
						let in_chunk = y < CHUNK_SIZE as usize;

						// Set top if exposed on top
						if last_was_empty {
							// Begin placing fill
							fill_to_place = fill_depth;
							if in_chunk {
								// This y could be wrong
								volume.insert(UVec3::new(x as u32, y as u32, z as u32), top);
							}
						} else {
							// If not exposed and more fill to place, set fill
							if fill_to_place != 0 {
								fill_to_place -= 1;
								if in_chunk {
									// This y could be wrong
									volume.insert(UVec3::new(x as u32, y as u32, z as u32), fill);
								}
							}
						}

						last_was_empty = false;
					}
				}
			}
		}
	}

	#[deprecated]
	pub fn treeify(
		&self, 
		_chunk_position: IVec3, 
		_volume: &ArrayVolume<BlockKey>,
	) -> bool { // Should return block modifications
		todo!("Tree generation should be extended into a structure generation script")
	}
}


#[cfg(test)]
pub mod tests {
	use super::*;
	use test::Bencher;

	/// Tests that my magic scaling number is still working 
	#[test]
	fn test_noise_normalization() {
		let settings = RawFbmSettings {
			seed: 0,
			freq: 1.0,
			lacunarity: 1.0,
			gain: 2.5,
			octaves: 6,
		};

		let extent = 256;
		let (noise, _, _) = simdnoise::NoiseBuilder::fbm_3d(extent, extent, extent).apply_raw_settings(settings).generate();
		
		let min = noise.iter().min_by(|a, b| a.total_cmp(b)).unwrap();
		let max = noise.iter().max_by(|a, b| a.total_cmp(b)).unwrap();
		println!("Max {max}, Min {min}");

		let scale = settings.compute_scale();
		println!("Scale {scale}");
		let normed = noise.into_iter().map(|v| (v * scale + 1.0) / 2.0).collect::<Vec<_>>();

		let min = normed.iter().min_by(|a, b| a.total_cmp(b)).unwrap();
		let max = normed.iter().max_by(|a, b| a.total_cmp(b)).unwrap();
		println!("Max {max}, Min {min}");

		assert!(normed.iter().copied().all(|v| v <= 1.0));
		assert!(normed.iter().copied().all(|v| v >= 0.0));
	}

	#[bench]
	fn bench_interpolated_noise(b: &mut Bencher) {
		let inverse_scale: u32 = 4;
		let extent: usize = 32;

		let settings = RawFbmSettings {
			seed: 42,
			freq: 1.0 / 50.0 / inverse_scale as f32,
			lacunarity: 2.0,
			gain: 0.5,
			octaves: 3,
		};

		b.iter(|| {
			let world_pos = rand::random::<IVec3>();
			let st = world_pos / extent as i32 * extent as i32;
			let en = st + IVec3::splat(extent as i32);
			let [x_offset, y_offset, z_offset] = st.as_vec3().to_array();

			let sampling_extent = (extent / inverse_scale as usize) + 1;
			let data = simdnoise::NoiseBuilder::fbm_3d_offset(
				x_offset as f32 + 0.5, sampling_extent, 
				y_offset as f32 + 0.5, sampling_extent, 
				z_offset as f32 + 0.5, sampling_extent,
			).apply_raw_settings(settings).generate().0;

			let interp = InteroplatedGeneratorNoise {
				data,
				inverse_scale,
				st,
				en,
				extent: extent as u32 / inverse_scale,
			};

			let output = cube_iterator_xyz_uvec(UVec3::splat(extent as u32)).map(|p| interp.get(st + p.as_ivec3())).collect::<Vec<_>>();
			
			output
		});
	}

	#[bench]
	fn bench_uninterpolated_noise(b: &mut Bencher) {
		let extent: usize = 32;

		let settings = RawFbmSettings {
			seed: 42,
			freq: 1.0 / 50.0,
			lacunarity: 2.0,
			gain: 0.5,
			octaves: 3,
		};

		b.iter(|| {
			let world_pos = rand::random::<IVec3>();
			let st = world_pos / extent as i32 * extent as i32;
			let [x_offset, y_offset, z_offset] = st.as_vec3().to_array();

			let data = simdnoise::NoiseBuilder::fbm_3d_offset(
				x_offset as f32 + 0.5, extent, 
				y_offset as f32 + 0.5, extent, 
				z_offset as f32 + 0.5, extent,
			).apply_raw_settings(settings).generate().0;

			data
		});
	}

	// /// Generates chunks until one is fully solid and another is fully empty
	// #[test]
	// fn test_density_falloff() {
	// 	let base = 0;
	// 	let x = 0;
	// 	let z = 0;
	// 	let mut y_min = None;
	// 	let mut y_max = None;
	// 	let max_look_length = 10; // Look five chunks up or down

	// 	let generator = NewTerrainGenerator::new(0);

	// 	println!("Looking up...");
	// 	for y in base..=base+max_look_length {
	// 		let chunk_position = IVec3::new(x, y, z);
	// 		let mut volume = ArrayVolume::new(UVec3::splat(CHUNK_SIZE));
	// 		generator.base(chunk_position, &mut volume, BlockKey::default());

	// 		let n_solid = volume.contents.iter().filter(|v| v.is_some()).count();
	// 		println!("y={y} is {:.2}% solid ({} / {})", n_solid as f32 / CHUNK_SIZE.pow(3) as f32 * 100.0, n_solid, CHUNK_SIZE.pow(3));

	// 		if volume.contents.iter().all(|v| v.is_none()) {
	// 			println!("y={y} is fully empty");
	// 			y_max = Some(y);
	// 			break
	// 		}
	// 	}
	// 	assert!(y_max.is_some(), "No fully empty chunk found");

	// 	println!("Looking down...");
	// 	for y in (base-max_look_length..=base).rev() {
	// 		let chunk_position = IVec3::new(x, y, z);
	// 		let mut volume = ArrayVolume::new(UVec3::splat(CHUNK_SIZE));
	// 		generator.base(chunk_position, &mut volume, BlockKey::default());

	// 		let n_solid = volume.contents.iter().filter(|v| v.is_some()).count();
	// 		println!("y={y} is {:.2}% solid ({} / {})", n_solid as f32 / CHUNK_SIZE.pow(3) as f32 * 100.0, n_solid, CHUNK_SIZE.pow(3));

	// 		if volume.contents.iter().all(|v| v.is_some()) {
	// 			println!("y={y} is fully solid");
	// 			y_min = Some(y);
	// 			break
	// 		}
	// 	}
	// 	assert!(y_min.is_some(), "No fully solid chunk found");
	// }

	#[test]
	fn test_3d_fbm_index() {
		let settings = RawFbmSettings {
			seed: 0,
			freq: 0.05,
			lacunarity: 1.0,
			gain: 2.5,
			octaves: 6,
		};

		let distance = 15;

		let (noise, _, _) = simdnoise::NoiseBuilder::fbm_3d_offset(
			0.0, distance, 
			0.25, 1, 
			0.25, 1,
		).apply_raw_settings(settings).generate();
		let a = noise[distance-1];
		println!("{noise:?}");
		dbg!(a);

		let (noise, _, _) = simdnoise::NoiseBuilder::fbm_3d_offset(
			(distance-2) as f32, 3, 
			0.25, 1, 
			0.25, 1,
		).apply_raw_settings(settings).generate();
		let b = noise[1];
		println!("{noise:?}");
		dbg!(b);

		assert!(a - b <= f32::EPSILON);
	}
}
