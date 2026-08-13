use std::fs;
use std::path::PathBuf;

use image::imageops::FilterType;

use crate::{repo_root, Result, XtaskError};

pub fn run(source: Option<&str>) -> Result {
    let root = repo_root()?;
    let source = source
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("apps/desktop/assets/icons/icon.png"));
    let output = root.join("apps/desktop/assets/icons");
    fs::create_dir_all(&output)
        .map_err(|error| XtaskError::io("icon_directory_failed", "create icon directory", error))?;
    let image = image::open(&source)
        .map_err(|error| {
            XtaskError::failure(
                "icon_source_invalid",
                format!("{}: {error}", source.display()),
            )
        })?
        .into_rgba8();
    if image.width() != image.height() {
        return Err(XtaskError::failure(
            "icon_source_not_square",
            "icon source must be square",
        ));
    }
    for size in [32, 128, 256] {
        let resized = image::imageops::resize(&image, size, size, FilterType::Lanczos3);
        let path = output.join(match size {
            32 => "32x32.png",
            128 => "128x128.png",
            _ => "128x128@2x.png",
        });
        resized
            .save_with_format(&path, image::ImageFormat::Png)
            .map_err(|error| {
                XtaskError::failure("icon_write_failed", format!("{}: {error}", path.display()))
            })?;
    }
    image::imageops::resize(&image, 256, 256, FilterType::Lanczos3)
        .save_with_format(output.join("icon.ico"), image::ImageFormat::Ico)
        .map_err(|error| XtaskError::failure("icon_write_failed", error.to_string()))?;
    println!("icons: ok ({})", output.display());
    Ok(())
}
