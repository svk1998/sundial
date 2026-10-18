#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod store;
mod stats;
mod tracker;

fn main() {
    println!("sundial");
}
