use rayon::prelude::*;
use std::env;
use std::fs;
use std::io;
use std::iter::Sum;
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::process;

/// Apparent size in bytes, plus the number of entries that could not be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Usage {
    bytes: u64,
    errors: u64,
}

impl Usage {
    fn bytes(bytes: u64) -> Self {
        Usage { bytes, errors: 0 }
    }
}

impl Add for Usage {
    type Output = Usage;

    fn add(self, other: Usage) -> Usage {
        Usage {
            bytes: self.bytes + other.bytes,
            errors: self.errors + other.errors,
        }
    }
}

impl Sum for Usage {
    fn sum<I: Iterator<Item = Usage>>(iter: I) -> Usage {
        iter.fold(Usage::default(), Add::add)
    }
}

fn main() {
    let root = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    let (directories, total) = match scan(&root) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("dirsize: {}: {}", root.display(), err);
            process::exit(1);
        }
    };

    for (path, usage) in &directories {
        println!("{:>10}  {}", human_size(usage.bytes), path.display());
    }
    println!("{:>10}  total", human_size(total.bytes));

    if total.errors > 0 {
        process::exit(1);
    }
}

/// Returns the usage of each directory directly under `root`, largest first,
/// and the total usage of `root` including any files directly under it.
///
/// `root` itself is followed if it is a symlink; nothing beneath it is.
fn scan(root: &Path) -> io::Result<(Vec<(PathBuf, Usage)>, Usage)> {
    let metadata = fs::metadata(root)?;
    let own = Usage::bytes(metadata.len());
    if !metadata.is_dir() {
        return Ok((Vec::new(), own));
    }

    let entries: Vec<_> = fs::read_dir(root)?.collect();
    let results: Vec<_> = entries
        .into_par_iter()
        .map(|entry| entry_usage(root, entry))
        .collect();

    let total = own + results.iter().map(|(_, usage)| *usage).sum::<Usage>();
    let mut directories: Vec<_> = results
        .into_iter()
        .filter_map(|(path, usage)| path.map(|path| (path, usage)))
        .collect();
    directories.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then_with(|| a.0.cmp(&b.0)));

    Ok((directories, total))
}

/// Usage of a single directory entry, along with its path if it is a directory.
fn entry_usage(parent: &Path, entry: io::Result<fs::DirEntry>) -> (Option<PathBuf>, Usage) {
    let entry = match entry {
        Ok(entry) => entry,
        Err(err) => return (None, report(parent, &err)),
    };
    let path = entry.path();

    // `DirEntry::metadata` does not follow symlinks, so links are counted by
    // their own size and never recursed into.
    match entry.metadata() {
        Ok(metadata) => {
            let usage = usage_of(&path, &metadata);
            let directory = if metadata.is_dir() { Some(path) } else { None };
            (directory, usage)
        }
        Err(err) => (None, report(&path, &err)),
    }
}

/// Usage of `path`, including everything beneath it if it is a directory.
fn usage_of(path: &Path, metadata: &fs::Metadata) -> Usage {
    let own = Usage::bytes(metadata.len());
    if !metadata.is_dir() {
        return own;
    }

    match fs::read_dir(path) {
        Ok(entries) => {
            let entries: Vec<_> = entries.collect();
            own + entries
                .into_par_iter()
                .map(|entry| entry_usage(path, entry).1)
                .sum::<Usage>()
        }
        Err(err) => own + report(path, &err),
    }
}

fn report(path: &Path, err: &io::Error) -> Usage {
    eprintln!("dirsize: {}: {}", path.display(), err);
    Usage {
        bytes: 0,
        errors: 1,
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that is removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = env::temp_dir().join(format!("dirsize-{}-{}", name, process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(path: &Path, len: usize) {
        fs::write(path, vec![0u8; len]).unwrap();
    }

    fn dir_len(path: &Path) -> u64 {
        fs::symlink_metadata(path).unwrap().len()
    }

    #[test]
    fn counts_files_in_nested_directories() {
        let tmp = TempDir::new("nested");
        let root = &tmp.0;
        let a = root.join("a");
        let b = a.join("b");
        let c = root.join("c");
        fs::create_dir_all(&b).unwrap();
        fs::create_dir(&c).unwrap();
        write_file(&a.join("one"), 1000);
        write_file(&b.join("two"), 500);
        write_file(&root.join("top"), 200);

        let (directories, total) = scan(root).unwrap();

        let a_bytes = dir_len(&a) + dir_len(&b) + 1500;
        let c_bytes = dir_len(&c);
        assert_eq!(
            directories,
            vec![(a, Usage::bytes(a_bytes)), (c, Usage::bytes(c_bytes))]
        );
        assert_eq!(total, Usage::bytes(dir_len(root) + a_bytes + c_bytes + 200));
    }

    #[test]
    fn file_root_reports_its_own_size() {
        let tmp = TempDir::new("file-root");
        let file = tmp.0.join("file");
        write_file(&file, 42);

        assert_eq!(scan(&file).unwrap(), (Vec::new(), Usage::bytes(42)));
    }

    #[test]
    fn missing_root_is_an_error() {
        let tmp = TempDir::new("missing");
        assert!(scan(&tmp.0.join("nope")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let tmp = TempDir::new("symlink");
        let root = &tmp.0;
        let a = root.join("a");
        fs::create_dir(&a).unwrap();
        write_file(&root.join("big"), 10_000);
        // A link back to the root would recurse forever if followed.
        let link = a.join("loop");
        std::os::unix::fs::symlink(root, &link).unwrap();

        let (directories, _) = scan(root).unwrap();

        assert_eq!(
            directories,
            vec![(a.clone(), Usage::bytes(dir_len(&a) + dir_len(&link)))]
        );
    }

    #[test]
    fn formats_human_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(10 * 1024 * 1024), "10.0 MiB");
        assert_eq!(human_size(u64::MAX), "16384.0 PiB");
    }
}
