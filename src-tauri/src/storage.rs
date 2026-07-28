use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "config path has no UTF-8 file name".to_owned())?;
    let temporary_path = parent.join(format!(".{file_name}.tmp"));

    let result = (|| -> Result<(), std::io::Error> {
        let mut temporary = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)?;
        temporary.write_all(contents)?;
        temporary.sync_all()?;
        fs::rename(&temporary_path, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("save {}: {error}", path.display()));
    }
    Ok(())
}
