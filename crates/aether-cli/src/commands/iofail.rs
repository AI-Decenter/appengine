use anyhow::Result;

pub async fn handle() -> Result<()> {
    // Attempt to write into an obviously invalid path to trigger an IO error.
    let path = std::path::Path::new("/proc/this_should_fail/aether");
    std::fs::write(path, b"fail")?; // will error
    Ok(())
}