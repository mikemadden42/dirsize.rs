use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn main() {
    let directory = ".";

    let paths = fs::read_dir(directory)
        .unwrap()
        .map(|res| res.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let directories = paths
        .into_par_iter()
        .filter_map(|path| if path.is_dir() { Some(path) } else { None })
        .collect::<Vec<_>>();

    let directory_sizes: Vec<_> = directories
        .par_iter()
        .map(|path| {
            let size = calculate_directory_size(path);
            (path.to_str().unwrap().to_owned(), size)
        })
        .collect();

    for (directory, size) in directory_sizes {
        println!("{directory}: {size} bytes");
    }
}

fn calculate_directory_size(path: &Path) -> u64 {
    let metadata = fs::metadata(path).unwrap();
    let mut size = metadata.len();

    if metadata.is_dir() {
        let paths = fs::read_dir(path)
            .unwrap()
            .map(|res| res.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let arc_paths = Arc::new(paths);
        let subdirectory_sizes: Vec<_> = arc_paths
            .par_iter()
            .filter(|sub_path| sub_path.is_dir())
            .map(|sub_path| calculate_directory_size(sub_path))
            .collect();

        size += subdirectory_sizes.into_iter().sum::<u64>();
    }

    size
}
