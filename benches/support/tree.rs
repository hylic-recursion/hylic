use std::sync::Arc;

pub type NodeId = usize;

pub struct TreeSpec {
    pub node_count: usize,
    pub branch_factor: usize,
}

/// Deterministic breadth-first balanced tree.
/// Returns adjacency list + actual node count.
pub fn gen_tree(spec: &TreeSpec) -> (Arc<Vec<Vec<NodeId>>>, usize) {
    let mut children: Vec<Vec<NodeId>> = Vec::new();
    children.push(vec![]);
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;

    while next_id < spec.node_count {
        let mut new_level_end = level_end;
        for parent in level_start..level_end {
            let n_ch = spec.branch_factor.min(spec.node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_children = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= spec.node_count { break; }
                children.push(vec![]);
                my_children.push(next_id);
                next_id += 1;
                new_level_end += 1;
            }
            children[parent] = my_children;
        }
        level_start = level_end;
        level_end = new_level_end;
        if level_start == level_end { break; }
    }

    let count = children.len();
    (Arc::new(children), count)
}
