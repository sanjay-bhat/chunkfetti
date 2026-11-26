//! chunkfetti: turn a block-game world into streaming 3D tiles you can fly through in any browser.
//!
//! `doctor` finds the game installs whose textures the builder reads; `build` turns a world into a 3D Tiles
//! tileset with a level of detail for every zoom, from the whole world down to single blocks.

mod assets;
mod build;
mod mesh;
mod nbt;
mod report;
mod voxel;
mod world;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Release version: CI sets CHUNKFETTI_VERSION from the git tag; local builds say "dev".
const VERSION: &str = match option_env!("CHUNKFETTI_VERSION") {
    Some(v) => v,
    None => "dev",
};

const HELP: &str = "\
chunkfetti - your world, in any browser

USAGE:
    chunkfetti <COMMAND> [OPTIONS]

COMMANDS:
    doctor                  Find game installs and versions on this computer (textures are read from here,
                            never shipped with chunkfetti)
    build <WORLD> -o <DIR>  Turn a saved world into streaming tiles (ends with a size report)
    size <DIR>              Size report for tiles already built: per level, compressed, and against host limits
    help                    Show this message
    version                 Show the version

OPTIONS for build:
    -o, --out <DIR>         Where the tiles go
    --game <JAR>            The game's client jar to take block colours from (default: the newest one doctor finds)
    --mods <DIR>            A mods folder: modded blocks take their colours from these jars
    --pack <ZIP|DIR>        A resource pack to apply on top (repeatable; later packs win)
    --threads <N>           Worker threads (default: a quarter of the CPU cores, at most 4, to keep laptops cool)
    --area <X0,Z0,X1,Z1>    Only this part of the world (block coordinates)
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("help") | Some("-h") | Some("--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("version") | Some("-V") | Some("--version") => {
            println!("chunkfetti {VERSION}");
            ExitCode::SUCCESS
        }
        Some("doctor") => doctor(),
        Some("build") => build(&args[1..]),
        Some("size") => match args.get(1) {
            Some(d) => match report::run(Path::new(d)) {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("no tiles to report on in {d}: {e}");
                    ExitCode::from(1)
                }
            },
            None => {
                eprintln!("usage: chunkfetti size <DIR>");
                ExitCode::from(2)
            }
        },
        Some(other) => {
            eprintln!("unknown command: {other}\n");
            eprint!("{HELP}");
            ExitCode::from(2)
        }
    }
}

/// Default game folders for each operating system.
fn game_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        out.push(home.join("Library/Application Support/minecraft")); // macOS
        out.push(home.join(".minecraft")); // Linux
    }
    if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
        out.push(appdata.join(".minecraft")); // Windows
    }
    out
}

/// Installed game versions: folders under versions/ that hold a client jar of the same name.
fn versions(game: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    if let Ok(rd) = std::fs::read_dir(game.join("versions")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let jar = e.path().join(format!("{name}.jar"));
            if jar.is_file() {
                found.push((name, jar));
            }
        }
    }
    found.sort();
    found
}

