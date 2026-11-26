//! Size report for a built tileset: how big it is per level, how well it compresses, and whether it fits the
//! limits of common free static hosts. Printed at the end of every build and saved as <out>/size-report.json.

use flate2::{write::GzEncoder, Compression};
use std::io::Write;
use std::path::Path;

/// Free-tier limits of common static hosts (check the host's own docs; these change).
const HOSTS: [(&str, Option<u64>, Option<u64>, Option<u64>); 2] = [
    // name, max files, max bytes per file, max total bytes
    ("Cloudflare Pages", Some(20_000), Some(25 * 1024 * 1024), None),
    ("GitHub Pages", None, Some(100 * 1024 * 1024), Some(1_000_000_000)),
];

pub struct Level {
    pub level: usize,
    pub files: u64,
    pub bytes: u64,
    pub largest: u64,
}

pub fn run(out: &Path) -> Result<serde_json::Value, String> {
    let mut levels: Vec<Level> = Vec::new();
    let mut sample_raw = 0u64;
    let mut sample_gz = 0u64;
    let tiles = out.join("tiles");
    let mut dirs: Vec<(usize, std::path::PathBuf)> = std::fs::read_dir(&tiles)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().parse::<usize>().ok().map(|l| (l, e.path())))
        .collect();
    dirs.sort();
    for (level, dir) in dirs {
        let mut l = Level { level, files: 0, bytes: 0, largest: 0 };
        let mut files: Vec<(u64, std::path::PathBuf)> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter_map(|e| e.metadata().ok().map(|m| (m.len(), e.path())))
            .collect();
        files.sort();
        for (n, _) in &files {
            l.files += 1;
            l.bytes += n;
            l.largest = l.largest.max(*n);
        }
        // compression: a spread of up to 24 files per level, gzip as a stand-in for what a host serves
        let step = (files.len() / 24).max(1);
        for (n, p) in files.iter().step_by(step).take(24) {
            if let Ok(b) = std::fs::read(p) {
                let mut z = GzEncoder::new(Vec::new(), Compression::new(6));
                z.write_all(&b).ok();
                sample_gz += z.finish().map(|v| v.len() as u64).unwrap_or(*n);
                sample_raw += n;
            }
        }
        levels.push(l);
    }
    let index = std::fs::metadata(out.join("tileset.json")).map(|m| m.len()).unwrap_or(0);
    let files: u64 = levels.iter().map(|l| l.files).sum::<u64>() + 1;
    let bytes: u64 = levels.iter().map(|l| l.bytes).sum::<u64>() + index;
    let largest = levels.iter().map(|l| l.largest).max().unwrap_or(0).max(index);
    let ratio = if sample_raw > 0 { sample_gz as f64 / sample_raw as f64 } else { 1.0 };

    let mb = |b: u64| b as f64 / 1e6;
    println!("\nsize report");
    println!("  level   tiles        MB   largest MB");
    for l in &levels {
        println!("  {:>5} {:>7} {:>9.1} {:>12.2}", l.level, l.files, mb(l.bytes), mb(l.largest));
    }
    println!("  index        1 {:>9.2}", mb(index));
    println!("  total {files:>7} {:>9.1}   (about {:.1} MB compressed in transit)", mb(bytes), mb(bytes) * ratio);
    let mut hosts = Vec::new();
    for (name, max_files, max_file, max_total) in HOSTS {
        let mut problems = Vec::new();
        if let Some(m) = max_files {
            if files > m {
                problems.push(format!("{files} files, limit {m}"));
            }
        }
        if let Some(m) = max_file {
            if largest > m {
                problems.push(format!("largest file {:.1} MB, limit {:.0} MB", mb(largest), mb(m)));
            }
        }
        if let Some(m) = max_total {
            if bytes > m {
                problems.push(format!("{:.0} MB in all, limit {:.0} MB", mb(bytes), mb(m)));
            }
        }
        if problems.is_empty() {
            println!("  fits {name}");
        } else {
            println!("  too big for {name}: {}", problems.join("; "));
        }
        hosts.push(serde_json::json!({"host": name, "fits": problems.is_empty(), "problems": problems}));
    }
    let report = serde_json::json!({
        "files": files,
        "bytes": bytes,
        "largest_file_bytes": largest,
        "compressed_ratio_estimate": (ratio * 1000.0).round() / 1000.0,
        "levels": levels.iter().map(|l| serde_json::json!({
            "level": l.level, "tiles": l.files, "bytes": l.bytes, "largest_bytes": l.largest
        })).collect::<Vec<_>>(),
        "hosts": hosts,
    });
    std::fs::write(out.join("size-report.json"), serde_json::to_vec_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_levels_and_totals() {
        let out = std::env::temp_dir().join(format!("chunkfetti-report-{}", std::process::id()));
        std::fs::create_dir_all(out.join("tiles/0")).unwrap();
        std::fs::create_dir_all(out.join("tiles/1")).unwrap();
        std::fs::write(out.join("tiles/0/0_0.glb"), vec![7u8; 1000]).unwrap();
        std::fs::write(out.join("tiles/0/1_0.glb"), vec![7u8; 3000]).unwrap();
        std::fs::write(out.join("tiles/1/0_0.glb"), vec![7u8; 500]).unwrap();
        std::fs::write(out.join("tileset.json"), b"{}").unwrap();
        let r = super::run(&out).unwrap();
        assert_eq!(r["files"], 4);
        assert_eq!(r["bytes"], 4502);
        assert_eq!(r["largest_file_bytes"], 3000);
        assert_eq!(r["levels"][0]["tiles"], 2);
        assert!(r["compressed_ratio_estimate"].as_f64().unwrap() < 0.5);
        assert!(out.join("size-report.json").is_file());
        std::fs::remove_dir_all(&out).ok();
    }
}
