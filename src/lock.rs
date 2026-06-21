use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use crate::errors::{ErrorCode, GraphDbError};

#[derive(Debug)]
pub struct WriteLockGuard {
    path: PathBuf,
}

pub fn acquire_write_lock(db_file: &Path) -> Result<WriteLockGuard, GraphDbError> {
    let lock_path = db_file.with_extension("write.lock");
    let open_result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path);

    match open_result {
        Ok(_) => Ok(WriteLockGuard { path: lock_path }),
        Err(err) => Err(GraphDbError::new(
            ErrorCode::WriteLockConflict,
            format!("write lock conflict: {err}"),
        )),
    }
}

impl Drop for WriteLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_db() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("agdb-lock-{nanos}.db"))
    }

    #[test]
    fn rejects_second_writer() {
        let db = temp_db();
        let first = acquire_write_lock(&db).expect("first lock succeeds");
        let second = acquire_write_lock(&db).expect_err("second lock fails");
        assert_eq!(second.code, ErrorCode::WriteLockConflict);
        drop(first);
        let third = acquire_write_lock(&db).expect("lock released");
        drop(third);
    }
}
