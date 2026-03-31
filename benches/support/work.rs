use std::hint::black_box;

/// Deterministic CPU burn. Returns a hash-like u64 to prevent elision.
pub fn busy_work(iterations: u64) -> u64 {
    let mut x: u64 = 0xDEAD_BEEF;
    for _ in 0..iterations {
        x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
    }
    x
}

/// Simulated I/O latency via spin-wait.
pub fn spin_wait_us(micros: u64) {
    if micros == 0 { return; }
    let start = std::time::Instant::now();
    while start.elapsed().as_micros() < micros as u128 {
        std::hint::spin_loop();
    }
}

/// Per-phase work distribution.
#[derive(Clone)]
pub struct WorkSpec {
    pub init_work: u64,
    pub accumulate_work: u64,
    pub finalize_work: u64,
    pub graph_work: u64,
    pub graph_io_us: u64,
}

impl WorkSpec {
    /// Execute init-phase work. Returns a seed value.
    pub fn do_init(&self) -> u64 {
        if self.init_work > 0 { busy_work(self.init_work) } else { 0 }
    }

    /// Execute accumulate-phase work.
    pub fn do_accumulate(&self, heap: &mut u64, child: &u64) {
        if self.accumulate_work > 0 {
            *heap = heap.wrapping_add(busy_work(self.accumulate_work));
        }
        *heap = heap.wrapping_add(*child);
    }

    /// Execute finalize-phase work.
    pub fn do_finalize(&self, heap: &u64) -> u64 {
        if self.finalize_work > 0 {
            heap.wrapping_add(busy_work(self.finalize_work))
        } else {
            *heap
        }
    }

    /// Execute graph-traversal work (child discovery).
    pub fn do_graph(&self) {
        spin_wait_us(self.graph_io_us);
        if self.graph_work > 0 { black_box(busy_work(self.graph_work)); }
    }
}
