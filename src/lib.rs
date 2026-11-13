#![feature(portable_simd)]

pub mod assets;
mod atomic_file;
pub mod cli;
pub mod compiler;
pub mod config;
pub mod fonts;
#[cfg(feature = "gui")]
pub mod gui;
pub mod outliner;
mod paths;
pub mod pdx;
pub mod scaling;
pub mod source_mods;
pub mod textures;
mod user_data;
pub mod validation;

#[cfg(test)]
#[path = "../tests/support/temp.rs"]
mod test_support;

#[cfg(test)]
#[path = "../tests/support/images.rs"]
mod test_images;
