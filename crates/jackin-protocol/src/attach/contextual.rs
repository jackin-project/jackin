// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned request/response envelopes for bounded attach controls.

use anyhow::{Context, Result, bail};

use super::{
    AttachControlOperation, AttachControlRequest, AttachControlResponse, AttachControlResult,
    ClipboardImage, ClipboardImageChunk, ClipboardImageEnd, ClipboardImageError,
    ClipboardImageFormat, ClipboardImageStart, FILE_EXPORT_DIGEST_BYTES, MAX_CLIPBOARD_IMAGE_BYTES,
    MAX_CLIPBOARD_IMAGE_ERROR_BYTES, MAX_CLIPBOARD_IMAGE_FRAME_PAYLOAD, PayloadCursor,
    TAG_ATTACH_CONTROL, TAG_ATTACH_CONTROL_RESPONSE, encode, encode_clipboard_image_chunk,
    encode_clipboard_image_end, encode_clipboard_image_start,
};

pub(super) fn encode_request(request: AttachControlRequest) -> Result<Vec<u8>> {
    let context = serde_json::to_vec(&request.context)
        .context("serializing attach control telemetry context")?;
    let context_len = u16::try_from(context.len())
        .map_err(|_| anyhow::anyhow!("attach control telemetry context exceeds u16::MAX"))?;
    let mut payload = Vec::with_capacity(11 + context.len());
    let kind = match &request.operation {
        AttachControlOperation::Detach => 1,
        AttachControlOperation::FocusIn => 2,
        AttachControlOperation::FocusOut => 3,
        AttachControlOperation::ClipboardImage(_) => 4,
        AttachControlOperation::ClipboardImageStart(_) => 5,
        AttachControlOperation::ClipboardImageChunk(_) => 6,
        AttachControlOperation::ClipboardImageEnd(_) => 7,
        AttachControlOperation::ClipboardImageError(_) => 8,
    };
    payload.push(kind);
    payload.extend_from_slice(&request.request_id.to_be_bytes());
    payload.extend_from_slice(&context_len.to_be_bytes());
    payload.extend_from_slice(&context);
    encode_operation(&mut payload, request.operation)?;
    if payload.len() > MAX_CLIPBOARD_IMAGE_FRAME_PAYLOAD {
        bail!("contextual attach control payload exceeds frame limit");
    }
    Ok(encode(TAG_ATTACH_CONTROL, &payload))
}

fn encode_operation(payload: &mut Vec<u8>, operation: AttachControlOperation) -> Result<()> {
    match operation {
        AttachControlOperation::Detach
        | AttachControlOperation::FocusIn
        | AttachControlOperation::FocusOut => {}
        AttachControlOperation::ClipboardImage(image) => {
            if image.bytes.is_empty() || image.bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
                bail!("clipboard image payload size is outside the bounded range");
            }
            payload.push(image.format.tag());
            payload.extend_from_slice(&image.bytes);
        }
        AttachControlOperation::ClipboardImageStart(start) => {
            let encoded = encode_clipboard_image_start(start);
            payload.extend_from_slice(encoded.get(5..).unwrap_or_default());
        }
        AttachControlOperation::ClipboardImageChunk(chunk) => {
            let encoded = encode_clipboard_image_chunk(chunk);
            payload.extend_from_slice(encoded.get(5..).unwrap_or_default());
        }
        AttachControlOperation::ClipboardImageEnd(end) => {
            let encoded = encode_clipboard_image_end(end);
            payload.extend_from_slice(encoded.get(5..).unwrap_or_default());
        }
        AttachControlOperation::ClipboardImageError(error) => {
            let message = error.message().as_bytes();
            if message.is_empty() || message.len() > MAX_CLIPBOARD_IMAGE_ERROR_BYTES {
                bail!("clipboard image error message is outside the bounded range");
            }
            payload.extend_from_slice(message);
        }
    }
    Ok(())
}

