#[cfg(not(feature = "ecp"))]
use wasm_bindgen::prelude::*;

#[macro_use]
extern crate impl_ops;

pub mod accelerators;
pub mod blockqueue;
pub mod cameras;
pub mod core;
mod entry;
pub mod filters;
pub mod integrators;
pub mod lights;
pub mod materials;
pub mod media;
pub mod samplers;
pub mod shapes;
pub mod textures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_entry_test() {
        lib_entry(16);
    }
}

#[cfg(not(feature = "ecp"))]
#[wasm_bindgen]
pub fn lib_entry(tile_size: i32) -> Vec<u8> {
    entry::entry(true, tile_size, None, None)
}
