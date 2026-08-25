use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDialogRequest {
    pub title: Option<String>,
    pub initial_directory: Option<PathBuf>,
    pub filters: Vec<FileFilter>,
    pub select_directories: bool,
    pub multiple: bool,
}

/// Blocks on the OS picker. An empty result means the user cancelled, which is
/// not an error.
pub fn pick(request: FileDialogRequest) -> Vec<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = request.title {
        dialog = dialog.set_title(title);
    }
    if let Some(initial_directory) = request.initial_directory {
        dialog = dialog.set_directory(initial_directory);
    }
    for filter in request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(filter.name, &extensions);
    }
    match (request.select_directories, request.multiple) {
        (true, true) => dialog.pick_folders().unwrap_or_default(),
        (true, false) => dialog.pick_folder().into_iter().collect(),
        (false, true) => dialog.pick_files().unwrap_or_default(),
        (false, false) => dialog.pick_file().into_iter().collect(),
    }
}
