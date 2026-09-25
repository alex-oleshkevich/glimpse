use std::io::Write;

pub const GUIDE: &str = include_str!("../prime.md");

pub fn print(out: &mut impl Write) -> anyhow::Result<()> {
    let _ = out.write_all(GUIDE.as_bytes());
    Ok(())
}
