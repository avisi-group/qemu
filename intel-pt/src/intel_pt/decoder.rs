const PT_OPC_PSB: u64 = 0x02;
const PT_EXT_PSB: u64 = 0x82;

/// The high and low bytes in the pattern
const PT_PSB_HI: u64 = PT_OPC_PSB;
const PT_PSB_LO: u64 = PT_EXT_PSB;

/// Various combinations of the above parts
const PT_PSB_LOHI: u64 = PT_PSB_LO | PT_PSB_HI << 8;
const PT_PSB_HILO: u64 = PT_PSB_HI | PT_PSB_LO << 8;

/// A psb packet contains a unique 2-byte repeating pattern, there are only two ways to fill up a u64 with such a pattern.
const PSB_PATTERNS: [u64; 2] = [
    (PT_PSB_LOHI | PT_PSB_LOHI << 16 | PT_PSB_LOHI << 32 | PT_PSB_LOHI << 48),
    (PT_PSB_HILO | PT_PSB_HILO << 16 | PT_PSB_HILO << 32 | PT_PSB_HILO << 48),
];

/// Finds the index of the next PSB, if it exists
pub fn find_next_sync(buf: &[u8]) -> Option<usize> {
    let offset = buf.as_ptr().align_offset(64);

    buf.windows(64)
        .enumerate()
        .find(|(_, chunk)| {
            *chunk == PSB_PATTERNS[0].to_ne_bytes() || *chunk == PSB_PATTERNS[1].to_ne_bytes()
        })
        .map(|(idx, _)| idx)
}
