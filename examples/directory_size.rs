//! Directory-size computation — the canonical tree fold.
//!
//! Each `Dir` has an own size and zero or more children. The total
//! size of a directory is its own size plus the sum of its
//! descendants'. This example constructs the fold, the treeish
//! describing the tree structure, and runs them under the sequential
//! executor.
//!
//! Run with: `cargo run --example directory_size -p hylic`

use hylic::domain::shared as dom;
use hylic::graph;

#[derive(Clone)]
struct Dir {
    name:     String,
    own_size: u64,
    children: Vec<Dir>,
}

fn main() {
    // A small filesystem tree. In a real program this would come
    // from `std::fs::read_dir` or similar.
    let root = Dir {
        name: "project".into(),
        own_size: 4_096,
        children: vec![
            Dir { name: "src".into(),   own_size: 8_192,
                children: vec![
                    Dir { name: "main.rs".into(), own_size: 12_288, children: vec![] },
                    Dir { name: "lib.rs".into(),  own_size:  6_144, children: vec![] },
                ],
            },
            Dir { name: "docs".into(),  own_size: 2_048,
                children: vec![
                    Dir { name: "intro.md".into(), own_size: 4_096, children: vec![] },
                ],
            },
            Dir { name: "Cargo.toml".into(), own_size: 1_024, children: vec![] },
        ],
    };

    // The treeish: given a directory, yield its subdirectories as
    // children of the fold's walk.
    let structure = graph::treeish(|d: &Dir| d.children.clone());

    // The fold, in its three phases:
    //   init       — seed the accumulator with the node's own size
    //   accumulate — add each child's total into the accumulator
    //   finalize   — emit the accumulator as the node's result
    let sum = dom::fold(
        |d: &Dir| d.own_size,
        |acc: &mut u64, child_total: &u64| *acc += child_total,
        |acc: &u64| *acc,
    );

    // Run sequentially under the Fused executor.
    let total: u64 = dom::FUSED.run(&sum, &structure, &root);

    println!("Total size of '{}': {} bytes", root.name, total);
    assert_eq!(total, 4_096 + 8_192 + 12_288 + 6_144 + 2_048 + 4_096 + 1_024);
}
