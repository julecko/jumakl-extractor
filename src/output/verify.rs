//! Post-write sanity check on an output CSV, ported from massExtraction's
//! security/filecheck.cpp: a corrupt or oversized file is worse than no file
//! at all, since a downstream consumer ingests it blindly. Checked once per
//! mode, after that mode's writer has closed.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

const MAX_FILE_SIZE_MB: u64 = 20;

/// Checks the file's size and that every non-blank line has the same number
/// of `DELIMITER`-separated columns. On failure, renames the file to
/// `<path>.error` so a broken file is never left where a downstream consumer
/// expects a good one, and returns why it failed.
pub fn verify(path: &Path) -> Result<(), String> {
    match check(path) {
        Ok(()) => Ok(()),
        Err(reason) => {
            if let Err(err) = move_aside(path) {
                tracing::warn!("failed to move aside invalid output file {path:?}: {err:#}");
            }
            Err(reason)
        }
    }
}

fn check(path: &Path) -> Result<(), String> {
    let size = fs::metadata(path)
        .map_err(|err| format!("can't stat {path:?}: {err}"))?
        .len();
    if size > MAX_FILE_SIZE_MB * 1024 * 1024 {
        return Err(format!(
            "{path:?} is {size} bytes, over the {MAX_FILE_SIZE_MB}MB limit"
        ));
    }

    let file = File::open(path).map_err(|err| format!("can't open {path:?}: {err}"))?;
    let mut expected_columns = None;
    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|err| format!("can't read {path:?}: {err}"))?;
        if line.is_empty() {
            continue;
        }
        let columns = line.matches(super::csv::DELIMITER).count() + 1;
        match expected_columns {
            None => expected_columns = Some(columns),
            Some(expected) if columns != expected => {
                return Err(format!(
                    "{path:?} line {}: expected {expected} column(s), found {columns}",
                    line_number + 1
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

fn move_aside(path: &Path) -> std::io::Result<()> {
    let mut error_path = path.as_os_str().to_owned();
    error_path.push(".error");
    fs::rename(path, error_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn accepts_a_consistent_csv() {
        let path = write_temp(
            "verify_ok.csv",
            "SOURCE;SKU;STOCK;DATE\nAM;AM - 1;5;1.1.2026\n",
        );
        assert!(verify(&path).is_ok());
        assert!(path.exists());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn rejects_and_moves_aside_a_ragged_csv() {
        let path = write_temp("verify_ragged.csv", "SOURCE;SKU;STOCK;DATE\nAM;AM - 1;5\n");
        assert!(verify(&path).is_err());
        assert!(!path.exists());
        let error_path = {
            let mut p = path.as_os_str().to_owned();
            p.push(".error");
            std::path::PathBuf::from(p)
        };
        assert!(error_path.exists());
        let _ = fs::remove_file(&error_path);
    }

    #[test]
    fn missing_file_is_an_error() {
        let path = std::env::temp_dir().join("verify_missing_does_not_exist.csv");
        let _ = fs::remove_file(&path);
        assert!(verify(&path).is_err());
    }
}
