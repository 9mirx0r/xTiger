//! The xTiger MCP server: lets AI assistants validate Crusader Kings III mods with ck3-tiger, query
//! the results, and play-test mods in the real game.
//!
//! The xTiger app uses [`locate`] and [`mods`] too, so both always find the same things.

pub mod docs;
pub mod game;
pub mod journal;
pub mod locate;
pub mod mods;
pub mod playsets;
pub mod requests;
pub mod runs;
pub mod server;
pub mod sessions;
pub mod tools;
pub mod vanilla;

#[cfg(test)]
pub mod testing {
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// A folder that is removed when the test ends.
    #[derive(Debug)]
    pub struct TempDir(PathBuf);

    impl TempDir {
        /// # Panics
        /// If the folder cannot be created.
        pub fn new() -> Self {
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let n = COUNT.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir()
                .join(format!("xtiger-mcp-test-{}-{n}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            // The temp folder can be written with Windows' short names (`RUNNER~1`), which the
            // code under test spells out in full.
            Self(crate::mods::clean(&dir))
        }
    }

    impl Default for TempDir {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Deref for TempDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
