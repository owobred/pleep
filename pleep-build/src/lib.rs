use std::path::PathBuf;

use tracing::{instrument, warn};

#[instrument(level = "trace", err(level = "debug"))]
pub fn get_files_in_directory(directory: &PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
    get_files_recursive(directory, directory)
}

#[instrument(skip(base), err(level = "debug"), level = "trace")]
fn get_files_recursive(
    directory: &PathBuf,
    base: &PathBuf,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut paths = Vec::new();

    for file in std::fs::read_dir(directory)? {
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                warn!(?error, "skipping file");
                continue;
            }
        };

        let file_path = file.path();
        let file_type = file.file_type()?;

        if file_type.is_dir() {
            let mut sub_files = get_files_recursive(&directory.join(file.file_name()), base)?;
            paths.append(&mut sub_files);
        } else if file_type.is_file() {
            paths.push(file_path);
        }
    }

    Ok(paths)
}

pub fn make_log(values: &[f32], new_size: usize) -> Vec<f32> {
    let last_point_ln = (values.len() as f32).ln();
    let mut new = vec![0.0; new_size];

    for index in 0..new_size {
        let point = (index as f32).ln() / last_point_ln * values.len() as f32;
        new[index] = values[point as usize];
    }

    new
}
