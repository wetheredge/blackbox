use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;

#[test]
fn readme() -> io::Result<()> {
    let readme = File::open("README.md")?;
    let readme = BufReader::new(readme);

    let msrv = readme
        .lines()
        .map(Result::unwrap)
        .find(|line| line.starts_with("[![MSRV]("))
        .expect("MSRV badge");

    let (msrv, _) = msrv.split_once(')').unwrap();
    let (_, msrv) = msrv.rsplit_once('=').unwrap();

    assert_eq!(env!("CARGO_PKG_RUST_VERSION"), msrv);

    Ok(())
}

#[test]
fn toolchain_file() -> io::Result<()> {
    let path = PathBuf::from("../rust-toolchain.toml");

    if env::var("CI").is_ok() && !path.exists() {
        // Assume it was removed in CI to avoid overriding a >MSRV toolchain
        return Ok(());
    }

    let file = File::open(path)?;
    let file = BufReader::new(file);

    let msrv = file
        .lines()
        .map(Result::unwrap)
        .find(|line| line.starts_with("version"))
        .expect("version line");

    let (_, msrv) = msrv.split_once('=').unwrap();
    let msrv = msrv.trim().trim_matches('"');

    assert_eq!(env!("CARGO_PKG_RUST_VERSION"), msrv);

    Ok(())
}
