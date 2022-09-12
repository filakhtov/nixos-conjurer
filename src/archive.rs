pub fn extract<P: AsRef<std::path::Path>>(path: P) -> std::io::Result<()> {
    let file = std::fs::File::open(&path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    let dst = path.as_ref().parent().unwrap();
    archive.unpack(dst)
}
