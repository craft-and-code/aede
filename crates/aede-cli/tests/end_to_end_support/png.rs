//! PNG geometry for image-producing CLI regressions.

pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    let image = aede_core::coverart::render_image(bytes, None)?;
    if image.format != "png" {
        return Err("expected a complete PNG image".into());
    }
    let header = image.bytes.get(16..24).ok_or("missing PNG dimensions")?;
    let (width, height) = header.split_at(4);
    let width = u32::from_be_bytes(width.try_into().map_err(|_| "invalid PNG width")?);
    let height = u32::from_be_bytes(height.try_into().map_err(|_| "invalid PNG height")?);
    Ok((width, height))
}
