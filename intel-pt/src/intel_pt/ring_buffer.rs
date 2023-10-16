use {
    memmap2::MmapRaw,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::{ptr, slice, sync::atomic::Ordering},
};

pub struct RingBuffer {
    mmap: MmapRaw,
    aux_area: MmapRaw,
    size: usize,
}

impl RingBuffer {
    pub fn new(mmap: MmapRaw, aux_area: MmapRaw) -> Self {
        let size = unsafe {
            ptr::read(ptr::addr_of!(
                (*(mmap.as_mut_ptr() as *mut perf_event_mmap_page)).aux_size
            ))
        } as usize;

        Self {
            mmap,
            aux_area,
            size,
        }
    }

    fn page(&self) -> *mut perf_event_mmap_page {
        self.mmap.as_mut_ptr() as *mut _
    }

    pub fn next_record<F: FnMut(&[u8])>(&mut self, mut process: F) -> bool {
        let header = self.page();

        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        // - data_tail is only written by the user side so it is safe to do a non-atomic
        //   read here.
        let tail = unsafe { ptr::read(ptr::addr_of!((*header).aux_tail)) } as usize;

        // ATOMICS:
        // - The acquire load here syncronizes with the release store in the kernel and
        //   ensures that all the data written to the ring buffer before aux_head is
        //   visible to this thread.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        let head =
            unsafe { atomic_load(ptr::addr_of!((*header).aux_head), Ordering::Acquire) } as usize;

        if tail == head {
            return false;
        }

        let wrapped_head = head % self.size;
        let wrapped_tail = tail % self.size;

        if wrapped_head > wrapped_tail {
            // from tail --> head
            let slice = unsafe {
                slice::from_raw_parts(
                    self.aux_area.as_mut_ptr().add(wrapped_tail),
                    wrapped_head - wrapped_tail,
                )
            };
            process(slice);
        } else {
            let a = unsafe {
                slice::from_raw_parts(
                    self.aux_area.as_mut_ptr().add(wrapped_tail),
                    self.size - wrapped_tail,
                )
            };
            process(a);

            let b = unsafe { slice::from_raw_parts(self.aux_area.as_mut_ptr(), wrapped_head) };
            process(b);
        };

        // ATOMICS:
        // - The release store here prevents the compiler from re-ordering any reads
        //   past the store to data_tail.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page
        unsafe {
            atomic_store(
                ptr::addr_of!((*header).aux_tail),
                u64::try_from(head).unwrap(),
                Ordering::Release,
            );
        }

        true
    }
}

/// A `Buf` that can be either a single byte slice or two disjoint byte
/// slices.
#[derive(Clone, Copy)]
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
    pub fn copy_to_slice(&self, dst: &mut [u8]) {
        assert!(self.len() == dst.len());

        match self {
            Self::Single(buf) => {
                dst.copy_from_slice(buf);
            }
            Self::Split([a, b]) => {
                dst[..a.len()].copy_from_slice(a);
                dst[a.len()..].copy_from_slice(b);
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
