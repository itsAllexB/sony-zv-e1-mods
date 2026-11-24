#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bits;
mod gui;
mod job;
mod mp4fix;
mod sps;

use std::path::PathBuf;

fn main() {
    // `opengate-fix --cli <paths>` works without a window. Paths given without
    // `--cli` (e.g. files dropped onto the program's icon) are queued in the window.
    let mut args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.first().is_some_and(|a| a.as_os_str() == "--cli") {
        std::process::exit(cli(&args[1..]));
    }
    args.retain(|a| a.exists());
    let options = eframe::NativeOptions { viewport: viewport(), ..Default::default() };
    if let Err(e) = eframe::run_native("Open Gate Fix", options, Box::new(move |cc| Ok(Box::new(gui::App::new(cc, args))))) {
        eprintln!("Could not open the window: {e}");
        std::process::exit(1);
    }
}

pub const ABOUT_NOTE: &str = "Open gate clips are recorded at 3840×2560, but the camera labels them \
3840×2160, so editors show them as 16:9. The fix corrects that label in a copy of the file. Picture, \
sound and the camera's gyro and lens data are copied untouched; nothing is re-encoded.";

/// The window has no visible title bar. On macOS the native traffic lights stay
/// and the content runs underneath them; elsewhere the app draws its own
/// minimise / maximise / close buttons and resize edges (see `gui.rs`).
/// The window can be resized a little, but it is a small utility: no full screen.
const MIN_SIZE: [f32; 2] = [420.0, 560.0];
const MAX_SIZE: [f32; 2] = [560.0, 860.0];

fn viewport() -> eframe::egui::ViewportBuilder {
    let builder = eframe::egui::ViewportBuilder::default()
        .with_title("Open Gate Fix")
        .with_app_id("opengate-fix")
        .with_inner_size([440.0, 620.0])
        .with_min_inner_size(MIN_SIZE)
        .with_max_inner_size(MAX_SIZE)
        .with_maximize_button(false)
        .with_drag_and_drop(true);
    #[cfg(target_os = "macos")]
    let builder = builder.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    #[cfg(not(target_os = "macos"))]
    let builder = {
        let builder = builder.with_decorations(false).with_resizable(true);
        match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
            Ok(icon) => builder.with_icon(std::sync::Arc::new(icon)),
            Err(_) => builder,
        }
    };
    builder
}

/// Fixes files or folders with the saved settings, no window.
fn cli(args: &[PathBuf]) -> i32 {
    let settings = job::Settings::load();
    let mut failed = 0;
    for path in job::collect(args) {
        match job::process(&path, &settings, &|_| {}) {
            Ok(job::Outcome::Fixed { from, to, output, warning, .. }) => {
                println!("fixed  {}  {}x{} -> {}x{}  => {}", path.display(), from.0, from.1, to.0, to.1, output.display());
                if let Some(w) = warning {
                    println!("       note: {w}");
                }
            }
            Ok(job::Outcome::NothingToFix { size }) => {
                println!("ok     {}  already {}x{}", path.display(), size.0, size.1)
            }
            Err(e) => {
                failed += 1;
                println!("error  {}  {e}", path.display());
            }
        }
    }
    failed
}
