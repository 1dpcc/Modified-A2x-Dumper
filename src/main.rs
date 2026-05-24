#![allow(dead_code)]
#![allow(unused_imports)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::Result;

mod analysis;
mod core;
mod memory;
mod output;
mod parser;
mod source2;
mod ui;

fn main() -> Result<()> {
    ui::run_ui().map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}
