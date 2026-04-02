use hylic::domain::shared::{self as dom, ExecutorExt};
use hylic::prelude::{ParEager, WorkPool, WorkPoolSpec};
use std::sync::Arc;
use std::time::Instant;

type NodeId = usize;
const ROOT: NodeId = 0;

fn gen_tree(node_count: usize, branch_factor: usize) -> (Arc<Vec<Vec<NodeId>>>, usize) {
    let mut children: Vec<Vec<NodeId>> = vec![vec![]];
    let mut next_id = 1usize;
    let mut level_start = 0;
    let mut level_end = 1;
    while next_id < node_count {
        let mut new_end = level_end;
        for parent in level_start..level_end {
            let n_ch = branch_factor.min(node_count - next_id);
            if n_ch == 0 { break; }
            let mut my_ch = Vec::with_capacity(n_ch);
            for _ in 0..n_ch {
                if next_id >= node_count { break; }
                children.push(vec![]);
                my_ch.push(next_id);
                next_id += 1;
                new_end += 1;
            }
            children[parent] = my_ch;
        }
        level_start = level_end;
        level_end = new_end;
        if level_start == level_end { break; }
    }
    let count = children.len();
    (Arc::new(children), count)
}

#[test]
fn no_hang_branching() {
    let (ch, count) = gen_tree(200, 8);
    eprintln!("Tree: {} nodes", count);

    let graph = dom::treeish(move |n: &NodeId| ch[*n].clone());
    let init = |_: &NodeId| 0u64;
    let acc = |a: &mut u64, c: &u64| { *a += c; };
    let my_fold = dom::simple_fold(init, acc);

    WorkPool::with(WorkPoolSpec::threads(3), |pool| {
        for i in 0..20 {
            let t = Instant::now();
            let r = dom::FUSED.run_lifted(&ParEager::lift(pool), &my_fold, &graph, &ROOT);
            eprintln!("  iter {}: {}µs result={}", i, t.elapsed().as_micros(), r);
        }
    });
    eprintln!("ALL 20 DONE");
}
