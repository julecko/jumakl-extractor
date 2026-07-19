use std::path::PathBuf;

fn app_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::new()
    } else {
        std::env::current_exe()
            .expect("failed to get executable path")
            .parent()
            .expect("binary has no parent directory")
            .to_path_buf()
    }
}

pub fn config_folder_path() -> PathBuf {
    app_dir().join("config")
}
