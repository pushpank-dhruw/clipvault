//! Best-effort freedesktop app-icon resolution.
//!
//! The clipboard `source` is a window class (e.g. `chromium`,
//! `dev.warp.Warp`). We look for a matching PNG in the usual icon-theme
//! locations so the shelf can show the app's logo on each card. SVG-only
//! themes and unresolved classes fall back to a lettered chip in the GUI.

use std::path::PathBuf;

const SIZES: &[&str] = &[
    "48x48", "64x64", "32x32", "96x96", "128x128", "256x256", "24x24",
];

/// Resolve a PNG icon path for a window class, or `None` if not found.
pub fn resolve(class: &str) -> Option<PathBuf> {
    let last = class.rsplit('.').next().unwrap_or(class);
    // Try the full class, its last dotted component, and lowercased variants.
    let mut names: Vec<String> = Vec::new();
    for n in [class, last] {
        names.push(n.to_string());
        names.push(n.to_lowercase());
    }
    names.dedup();

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/icons/hicolor"));
        roots.push(PathBuf::from(&home).join(".icons/hicolor"));
    }
    roots.push(PathBuf::from("/usr/share/icons/hicolor"));

    for name in &names {
        for root in &roots {
            for size in SIZES {
                let p = root.join(size).join("apps").join(format!("{name}.png"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        let pixmap = PathBuf::from("/usr/share/pixmaps").join(format!("{name}.png"));
        if pixmap.is_file() {
            return Some(pixmap);
        }
    }
    None
}

/// Short human label for a window class, used for the lettered fallback chip
/// and the source line (e.g. `dev.warp.Warp` -> `Warp`).
pub fn short_name(class: &str) -> &str {
    class.rsplit(['.', '/']).next().unwrap_or(class)
}
