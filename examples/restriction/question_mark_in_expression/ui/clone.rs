const DYNLINT_URL: &str = "https://github.com/khulnasoft-lab/dynlint";

fn main() {
    clone().unwrap();
}

#[derive(Debug)]
struct Error;

impl From<git2::Error> for Error {
    fn from(_: git2::Error) -> Self {
        Self
    }
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self
    }
}

fn clone() -> Result<(), Error> {
    let _ = git2::Repository::clone(DYNLINT_URL, tempfile::tempdir()?.path())?;
    Ok(())
}
