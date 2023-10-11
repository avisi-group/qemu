use std::{mem::size_of, slice};

use libipt::{packet::PacketDecoder, ConfigBuilder};

const PT_OPC_PSB: u8 = 0x02;
const PT_EXT_PSB: u8 = 0x82;

/// The high and low bytes in the pattern
const PT_PSB_HI: u8 = PT_OPC_PSB;
const PT_PSB_LO: u8 = PT_EXT_PSB;

/// Various combinations of the above parts
const PT_PSB_LOHI: u16 = (PT_PSB_LO as u16) | (PT_PSB_HI as u16) << 8;
const PT_PSB_HILO: u16 = (PT_PSB_HI as u16) | (PT_PSB_LO as u16) << 8;

/// The repeat count of the payload, not including opc and ext.
const PT_PSB_REPEAT_COUNT: usize = 7;

/// The size of the repeated pattern in bytes.
const PT_PSB_REPEAT_SIZE: usize = 2;

/* The size of a PSB packet's payload in bytes. */
const PT_PL_PSB_SIZE: usize = PT_PSB_REPEAT_COUNT * PT_PSB_REPEAT_SIZE;

const PT_OPCS_PSB: usize = 2;

const PTPS_PSB: usize = PT_OPCS_PSB + PT_PL_PSB_SIZE;

/// A psb packet contains a unique 2-byte repeating pattern, there are only two
/// ways to fill up a u64 with such a pattern.
const PSB_PATTERNS: [u64; 2] = [
    ((PT_PSB_LOHI as u64)
        | (PT_PSB_LOHI as u64) << 16
        | (PT_PSB_LOHI as u64) << 32
        | (PT_PSB_LOHI as u64) << 48),
    ((PT_PSB_HILO as u64)
        | (PT_PSB_HILO as u64) << 16
        | (PT_PSB_HILO as u64) << 32
        | (PT_PSB_HILO as u64) << 48),
];

/// Finds the index of the next PSB, if it exists
pub fn find_next_sync(slice: &mut [u8]) -> Option<usize> {
    let expected = {
        let mut decoder = PacketDecoder::new(&ConfigBuilder::new(slice).unwrap().finish()).unwrap();

        decoder
            .sync_forward()
            .ok()
            .map(|_| decoder.sync_offset().unwrap() as usize)
    };

    // let offset = slice.as_ptr().align_offset(size_of::<u64>());

    // let got = slice[offset..]
    //     .chunks(size_of::<u64>())
    //     .enumerate()
    //     .find(|(_, chunk)| {
    //         **chunk == PSB_PATTERNS[0].to_ne_bytes() || *chunk ==
    // PSB_PATTERNS[1].to_ne_bytes()     })
    //     .map(|(idx, _)| idx)
    //     .and_then(|idx| find_psb(slice, idx));

    // assert_eq!(expected, got);

    expected
}

pub fn _find_psb(slice: &[u8], mut idx: usize) -> Option<usize> {
    if slice[idx] != PT_PSB_HI {
        idx += 1;
    }

    loop {
        if idx >= slice.len() {
            break;
        }

        let hi = slice[idx];
        let lo = slice[idx + 1];

        if hi != PT_PSB_HI {
            break;
        }
        if lo != PT_PSB_LO {
            break;
        }

        idx += 2;
    }

    idx -= PTPS_PSB;

    if idx >= slice.len() {
        println!("idx exceeded slice");
        return None;
    }

    if slice[idx] != PT_OPC_PSB || slice[idx + 1] != PT_EXT_PSB {
        return None;
    }

    Some(idx)
}
