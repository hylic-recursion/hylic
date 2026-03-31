use std::hint::black_box;

/// Node is just an ID — O(1) clone. Tree structure lives externally.
pub type NodeId = usize;

/// External adjacency list.
pub struct Tree {
    pub children: Vec<Vec<NodeId>>,
    pub root: NodeId,
}

pub struct TreeSpec {
    pub node_count: usize,
    pub branch_factor: usize,
}

pub fn gen_tree(spec: &TreeSpec, seed: u64) -> Tree {
    let mut rng = SimpleRng(seed);
    let mut children: Vec<Vec<NodeId>> = vec![vec![]]; // root
    let mut built = 1;
    build_subtree(0, spec, &mut built, &mut rng, &mut children);
    Tree { children, root: 0 }
}

fn build_subtree(
    id: NodeId, spec: &TreeSpec, built: &mut usize,
    rng: &mut SimpleRng, children: &mut Vec<Vec<NodeId>>,
) {
    if *built >= spec.node_count { return; }
    let max_ch = spec.branch_factor.min(spec.node_count - *built);
    let n_ch = if max_ch == 0 { 0 } else { 1 + rng.next_usize() % max_ch };
    let mut my_children = Vec::with_capacity(n_ch);
    for _ in 0..n_ch {
        let cid = *built;
        *built += 1;
        children.push(vec![]);
        my_children.push(cid);
        build_subtree(cid, spec, built, rng, children);
    }
    children[id] = my_children;
}

struct SimpleRng(u64);
impl SimpleRng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0
    }
    fn next_usize(&mut self) -> usize { self.next_u64() as usize }
}

pub fn busy_work(iterations: u64) -> u64 {
    let mut x: u64 = 0xDEAD_BEEF;
    for _ in 0..iterations {
        x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
    }
    x
}

pub fn spin_wait_us(micros: u64) {
    if micros == 0 { return; }
    let start = std::time::Instant::now();
    while start.elapsed().as_micros() < micros as u128 {
        std::hint::spin_loop();
    }
}
