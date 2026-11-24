//! Turns one dropped file into one fixed file, following the user's settings.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mp4fix;

pub const SUFFIX: &str = "_remuxed";
pub const SUBFOLDER: &str = "remuxed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputMode {
    /// Next to the original, with `_remuxed` added to the name.
    Alongside,
    /// In a `remuxed` folder next to the original, same name.
    Subfolder,
    /// In one folder the user picked, same name.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output: OutputMode,
    pub custom_dir: Option<PathBuf>,
    pub trash_originals: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { output: OutputMode::Alongside, custom_dir: None, trash_originals: false }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "OpenGateFix").map(|d| d.config_dir().join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_vec_pretty(self) {
                let _ = fs::write(p, json);
            }
        }
    }
}

pub type Size = (u32, u32);

#[derive(Debug, Clone)]
pub enum Outcome {
    Fixed { from: Size, to: Size, output: PathBuf, original_trashed: bool, warning: Option<String> },
    /// Header and stream already agree.
    NothingToFix { size: Size },
}

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "mp4" | "mov" | "m4v"))
        .unwrap_or(false)
}

/// Expands dropped paths: files stay as they are, folders are searched for videos.
pub fn collect(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let hidden = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
            if hidden {
                continue;
            }
            if p.is_dir() {
                // Don't re-process our own output folder.
                if depth < 8 && p.file_name().and_then(|n| n.to_str()) != Some(SUBFOLDER) {
                    walk(&p, depth + 1, out);
                }
            } else if is_video(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, 0, &mut out);
        } else {
            out.push(p.clone());
        }
    }
    out
}

fn file_name(p: &Path) -> Result<&str, String> {
    p.file_name().and_then(|n| n.to_str()).ok_or_else(|| "Unusable file name".to_string())
}

fn output_path(src: &Path, s: &Settings) -> Result<PathBuf, String> {
    let dir = src.parent().unwrap_or(Path::new("."));
    let name = file_name(src)?;
    Ok(match s.output {
        OutputMode::Alongside => {
            let stem = src.file_stem().and_then(|n| n.to_str()).ok_or("Unusable file name")?;
            let ext = src.extension().and_then(|n| n.to_str()).unwrap_or("MP4");
            dir.join(format!("{stem}{SUFFIX}.{ext}"))
        }
        OutputMode::Subfolder => dir.join(SUBFOLDER).join(name),
        OutputMode::Custom => s.custom_dir.clone().ok_or("No output folder chosen in settings")?.join(name),
    })
}

/// Sony writes `<clip>M01.XML` next to `<clip>.MP4`.
fn sidecar_of(video: &Path) -> Option<PathBuf> {
    let stem = video.file_stem()?.to_str()?;
    ["M01.XML", "M01.xml"].iter().map(|s| video.with_file_name(format!("{stem}{s}"))).find(|p| p.is_file())
}

fn copy_with_progress(src: &Path, dst: &Path, progress: &dyn Fn(f32)) -> std::io::Result<()> {
    let mut input = File::open(src)?;
    let total = input.metadata()?.len().max(1);
    let mut output = File::create(dst)?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut done = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        done += n as u64;
        progress(done as f32 / total as f32);
    }
    output.sync_all()
}

pub fn process(src: &Path, settings: &Settings, progress: &dyn Fn(f32)) -> Result<Outcome, String> {
    if !src.is_file() {
        return Err("File not found".into());
    }
    let analysis = mp4fix::analyze(src)?;
    if !analysis.needs_fix() {
        return Ok(Outcome::NothingToFix { size: analysis.stream });
    }
    let dst = output_path(src, settings)?;
    if dst.exists() {
        return Err(format!("Already exists: {}", dst.display()));
    }
    if let Some(dir) = dst.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Can't create output folder: {e}"))?;
    }
    let part = dst.with_file_name(format!("{}.part", file_name(&dst)?));

    let result = (|| -> Result<(), String> {
        copy_with_progress(src, &part, progress).map_err(|e| format!("Copy failed: {e}"))?;
        let mut f = OpenOptions::new().read(true).write(true).open(&part).map_err(|e| e.to_string())?;
        mp4fix::apply(&mut f, &analysis.patches)?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        // Check the result before it gets its final name.
        let check = mp4fix::analyze(&part)?;
        if check.needs_fix() || check.stream != analysis.stream {
            return Err("Verification failed".into());
        }
        if let Ok(meta) = fs::metadata(src) {
            if let Ok(mtime) = meta.modified() {
                let _ = filetime::set_file_mtime(&part, filetime::FileTime::from_system_time(mtime));
            }
        }
        fs::rename(&part, &dst).map_err(|e| format!("Can't finish output file: {e}"))
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&part);
        return Err(e);
    }

    // Sidecar XML: write a corrected copy next to the output.
    let mut warning = None;
    let sidecar = sidecar_of(src);
    if let Some(side) = &sidecar {
        let out_stem = dst.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
        let side_dst = dst.with_file_name(format!("{out_stem}M01.XML"));
        if !side_dst.exists() {
            let written = fs::read(side).and_then(|raw| {
                let fixed = mp4fix::fix_xml(&raw, analysis.container, analysis.stream, false).unwrap_or(raw);
                fs::write(&side_dst, fixed)
            });
            if written.is_err() {
                warning = Some("Video fixed, but the sidecar XML could not be written".to_string());
            }
        }
    }

    let mut original_trashed = false;
    if settings.trash_originals {
        match trash::delete(src) {
            Ok(()) => {
                original_trashed = true;
                if let Some(side) = &sidecar {
                    let _ = trash::delete(side);
                }
            }
            Err(e) => warning = Some(format!("Fixed, but the original could not be moved to the trash: {e}")),
        }
    }

    Ok(Outcome::Fixed { from: analysis.container, to: analysis.stream, output: dst, original_trashed, warning })
}
