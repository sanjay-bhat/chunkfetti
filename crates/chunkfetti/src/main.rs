//! chunkfetti: turn a block-game world into streaming 3D tiles you can fly through in any browser.
//!
//! This is the very first cut: the command line, `doctor` (finds the game installs whose textures the
//! builder will read) and the shape of `build`. Tiling lands in the next releases.

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
    build <WORLD> -o <DIR>  Turn a saved world into streaming tiles (coming in the next releases)
    help                    Show this message
    version                 Show the version

OPTIONS for build:
    -o, --out <DIR>         Where the tiles go
    --game <JAR>            The game's client jar to take block textures from (default: found by doctor)
    --threads <N>           Worker threads (default: a quarter of the CPU cores, at most 4, to keep laptops cool)
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
    let mut world: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                out = args.get(i + 1).map(String::as_str);
                i += 1;
            }
            "--game" | "--threads" => i += 1,
            a if !a.starts_with('-') && world.is_none() => world = Some(a),
            a => {
                eprintln!("unknown option: {a}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let (Some(world), Some(out)) = (world, out) else {
        eprintln!("usage: chunkfetti build <WORLD> -o <DIR>");
        return ExitCode::from(2);
    };
    if !Path::new(world).join("level.dat").is_file() {
        eprintln!("{world} does not look like a saved world (no level.dat)");
        return ExitCode::from(1);
    }
    println!("chunkfetti {VERSION}: tiling is not built yet. World {world} -> {out} will work in a coming release.");
    ExitCode::from(3)
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
