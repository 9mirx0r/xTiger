use std::path::PathBuf;

pub mod ck3;

pub fn bench_mods<'a>() -> impl Iterator<Item = (&'a str, &'a PathBuf)> {
    ck3::bench_mods()
}
