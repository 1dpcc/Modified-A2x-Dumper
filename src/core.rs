use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

use anyhow::Result;
use log::info;
use memflow::prelude::v1::*;

use crate::analysis;
use crate::output::Output;

#[derive(Clone, Debug)]
pub struct DumpConfig {
    pub connector: Option<String>,
    pub connector_args: Option<String>,
    pub file_types: Vec<String>,
    pub indent_size: usize,
    pub output: PathBuf,
    pub process_name: String,
}

impl Default for DumpConfig {
    fn default() -> Self {
        Self {
            connector: None,
            connector_args: None,
            file_types: vec![
                "cs".to_string(),
                "hpp".to_string(),
                "json".to_string(),
                "rs".to_string(),
                "zig".to_string(),
            ],
            indent_size: 4,
            output: PathBuf::from("output"),
            process_name: "cs2.exe".to_string(),
        }
    }
}

pub fn execute_dump(config: DumpConfig) -> Result<()> {
    let conn_args = config
        .connector_args
        .map(|s| ConnectorArgs::from_str(&s).expect("unable to parse connector arguments"))
        .unwrap_or_default();

    let mut os = match config.connector {
        Some(conn) => {
            let mut inventory = Inventory::scan();

            inventory
                .builder()
                .connector(&conn)
                .args(conn_args)
                .os("win32")
                .build()?
        }
        None => {
            #[cfg(windows)]
            {
                memflow_native::create_os(&OsArgs::default(), LibArc::default())?
            }
            #[cfg(not(windows))]
            {
                panic!("no connector specified")
            }
        }
    };

    let mut process = os.process_by_name(&config.process_name)?;

    let now = Instant::now();

    let result = analysis::analyze_all(&mut process)?;
    let output = Output::new(&config.file_types, config.indent_size, &config.output, &result)?;

    output.dump_all(&mut process)?;

    info!("analysis completed in {:.2?}", now.elapsed());

    Ok(())
}
