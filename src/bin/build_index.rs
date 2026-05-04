use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use flate2::read::GzDecoder;
use serde::Deserialize;

const DIMS: usize = 14;
const MAGIC: &[u8; 8] = b"RINHAI01";

#[derive(Debug, Deserialize)]
struct ReferenceJson {
    vector: [f32; DIMS],
    label: String,
}

fn quantize(value: f32) -> u16 {
    if value < 0.0 {
        return 0;
    }

    let clamped = if value > 1.0 { 1.0 } else { value };

    // reservamos 0 para o caso especial -1
    // então 0.0 vira 1 e 1.0 vira 65535
    1 + (clamped * 65534.0).round() as u16
}

fn main() -> anyhow::Result<()> {
    let input_path = Path::new("data/references.json.gz");
    let output_path = Path::new("data/index.bin");

    println!("Reading {:?}", input_path);

    let file = File::open(input_path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let references: Vec<ReferenceJson> = serde_json::from_reader(reader)?;

    println!("Loaded {} references", references.len());

    let mut writer = BufWriter::new(File::create(output_path)?);

    writer.write_all(MAGIC)?;
    writer.write_all(&(references.len() as u32).to_le_bytes())?;
    writer.write_all(&(DIMS as u8).to_le_bytes())?;

    for reference in &references {
        for value in reference.vector {
            let q = quantize(value);
            writer.write_all(&q.to_le_bytes())?;
        }
    }

    for reference in &references {
        let label = match reference.label.as_str() {
            "fraud" => 1_u8,
            _ => 0_u8,
        };

        writer.write_all(&[label])?;
    }

    writer.flush()?;

    let size = std::fs::metadata(output_path)?.len();

    println!("Index written to {:?}", output_path);
    println!("Index size: {:.2} MB", size as f64 / 1024.0 / 1024.0);

    Ok(())
}