pub(super) fn decode_request(payload: &[u8]) -> Result<AttachControlRequest> {
    let mut cursor = PayloadCursor::new(payload);
    let kind = cursor.read_u8("attach control kind")?;
    let request_id = cursor.read_u64("attach control request id")?;
    let context_len = cursor.read_u16("attach control context length")? as usize;
    let context = serde_json::from_slice(cursor.read_bytes(context_len, "attach control context")?)
        .context("decoding attach control telemetry context")?;
    let operation = decode_operation(kind, &mut cursor)?;
    if !cursor.finished() {
        bail!("attach control payload has trailing bytes");
    }
    Ok(AttachControlRequest {
        request_id,
        context,
        operation,
    })
}

fn decode_operation(kind: u8, cursor: &mut PayloadCursor<'_>) -> Result<AttachControlOperation> {
    Ok(match kind {
        1 => AttachControlOperation::Detach,
        2 => AttachControlOperation::FocusIn,
        3 => AttachControlOperation::FocusOut,
        4 => {
            let format = ClipboardImageFormat::from_tag(cursor.read_u8("clipboard image format")?)?;
            let bytes = cursor.read_remaining("clipboard image bytes")?.to_vec();
            if bytes.is_empty() || bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
                bail!("clipboard image payload size is outside the bounded range");
            }
            AttachControlOperation::ClipboardImage(ClipboardImage { format, bytes })
        }
        5 => AttachControlOperation::ClipboardImageStart(ClipboardImageStart {
            transfer_id: cursor.read_u64("clipboard image transfer id")?,
            format: ClipboardImageFormat::from_tag(cursor.read_u8("clipboard image format")?)?,
            size: cursor.read_u64("clipboard image size")?,
        }),
        6 => AttachControlOperation::ClipboardImageChunk(ClipboardImageChunk {
            transfer_id: cursor.read_u64("clipboard image transfer id")?,
            offset: cursor.read_u64("clipboard image offset")?,
            bytes: cursor
                .read_remaining("clipboard image chunk bytes")?
                .to_vec(),
        }),
        7 => {
            let transfer_id = cursor.read_u64("clipboard image transfer id")?;
            let sha256 = cursor
                .read_bytes(FILE_EXPORT_DIGEST_BYTES, "clipboard image sha256")?
                .try_into()
                .map_err(|_| anyhow::anyhow!("clipboard image digest length mismatch"))?;
            AttachControlOperation::ClipboardImageEnd(ClipboardImageEnd {
                transfer_id,
                sha256,
            })
        }
        8 => {
            let message =
                std::str::from_utf8(cursor.read_remaining("clipboard image error message")?)
                    .context("clipboard image error message is not valid UTF-8")?;
            AttachControlOperation::ClipboardImageError(ClipboardImageError::from_message(
                message.to_owned(),
            ))
        }
        other => bail!("unknown attach control kind {other}"),
    })
}

pub(super) fn encode_response(response: AttachControlResponse) -> Vec<u8> {
    let mut payload = Vec::with_capacity(9);
    payload.extend_from_slice(&response.request_id.to_be_bytes());
    payload.push(match response.result {
        AttachControlResult::Success => 0,
        AttachControlResult::InvalidCorrelation => 1,
        AttachControlResult::Rejected => 2,
    });
    encode(TAG_ATTACH_CONTROL_RESPONSE, &payload)
}

pub(super) fn decode_response(payload: &[u8]) -> Result<AttachControlResponse> {
    let mut cursor = PayloadCursor::new(payload);
    let request_id = cursor.read_u64("attach control response request id")?;
    let result = match cursor.read_u8("attach control response result")? {
        0 => AttachControlResult::Success,
        1 => AttachControlResult::InvalidCorrelation,
        2 => AttachControlResult::Rejected,
        other => bail!("unknown attach control response result {other}"),
    };
    if !cursor.finished() {
        bail!("attach control response has trailing bytes");
    }
    Ok(AttachControlResponse { request_id, result })
}