fn doctor() -> ExitCode {
    println!("chunkfetti {VERSION} doctor\n");
    let mut any = false;
    for dir in game_dirs() {
        if !dir.is_dir() {
            continue;
        }
        any = true;
        println!("game folder: {}", dir.display());
        let vs = versions(&dir);
        if vs.is_empty() {
            println!("  no client jars yet (launch the game once to download one)");
        }
        for (name, jar) in vs {
            println!("  {name}\n      {}", jar.display());
        }
        let saves = dir.join("saves");
        match std::fs::read_dir(&saves) {
            Ok(rd) => {
                let n = rd.flatten().filter(|e| e.path().join("level.dat").is_file()).count();
                println!("  {n} saved world(s) in {}", saves.display());
            }
            Err(_) => println!("  no saves folder here: launcher profiles with their own game folder keep worlds there; pass that world folder to build"),
        }
    }
    if !any {
        println!("no game folder found in the usual places; pass --game <JAR> to build");
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    println!("\nCPU cores: {cores} (build uses {} by default; --threads to change)", default_threads(cores));
    ExitCode::SUCCESS
}

/// Gentle default: a quarter of the cores, at least 1 and at most 4, so a build never cooks a laptop.
fn default_threads(cores: usize) -> usize {
    (cores / 4).clamp(1, 4)
}

fn build(args: &[String]) -> ExitCode {
    let mut world: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut game: Option<PathBuf> = None;
    let mut mods: Option<PathBuf> = None;
    let mut packs: Vec<PathBuf> = Vec::new();
    let mut threads: Option<usize> = None;
    let mut area: Option<[i32; 4]> = None;
    let mut i = 0;
    while i < args.len() {
        let val = args.get(i + 1).cloned();
        let mut takes = true;
        match args[i].as_str() {
            "-o" | "--out" => out = val.map(PathBuf::from),
            "--game" => game = val.map(PathBuf::from),
            "--mods" => mods = val.map(PathBuf::from),
            "--pack" => packs.extend(val.map(PathBuf::from)),
            "--threads" => threads = val.and_then(|v| v.parse().ok()),
            "--area" => {
                let v: Vec<i32> = val.unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
                if v.len() != 4 {
                    eprintln!("--area takes four numbers: x0,z0,x1,z1");
                    return ExitCode::from(2);
                }
                area = Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]);
            }
            a if !a.starts_with('-') && world.is_none() => {
                world = Some(PathBuf::from(a));
                takes = false;
            }
            a => {
                eprintln!("unknown option: {a}");
                return ExitCode::from(2);
            }
        }
        i += if takes { 2 } else { 1 };
    }
    let (Some(world), Some(out)) = (world, out) else {
        eprintln!("usage: chunkfetti build <WORLD> -o <DIR>");
        return ExitCode::from(2);
    };
    if !world.join("level.dat").is_file() {
        eprintln!("{} does not look like a saved world (no level.dat)", world.display());
        return ExitCode::from(1);
    }
    // block colours come from the user's own game, mods and packs, in that order (later ones win)
    let mut a = assets::Assets::default();
    let game = game.or_else(newest_game_jar);
    match &game {
        Some(j) => match a.add(j) {
            Ok(n) => println!("game: {} ({n} asset files)", j.display()),
            Err(e) => eprintln!("could not read the game jar: {e}"),
        },
        None => println!("no game jar found: blocks will be plain grey (pass --game <JAR>)"),
    }
    if let Some(dir) = &mods {
        let mut jars: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jar")).collect())
            .unwrap_or_default();
        jars.sort();
        for j in jars {
            if let Ok(n) = a.add(&j) {
                if n > 0 {
                    println!("mod: {} ({n} asset files)", j.file_name().unwrap_or_default().to_string_lossy());
                }
            }
        }
    }
    for p in &packs {
        match a.add(p) {
            Ok(n) => println!("pack: {} ({n} asset files)", p.display()),
            Err(e) => eprintln!("could not read pack: {e}"),
        }
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let o = build::Options { world, out, threads: threads.unwrap_or_else(|| default_threads(cores)).max(1), area };
    match build::run(&o, &a) {
        Ok(s) => {
            println!("tileset: {} ({} levels, {} tiles, {:.1} MB)", o.out.join("tileset.json").display(), s.levels, s.tiles, s.bytes as f64 / 1e6);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("build failed: {e}");
            ExitCode::from(1)
        }
    }
}

/// The most recently changed vanilla client jar (no mod loader in its name).
fn newest_game_jar() -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for dir in game_dirs() {
        for (name, jar) in versions(&dir) {
            let l = name.to_lowercase();
            if ["fabric", "forge", "quilt", "optifine", "neoforge"].iter().any(|k| l.contains(k)) {
                continue;
            }
            let t = std::fs::metadata(&jar).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                best = Some((t, jar));
            }
        }
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn default_threads_stay_gentle() {
        assert_eq!(default_threads(1), 1);
        assert_eq!(default_threads(8), 2);
        assert_eq!(default_threads(16), 4);
        assert_eq!(default_threads(64), 4);
    }

    #[test]
    fn missing_game_folder_has_no_versions() {
        assert!(versions(Path::new("/definitely/not/here")).is_empty());
    }
}
