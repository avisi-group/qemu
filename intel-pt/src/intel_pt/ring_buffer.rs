use {
    memmap2::MmapRaw,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::{
        slice,
        sync::atomic::{fence, AtomicU64, Ordering},
    },
};

pub struct RingBuffer {
    mmap: MmapRaw,
    aux_area: MmapRaw,
    last_head: usize,
}

impl RingBuffer {
    pub fn new(mmap: MmapRaw, aux_area: MmapRaw) -> Self {
        Self {
            mmap,
            aux_area,
            last_head: 0,
        }
    }

    fn page(&self) -> *mut perf_event_mmap_page {
        self.mmap.as_mut_ptr() as *mut _
    }

    pub fn next_record(&mut self) -> Option<Record> {
        let header = self.page();
        fence(Ordering::SeqCst);
        let size = usize::try_from(unsafe { *header }.aux_size).unwrap();
        let head = usize::try_from(unsafe { *header }.aux_head).unwrap();
        fence(Ordering::SeqCst);

        if head == self.last_head {
            return None;
        }

        // fprintf(stderr, "STARTING To Read\n");

        let wrapped_head = head % size;
        let wrapped_tail = self.last_head % size;

        let byte_buffer = if wrapped_head > wrapped_tail {
            // from tail --> head
            ByteBuffer::Single(unsafe {
                slice::from_raw_parts(
                    self.aux_area.as_mut_ptr().add(wrapped_tail),
                    wrapped_head - wrapped_tail,
                )
            })
        } else {
            ByteBuffer::Split([
                unsafe {
                    slice::from_raw_parts(
                        self.aux_area.as_mut_ptr().add(wrapped_tail),
                        size - wrapped_tail,
                    )
                },
                unsafe { slice::from_raw_parts(self.aux_area.as_mut_ptr(), wrapped_tail) },
            ])
        };

        self.last_head = head;

        fence(Ordering::SeqCst);

        let mut old_tail;

        loop {
            let aux_tail =
                unsafe { (&((*header).aux_tail) as *const _ as *const AtomicU64).as_ref() }
                    .unwrap();

            old_tail = match aux_tail.compare_exchange(0, 0, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(val) => val,
                Err(val) => val,
            };

            if aux_tail
                .compare_exchange(old_tail, head as u64, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }

        Some(Record { data: byte_buffer })
    }
}

pub struct Record<'rb> {
    data: ByteBuffer<'rb>,
}

impl<'rb> Record<'rb> {
    pub fn data(&self) -> ByteBuffer<'rb> {
        self.data
    }
}

// Record contains a pointer which prevents it from implementing Send or Sync
// by default. It is, however, valid to send it across threads and it has no
// interior mutability so we implement Send and Sync here manually.
unsafe impl<'s> Sync for Record<'s> {}
unsafe impl<'s> Send for Record<'s> {}

/// A `Buf` that can be either a single byte slice or two disjoint byte
/// slices.
#[derive(Copy, Clone)]
pub enum ByteBuffer<'a> {
    Single(&'a [u8]),
    Split([&'a [u8]; 2]),
}

impl<'a> ByteBuffer<'a> {
    pub fn len(&self) -> usize {
        match self {
            Self::Single(buf) => buf.len(),
            Self::Split([a, b]) => a.len() + b.len(),
        }
    }

    /// Copy bytes from within this byte buffer to the provided slice.
    ///
    /// This will also remove those same bytes from the front of this byte
    /// buffer.
    ///
    /// # Panics
    /// Panics if `self.len() != dst.len()`
    pub fn copy_to_slice(&mut self, dst: &mut [u8]) {
        assert!(self.len() == dst.len());

        match self {
            Self::Single(buf) => {
                dst.copy_from_slice(buf);
            }

            &mut Self::Split([a, b]) => {
                dst[..a.len()].copy_from_slice(a);
                dst[a.len()..].copy_from_slice(b);
            }
        }
    }
}
