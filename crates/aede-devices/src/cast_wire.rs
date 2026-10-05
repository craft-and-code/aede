//! Bounded Cast V2 envelope framing; protocol fields follow Chromium CastMessage.

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) const MAX_FRAME: usize = 64 * 1024;

#[derive(Debug)]
pub(super) struct Message {
    pub source: String,
    pub destination: String,
    pub namespace: String,
    pub payload: Value,
}

fn varint(input: &[u8], offset: &mut usize) -> Result<u64, String> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *input.get(*offset).ok_or("truncated Cast varint")?;
        *offset += 1;
        if shift == 63 && byte > 1 {
            return Err("overflowing Cast varint".into());
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err("overflowing Cast varint".into())
}

fn bytes<'a>(input: &'a [u8], offset: &mut usize) -> Result<&'a [u8], String> {
    let length = usize::try_from(varint(input, offset)?).map_err(|_| "Cast field too large")?;
    let end = offset.checked_add(length).ok_or("Cast field too large")?;
    let value = input.get(*offset..end).ok_or("truncated Cast field")?;
    *offset = end;
    Ok(value)
}

fn text(input: &[u8], maximum: usize) -> Result<String, String> {
    if input.is_empty() || input.len() > maximum {
        return Err("invalid Cast text length".into());
    }
    let value = std::str::from_utf8(input).map_err(|_| "invalid Cast UTF-8")?;
    if value.chars().any(char::is_control) {
        return Err("invalid Cast routing text".into());
    }
    Ok(value.into())
}

pub(super) fn decode(input: &[u8]) -> Result<Message, String> {
    if input.is_empty() || input.len() > MAX_FRAME {
        return Err("Cast frame exceeds its limit".into());
    }
    let mut offset = 0;
    let mut seen = 0u8;
    let mut source = None;
    let mut destination = None;
    let mut namespace = None;
    let mut payload = None;
    while offset < input.len() {
        let tag = varint(input, &mut offset)?;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == 0 || field > 0x1fff_ffff {
            return Err("invalid Cast field number".into());
        }
        if field <= 7 {
            let bit = 1 << (field - 1);
            if seen & bit != 0 {
                return Err("duplicate Cast field".into());
            }
            seen |= bit;
        }
        match (field, wire) {
            (1 | 5, 0) => {
                if varint(input, &mut offset)? != 0 {
                    return Err("unsupported Cast version or binary payload".into());
                }
            }
            (2, 2) => source = Some(text(bytes(input, &mut offset)?, 128)?),
            (3, 2) => destination = Some(text(bytes(input, &mut offset)?, 128)?),
            (4, 2) => namespace = Some(text(bytes(input, &mut offset)?, 256)?),
            (6, 2) => {
                let value = serde_json::from_slice::<Value>(bytes(input, &mut offset)?)
                    .map_err(|_| "invalid Cast JSON payload")?;
                if !value.is_object() {
                    return Err("Cast payload must be an object".into());
                }
                payload = Some(value);
            }
            (1..=7, _) => return Err("invalid Cast field type".into()),
            (_, 0) => {
                varint(input, &mut offset)?;
            }
            (_, 2) => {
                bytes(input, &mut offset)?;
            }
            (_, 1 | 5) => {
                let length = if wire == 1 { 8 } else { 4 };
                let end = offset.checked_add(length).ok_or("Cast field too large")?;
                input.get(offset..end).ok_or("truncated Cast field")?;
                offset = end;
            }
            _ => return Err("unsupported Cast wire type".into()),
        }
    }
    if seen & 63 != 63 {
        return Err("missing required Cast field".into());
    }
    Ok(Message {
        source: source.ok_or("missing Cast source")?,
        destination: destination.ok_or("missing Cast destination")?,
        namespace: namespace.ok_or("missing Cast namespace")?,
        payload: payload.ok_or("missing Cast payload")?,
    })
}

fn put_varint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

fn put_text(output: &mut Vec<u8>, field: u8, value: &str) {
    output.push(field << 3 | 2);
    put_varint(output, value.len() as u64);
    output.extend_from_slice(value.as_bytes());
}

pub(super) fn encode(message: &Message) -> Result<Vec<u8>, String> {
    text(message.source.as_bytes(), 128)?;
    text(message.destination.as_bytes(), 128)?;
    text(message.namespace.as_bytes(), 256)?;
    if !message.payload.is_object() {
        return Err("Cast payload must be an object".into());
    }
    let payload = serde_json::to_string(&message.payload).map_err(|error| error.to_string())?;
    if payload.len() > MAX_FRAME {
        return Err("Cast payload exceeds its limit".into());
    }
    let mut output = vec![8, 0];
    put_text(&mut output, 2, &message.source);
    put_text(&mut output, 3, &message.destination);
    put_text(&mut output, 4, &message.namespace);
    output.extend_from_slice(&[40, 0]);
    put_text(&mut output, 6, &payload);
    if output.len() > MAX_FRAME {
        return Err("Cast frame exceeds its limit".into());
    }
    Ok(output)
}

pub(super) async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message, String> {
    let length = reader
        .read_u32()
        .await
        .map_err(|error| format!("Cast read: {error}"))?;
    if length == 0 || length as u64 > MAX_FRAME as u64 {
        return Err("Cast frame exceeds its limit".into());
    }
    let mut input = vec![0; length as usize];
    reader
        .read_exact(&mut input)
        .await
        .map_err(|error| format!("Cast read: {error}"))?;
    decode(&input)
}

pub(super) async fn write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
) -> Result<(), String> {
    let output = encode(message)?;
    writer
        .write_u32(output.len() as u32)
        .await
        .map_err(|error| format!("Cast write: {error}"))?;
    writer
        .write_all(&output)
        .await
        .map_err(|error| format!("Cast write: {error}"))?;
    writer
        .flush()
        .await
        .map_err(|error| format!("Cast flush: {error}"))
}

#[cfg(test)]
#[path = "cast_wire_tests.rs"]
mod tests;
