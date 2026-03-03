//! H.264 Annex-B parsing and MP4 sample payload packing helpers.

use std::error::Error;

const H264_NAL_TYPE_IDR: u8 = 5;
const H264_NAL_TYPE_SPS: u8 = 7;
const H264_NAL_TYPE_PPS: u8 = 8;

pub(super) struct EncodedFramePayload {
    pub(super) sample_payload: Vec<u8>,
    pub(super) is_sync: bool,
    pub(super) sps: Option<Vec<u8>>,
    pub(super) pps: Option<Vec<u8>>,
}

pub(super) fn append_h264_unit_to_payload(
    unit: &[u8],
    payload: &mut EncodedFramePayload,
) -> Result<(), Box<dyn Error>> {
    if unit.is_empty() {
        return Ok(());
    }
    let nal_type = unit[0] & 0x1F;
    if nal_type == H264_NAL_TYPE_SPS {
        payload.sps = Some(unit.to_vec());
        return Ok(());
    }
    if nal_type == H264_NAL_TYPE_PPS {
        payload.pps = Some(unit.to_vec());
        return Ok(());
    }
    if nal_type == H264_NAL_TYPE_IDR {
        payload.is_sync = true;
    }
    append_length_prefixed_nal(&mut payload.sample_payload, unit)
}

#[cfg(windows)]
pub(super) fn parse_annex_b_packet(packet: &[u8]) -> Result<EncodedFramePayload, Box<dyn Error>> {
    let mut payload = EncodedFramePayload {
        sample_payload: Vec::new(),
        is_sync: false,
        sps: None,
        pps: None,
    };
    for unit in annex_b_units(packet) {
        append_h264_unit_to_payload(unit, &mut payload)?;
    }
    Ok(payload)
}

#[cfg(windows)]
fn annex_b_units(packet: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut cursor = 0usize;
    while let Some((start, prefix_len)) = find_annex_b_start_code(packet, cursor) {
        let unit_start = start + prefix_len;
        let next = find_annex_b_start_code(packet, unit_start)
            .map(|(index, _)| index)
            .unwrap_or(packet.len());
        if unit_start < next {
            units.push(&packet[unit_start..next]);
        }
        cursor = next;
    }
    units
}

#[cfg(windows)]
fn find_annex_b_start_code(packet: &[u8], from: usize) -> Option<(usize, usize)> {
    if packet.len() < 3 || from >= packet.len() {
        return None;
    }
    let mut idx = from;
    while idx + 3 <= packet.len() {
        if idx + 4 <= packet.len()
            && packet[idx] == 0
            && packet[idx + 1] == 0
            && packet[idx + 2] == 0
            && packet[idx + 3] == 1
        {
            return Some((idx, 4));
        }
        if packet[idx] == 0 && packet[idx + 1] == 0 && packet[idx + 2] == 1 {
            return Some((idx, 3));
        }
        idx += 1;
    }
    None
}

fn append_length_prefixed_nal(dst: &mut Vec<u8>, payload: &[u8]) -> Result<(), Box<dyn Error>> {
    let len = u32::try_from(payload.len()).map_err(|_| "NAL payload is too large")?;
    dst.extend_from_slice(&len.to_be_bytes());
    dst.extend_from_slice(payload);
    Ok(())
}

pub(super) fn strip_annex_b_start_code(nal: &[u8]) -> &[u8] {
    if nal.starts_with(&[0, 0, 0, 1]) {
        return &nal[4..];
    }
    if nal.starts_with(&[0, 0, 1]) {
        return &nal[3..];
    }
    nal
}
