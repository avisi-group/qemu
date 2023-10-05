use {
    memmap2::MmapRaw,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::{ptr, slice, sync::atomic::Ordering},
};

pub struct RingBuffer {
    mmap: MmapRaw,
    aux_area: MmapRaw,
}

impl RingBuffer {
    pub fn new(mmap: MmapRaw, aux_area: MmapRaw) -> Self {
        Self { mmap, aux_area }
    }

    fn page(&self) -> *const perf_event_mmap_page {
        self.mmap.as_ptr() as *const _
    }

    pub fn next_record(&mut self) -> Option<Record> {
        let page = self.page();

        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        // - data_tail is only written by the user side so it is safe to do a non-atomic
        //   read here.
        let tail = unsafe { ptr::read(ptr::addr_of!((*page).aux_tail)) };
        // ATOMICS:
        // - The acquire load here syncronizes with the release store in the kernel and
        //   ensures that all the data written to the ring buffer before data_head is
        //   visible to this thread.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        let head = unsafe { atomic_load(ptr::addr_of!((*page).aux_head), Ordering::Acquire) };

        if tail == head {
            return None;
        }

        // SAFETY: (for both statements)
        // - page points to a valid instance of perf_event_mmap_page.
        // - neither of these fields are written to except before the map is created so
        //   reading from them non-atomically is safe.
        let data_size = unsafe { ptr::read(ptr::addr_of!((*page).aux_size)) };
        let data_offset = unsafe { ptr::read(ptr::addr_of!((*page).aux_offset)) };

        let mod_tail = (tail % data_size) as usize;
        let mod_head = (head % data_size) as usize;

        // SAFETY:
        // - perf_event_open guarantees that page.data_offset is within the memory
        //   mapping.
        let data_start = unsafe { self.aux_area.as_ptr().add(data_offset as usize) };
        // SAFETY:
        // - data_start is guaranteed to be valid for at least data_size bytes.
        let tail_start = unsafe { data_start.add(mod_tail) };

        let buffer = if mod_head > mod_tail {
            ByteBuffer::Single(unsafe { slice::from_raw_parts(tail_start, mod_head - mod_tail) })
        } else {
            ByteBuffer::Split([
                unsafe { slice::from_raw_parts(tail_start, data_size as usize - mod_tail) },
                unsafe { slice::from_raw_parts(data_start, mod_head) },
            ])
        };

        Some(Record {
            ring_buffer: self,
            data: buffer,
        })
    }
}

pub struct Record<'rb> {
    ring_buffer: &'rb RingBuffer,
    data: ByteBuffer<'rb>,
}

impl<'rb> Record<'rb> {
    /// Get the total length, in bytes, of this record.
    #[allow(clippy::len_without_is_empty)] // Records are never empty
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn data(&self) -> ByteBuffer<'rb> {
        self.data
    }
}

impl<'s> Drop for Record<'s> {
    fn drop(&mut self) {
        let page = self.ring_buffer.page();

        unsafe {
            // SAFETY:
            // - page points to a valid instance of perf_event_mmap_page
            // - data_tail is only written on our side so it is safe to do a non-atomic read
            //   here.
            let tail = ptr::read(ptr::addr_of!((*page).aux_tail));

            // ATOMICS:
            // - The release store here prevents the compiler from re-ordering any reads
            //   past the store to data_tail.
            // SAFETY:
            // - page points to a valid instance of perf_event_mmap_page
            atomic_store(
                ptr::addr_of!((*page).aux_tail),
                tail + (self.len() as u64),
                Ordering::SeqCst,
            );
        }
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
    /// Panics if `self.len() < dst.len()`
    pub fn copy_to_slice(&mut self, dst: &mut [u8]) {
        assert!(self.len() >= dst.len());

        match self {
            Self::Single(buf) => {
                let (head, rest) = buf.split_at(dst.len());
                dst.copy_from_slice(head);
                *buf = rest;
            }
            Self::Split([buf, _]) if buf.len() >= dst.len() => {
                let (head, rest) = buf.split_at(dst.len());
                dst.copy_from_slice(head);
                *buf = rest;
            }
            &mut Self::Split([a, b]) => {
                let (d_head, d_rest) = dst.split_at_mut(a.len());
                let (b_head, b_rest) = b.split_at(d_rest.len());

                d_head.copy_from_slice(a);
                d_rest.copy_from_slice(b_head);
                *self = Self::Single(b_rest);
            }
        }
    }
}

macro_rules! assert_same_size {
    ($a:ty, $b:ty) => {{
        if false {
            let _assert_same_size: [u8; ::std::mem::size_of::<$b>()] =
                [0u8; ::std::mem::size_of::<$a>()];
        }
    }};
}

trait Atomic: Sized + Copy {
    type Atomic;

    unsafe fn store(ptr: *const Self, val: Self, order: Ordering);
    unsafe fn load(ptr: *const Self, order: Ordering) -> Self;
}

macro_rules! impl_atomic {
    ($base:ty, $atomic:ty) => {
        impl Atomic for $base {
            type Atomic = $atomic;

            unsafe fn store(ptr: *const Self, val: Self, order: Ordering) {
                assert_same_size!(Self, Self::Atomic);

                let ptr = ptr as *const Self::Atomic;
                (*ptr).store(val, order)
            }

            unsafe fn load(ptr: *const Self, order: Ordering) -> Self {
                assert_same_size!(Self, Self::Atomic);

                let ptr = ptr as *const Self::Atomic;
                (*ptr).load(order)
            }
        }
    };
}

impl_atomic!(u64, std::sync::atomic::AtomicU64);
impl_atomic!(u32, std::sync::atomic::AtomicU32);
impl_atomic!(u16, std::sync::atomic::AtomicU16);
impl_atomic!(i64, std::sync::atomic::AtomicI64);

/// Do an atomic write to the value stored at `ptr`.
///
/// # Safety
/// - `ptr` must be valid for writes.
/// - `ptr` must be properly aligned.
unsafe fn atomic_store<T: Atomic>(ptr: *const T, val: T, order: Ordering) {
    T::store(ptr, val, order)
}

/// Perform an atomic read from the value stored at `ptr`.
///
/// # Safety
/// - `ptr` must be valid for reads.
/// - `ptr` must be properly aligned.
unsafe fn atomic_load<T: Atomic>(ptr: *const T, order: Ordering) -> T {
    T::load(ptr, order)
}
