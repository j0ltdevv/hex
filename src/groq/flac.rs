//! Minimal FLAC writer for Groq uploads.
//!
//! Groq accepts FLAC and compresses a little better than WAV, but a custom
//! encoder only needs to be valid, not clever: every frame uses FLAC's
//! verbatim subframe, so samples are stored as 16-bit little-endian PCM with
//! no prediction. The stream is mono, 16 kHz, 16-bit, with one frame per
//! 4096 samples. STREAMINFO's MD5 is zero, which FLAC defines as "unknown".

use super::pcm_i16;

const SAMPLE_RATE: u32 = 16_000;
const BITS_PER_SAMPLE: u8 = 16;
const BLOCK_SAMPLES: usize = 4096;
const STREAMINFO_LEN: u8 = 34;

/// Encodes mono 16 kHz `f32` samples as a complete FLAC stream.
pub(crate) fn encode(samples: &[f32]) -> Vec<u8> {
    let pcm = pcm_i16(samples);
    let frames = pcm.len().div_ceil(BLOCK_SAMPLES).max(1);
    let mut bytes = Vec::with_capacity(42 + pcm.len() * 2 + frames * 16);

    bytes.extend_from_slice(b"fLaC");
    // STREAMINFO is the only metadata block and is marked last.
    bytes.extend_from_slice(&[0x80, 0x00, 0x00, STREAMINFO_LEN]);
    bytes.resize(8 + STREAMINFO_LEN as usize, 0);

    let mut min_block = usize::MAX;
    let mut max_block = 0_usize;
    let mut min_frame = usize::MAX;
    let mut max_frame = 0_usize;

    for (index, block) in pcm.chunks(BLOCK_SAMPLES).enumerate() {
        let start = bytes.len();
        append_frame(&mut bytes, block, index as u64);
        let frame_len = bytes.len() - start;
        min_block = min_block.min(block.len());
        max_block = max_block.max(block.len());
        min_frame = min_frame.min(frame_len);
        max_frame = max_frame.max(frame_len);
    }
    if pcm.is_empty() {
        min_block = 0;
        min_frame = 0;
    }

    write_streaminfo(
        &mut bytes[8..8 + STREAMINFO_LEN as usize],
        min_block,
        max_block,
        min_frame,
        max_frame,
        pcm.len(),
    );
    bytes
}

fn append_frame(bytes: &mut Vec<u8>, block: &[i16], frame_index: u64) {
    let start = bytes.len();

    // 14-bit sync 0x3ffe, reserved 0, fixed-blocksize strategy 0.
    bytes.extend_from_slice(&0xfff8_u16.to_be_bytes());
    // Block size 7 (16-bit field), 16 kHz, mono, 16-bit, reserved 0.
    let description = (7_u16 << 12) | (5_u16 << 8) | (4_u16 << 1);
    bytes.extend_from_slice(&description.to_be_bytes());

    let (sequence, sequence_len) = utf8(frame_index);
    bytes.extend_from_slice(&sequence[..sequence_len]);
    bytes.extend_from_slice(&((block.len() - 1) as u16).to_be_bytes());
    bytes.push(crc8(&bytes[start..]));

    // Verbatim subframe (type 1), no wasted bits, then its samples.
    bytes.push(0x02);
    for &sample in block {
        bytes.extend_from_slice(&sample.to_be_bytes());
    }
    let checksum = crc16(&bytes[start..]);
    bytes.extend_from_slice(&checksum.to_be_bytes());
}

fn write_streaminfo(
    block: &mut [u8],
    min_block: usize,
    max_block: usize,
    min_frame: usize,
    max_frame: usize,
    samples: usize,
) {
    let mut bits = BitWriter::new(block);
    bits.push(min_block as u64, 16);
    bits.push(max_block as u64, 16);
    bits.push(min_frame as u64, 24);
    bits.push(max_frame as u64, 24);
    bits.push(SAMPLE_RATE as u64, 20);
    bits.push(0, 3); // channels - 1
    bits.push((BITS_PER_SAMPLE - 1) as u64, 5);
    bits.push(samples as u64, 36);
    debug_assert_eq!(bits.bit, 18 * 8);
    // The remaining 16 MD5 bytes stay zero: unknown, which decoders accept.
}

struct BitWriter<'a> {
    bytes: &'a mut [u8],
    bit: usize,
}

impl<'a> BitWriter<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, bit: 0 }
    }

    fn push(&mut self, value: u64, width: usize) {
        for shift in (0..width).rev() {
            if (value >> shift) & 1 == 1 {
                let index = self.bit / 8;
                self.bytes[index] |= 1 << (7 - self.bit % 8);
            }
            self.bit += 1;
        }
    }
}

/// FLAC's variable-length frame number, a UTF-8-like big-endian integer.
fn utf8(value: u64) -> ([u8; 7], usize) {
    let mut bytes = [0_u8; 7];
    if value <= 0x7f {
        bytes[0] = value as u8;
        (bytes, 1)
    } else if value <= 0x7ff {
        bytes[0] = 0xc0 | (value >> 6) as u8;
        bytes[1] = 0x80 | (value & 0x3f) as u8;
        (bytes, 2)
    } else if value <= 0xffff {
        bytes[0] = 0xe0 | (value >> 12) as u8;
        bytes[1] = 0x80 | ((value >> 6) & 0x3f) as u8;
        bytes[2] = 0x80 | (value & 0x3f) as u8;
        (bytes, 3)
    } else if value <= 0x1f_ffff {
        bytes[0] = 0xf0 | (value >> 18) as u8;
        bytes[1] = 0x80 | ((value >> 12) & 0x3f) as u8;
        bytes[2] = 0x80 | ((value >> 6) & 0x3f) as u8;
        bytes[3] = 0x80 | (value & 0x3f) as u8;
        (bytes, 4)
    } else if value <= 0x3ff_ffff {
        bytes[0] = 0xf8 | (value >> 24) as u8;
        bytes[1] = 0x80 | ((value >> 18) & 0x3f) as u8;
        bytes[2] = 0x80 | ((value >> 12) & 0x3f) as u8;
        bytes[3] = 0x80 | ((value >> 6) & 0x3f) as u8;
        bytes[4] = 0x80 | (value & 0x3f) as u8;
        (bytes, 5)
    } else if value <= 0x7fff_ffff {
        bytes[0] = 0xfc | (value >> 30) as u8;
        bytes[1] = 0x80 | ((value >> 24) & 0x3f) as u8;
        bytes[2] = 0x80 | ((value >> 18) & 0x3f) as u8;
        bytes[3] = 0x80 | ((value >> 12) & 0x3f) as u8;
        bytes[4] = 0x80 | ((value >> 6) & 0x3f) as u8;
        bytes[5] = 0x80 | (value & 0x3f) as u8;
        (bytes, 6)
    } else {
        bytes[0] = 0xfe | ((value >> 36) & 1) as u8;
        bytes[1] = 0x80 | ((value >> 30) & 0x3f) as u8;
        bytes[2] = 0x80 | ((value >> 24) & 0x3f) as u8;
        bytes[3] = 0x80 | ((value >> 18) & 0x3f) as u8;
        bytes[4] = 0x80 | ((value >> 12) & 0x3f) as u8;
        bytes[5] = 0x80 | ((value >> 6) & 0x3f) as u8;
        bytes[6] = 0x80 | (value & 0x3f) as u8;
        (bytes, 7)
    }
}

fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for &byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}
