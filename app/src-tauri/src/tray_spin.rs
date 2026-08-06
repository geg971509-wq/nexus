//! Menu-bar Earth spin while tunnel/proxy is live.
//! Pre-baked frames (icons/tray/frame_XX.png); cycle via tray.set_icon.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Manager};

static SPINNING: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static FRAMES: Mutex<Vec<Image<'static>>> = Mutex::new(Vec::new());

const FRAME_PNGS: &[&[u8]] = &[
    include_bytes!("../icons/tray/frame_00.png"),
    include_bytes!("../icons/tray/frame_01.png"),
    include_bytes!("../icons/tray/frame_02.png"),
    include_bytes!("../icons/tray/frame_03.png"),
    include_bytes!("../icons/tray/frame_04.png"),
    include_bytes!("../icons/tray/frame_05.png"),
    include_bytes!("../icons/tray/frame_06.png"),
    include_bytes!("../icons/tray/frame_07.png"),
    include_bytes!("../icons/tray/frame_08.png"),
    include_bytes!("../icons/tray/frame_09.png"),
    include_bytes!("../icons/tray/frame_10.png"),
    include_bytes!("../icons/tray/frame_11.png"),
];

fn load_frames() -> Result<(), String> {
    let mut g = FRAMES.lock().map_err(|e| e.to_string())?;
    if !g.is_empty() {
        return Ok(());
    }
    for b in FRAME_PNGS {
        let img = Image::from_bytes(b).map_err(|e| format!("tray frame: {e}"))?;
        g.push(img);
    }
    Ok(())
}

fn tray_of(app: &AppHandle) -> Option<TrayIcon<tauri::Wry>> {
    app.try_state::<TrayIcon<tauri::Wry>>()
        .map(|s| s.inner().clone())
}

fn set_frame(app: &AppHandle, idx: usize) {
    let Ok(g) = FRAMES.lock() else { return };
    if g.is_empty() {
        return;
    }
    let i = idx % g.len();
    let Some(tray) = tray_of(app) else { return };
    let _ = tray.set_icon(Some(g[i].clone()));
}

/// Call once from setup after tray is managed.
pub fn init(app: &AppHandle) {
    if let Err(e) = load_frames() {
        eprintln!("tray_spin load: {e}");
        return;
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    thread::Builder::new()
        .name("nexus-tray-spin".into())
        .spawn(move || {
            let mut i = 0usize;
            loop {
                if SPINNING.load(Ordering::Relaxed) {
                    set_frame(&app, i);
                    i = i.wrapping_add(1);
                    thread::sleep(Duration::from_millis(90));
                } else {
                    // idle: keep frame 0 (upright Earth)
                    set_frame(&app, 0);
                    // cheap wait until spin starts
                    while !SPINNING.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(200));
                    }
                    i = 0;
                }
            }
        })
        .ok();
}

pub fn set_spinning(on: bool) {
    SPINNING.store(on, Ordering::Relaxed);
}
