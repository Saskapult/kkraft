use crate::{query::Queriable, World};


pub trait SystemFunction<'q, Data, Args, R> {
	fn run_system(self, data: Data, world: &'q World) -> R;
}


/// Implements [SystemFunction] without input data
macro_rules! system_function {
	($($t:ident),+) => {
		impl<'q, Fun, $($t,)* R> SystemFunction<'q, (), ($($t, )*), R> for Fun 
		where 
			Fun: FnOnce($($t, )*) -> R + FnOnce($($t::Item, )*) -> R,
			$($t: Queriable<'q>, )* {
			fn run_system(self, _: (), world: &'q World) -> R {
				(self)(
					$(
						world.query::<$t>(),
					)*
				)
			}
		}
	};
}


/// Implements [SystemFunction] with input data tuple
macro_rules! system_data_function {
	(($($d:ident),+), ($($t:ident),+)) => {
		impl<'q, Fun, $($t,)* R, $($d,)*> SystemFunction<'q, ($($d,)*), ($($t,)*), R> for Fun 
		where 
			Fun: FnOnce(($($d,)*), $($t, )*) -> R + FnOnce(($($d,)*), $($t::Item, )*) -> R,
			$($t: Queriable<'q>, )* {
			fn run_system(self, data: ($($d,)*), world: &'q World) -> R {
				(self)(
					data,
					$(
						world.query::<$t>(),
					)*
				)
			}
		}
	};
}

/// Implements [SystemFunction] with input data tuple with length 0..=2
macro_rules! impl_system_function {
	($($t:ident),+) => {
		system_function!($($t),+);
		system_data_function!((D0), ($($t),+));
		system_data_function!((D0, D1), ($($t),+));
		system_data_function!((D0, D1, D2), ($($t),+));
	};
}


// I feel so smart right now :)
impl_system_function!(A);
impl_system_function!(A, B);
impl_system_function!(A, B, C);
impl_system_function!(A, B, C, D);
impl_system_function!(A, B, C, D, E);
impl_system_function!(A, B, C, D, E, F);
impl_system_function!(A, B, C, D, E, F, G);
impl_system_function!(A, B, C, D, E, F, G, H);
impl_system_function!(A, B, C, D, E, F, G, H, I);
impl_system_function!(A, B, C, D, E, F, G, H, I, J);
