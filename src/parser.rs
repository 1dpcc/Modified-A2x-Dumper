use std::collections::HashMap;
use std::fs;
use std::path::Path;
use regex::Regex;

type OffsetMap = HashMap<String, u64>;

pub fn parse_hpp_file(path: &Path) -> Result<OffsetMap, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    let regex = Regex::new(r"constexpr\s+std::ptrdiff_t\s+(\w+)\s*=\s*(0x[0-9a-fA-F]+)")
        .map_err(|e| format!("Regex error: {}", e))?;
    
    let mut offsets = OffsetMap::new();
    
    for cap in regex.captures_iter(&content) {
        if let (Some(name), Some(value_str)) = (cap.get(1), cap.get(2)) {
            let offset_name = name.as_str().to_string();
            if let Ok(value) = u64::from_str_radix(&value_str.as_str()[2..], 16) {
                offsets.insert(offset_name, value);
            }
        }
    }
    
    Ok(offsets)
}

pub fn scan_output_directory(output_dir: &Path) -> Result<OffsetMap, String> {
    let mut combined_offsets = OffsetMap::new();
    
    if !output_dir.exists() {
        return Err(format!("Output directory does not exist: {:?}", output_dir));
    }
    
    let entries = fs::read_dir(output_dir)
        .map_err(|e| format!("Failed to read output directory: {}", e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();
        
        if path.extension().map_or(false, |ext| ext == "hpp") {
            match parse_hpp_file(&path) {
                Ok(offsets) => {
                    combined_offsets.extend(offsets);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse {:?}: {}", path, e);
                }
            }
        }
    }
    
    Ok(combined_offsets)
}

pub fn update_offsets_in_file(
    user_file: &Path,
    dump_offsets: &OffsetMap,
) -> Result<(String, UpdateStats), String> {
    let content = fs::read_to_string(user_file)
        .map_err(|e| format!("Failed to read user file: {}", e))?;
    
    let mut updated_content = content.clone();
    let mut stats = UpdateStats::default();
    
    let regex = Regex::new(r"constexpr\s+std::ptrdiff_t\s+(\w+)\s*=\s*(0x[0-9a-fA-F]+)")
        .map_err(|e| format!("Regex error: {}", e))?;
    
    for cap in regex.captures_iter(&content) {
        if let (Some(name_match), Some(old_value)) = (cap.get(1), cap.get(2)) {
            let offset_name = name_match.as_str();
            
            if let Some(&new_value) = dump_offsets.get(offset_name) {
                let old_value_str = old_value.as_str();
                let new_value_str = format!("{:#X}", new_value);
                
                updated_content = updated_content.replacen(
                    &format!("{} = {}", offset_name, old_value_str),
                    &format!("{} = {}", offset_name, new_value_str),
                    1,
                );
                
                stats.updated += 1;
            } else {
                stats.missing += 1;
            }
        }
    }

    let banner_regex = Regex::new(r"(?m)^// Updated using.*\n(// .*\n)*\n?")
        .map_err(|e| format!("Regex error: {}", e))?;
    let stripped = banner_regex.replace(&updated_content, "").to_string();

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let banner = format!(
        "// Updated using Modified A2X Dumper — {}\n// https://github.com/1dpcc/Modified-A2x-Dumper\n\n",
        now
    );
    let final_content = format!("{}{}", banner, stripped);

    fs::write(user_file, &final_content)
        .map_err(|e| format!("Failed to write updated file: {}", e))?;
    
    stats.total = stats.updated + stats.missing;
    
    let summary = format!(
        "Update completed:\n✓ Updated: {}/{} offsets\n✗ Missing: {} offsets",
        stats.updated, stats.total, stats.missing
    );
    
    Ok((summary, stats))
}

#[derive(Default, Debug)]
pub struct UpdateStats {
    pub total: usize,
    pub updated: usize,
    pub missing: usize,
}
