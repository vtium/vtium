//! What kind of resource a path is, and the trust rule it passes.

use std::path::{Path, PathBuf};

use crate::apply::{Route, Targets};

/// What kind of resource `path` is, in the words a person uses: a folder
/// or a `.svg` file is a pointer theme, a `.cube` file a color map, a
/// `.vtshader` file a shader, a file in a color scheme folder a color scheme,
/// and anything else a font.
pub fn kind(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let in_palettes = path.parent().is_some_and(|dir| {
        vtium_settings::palette::FOLDERS
            .iter()
            .any(|f| dir == Path::new(f))
    });
    if path.is_dir() || extension == "svg" {
        "pointer theme"
    } else if extension == "cube" {
        "color map"
    } else if extension == "vtshader" {
        "shader"
    } else if in_palettes {
        "color scheme"
    } else {
        "font"
    }
}

/// The user whose files the trust rule accepts for an install to
/// `targets` through `sudo`: root for the system's directories, which
/// root installs, and this process's user for any other, as `apply`
/// runs as that user there.
pub fn rule_user(targets: &Targets, sudo: &std::ffi::OsString) -> u32 {
    let euid = rustix::process::geteuid().as_raw();
    match crate::apply::route(targets, sudo) {
        Route::Sudo(_) => 0,
        Route::User => euid,
    }
}

/// Whether the resource at `path` passes the trust rule for the user
/// `euid`'s files: it and every folder above it belong to root, or to
/// `euid`, and no one else can write them, none is a symbolic link, and
/// its filesystem stays mounted. A pointer theme is a folder, and passes
/// when a picture in its `cursors` folder passes the rule for a theme,
/// whose links may stay inside the theme. `Err` says why, file first.
pub fn trusted(path: &Path, euid: u32) -> Result<(), String> {
    use vtium_settings::cullet::{Rule, check, check_in_theme};
    let rule = Rule::running_as(euid);
    let mounts = vtium_settings::mounts::Mounts::system().map_err(|e| {
        format!(
            "{}: vtium-config cannot check this file, because it cannot read the mount table: {e}.",
            path.display()
        )
    })?;
    let is_folder = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    if !is_folder {
        return check(path, &rule, &mounts).map_err(|refused| refused.to_string());
    }
    let cursors = path.join("cursors");
    let mut pictures: Vec<PathBuf> = std::fs::read_dir(&cursors)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    pictures.sort();
    let Some(picture) = pictures.first() else {
        return Err(format!(
            "{}: this folder holds no cursors folder with pictures in it, so it is not a pointer theme.",
            path.display()
        ));
    };
    check_in_theme(path, picture, &rule, &mounts).map_err(|refused| refused.to_string())
}
