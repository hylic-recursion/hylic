//! Complex N/H/R payload tests.
//!
//! Stretches the library beyond primitive u64 types:
//! - owned strings, vecs, hashmaps as heap / result
//! - struct-shaped N with nested data
//! - non-Clone N via Arc-wrapper
//! - custom R types aggregating tree-wide state
//!
//! All under Funnel to exercise the Send+Sync + cross-thread pathway
//! for realistic payloads.

use std::collections::HashMap;
use std::sync::Arc;

use crate::cata::pipeline::{
    SeedPipeline, TreeishPipeline, PipelineExec, PipelineExecSeed,
};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::{edgy_visit, treeish};

// ── struct-shaped N with nested data ─────────────────────

#[derive(Clone, Debug)]
struct Module {
    name:    String,
    version: (u32, u32),
    deps:    Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct ModReport {
    names_seen:  Vec<String>,
    total_deps:  u32,
    max_version: (u32, u32),
}

fn module_registry() -> Arc<HashMap<String, Module>> {
    let mut m = HashMap::new();
    m.insert("app".to_string(), Module { name: "app".into(), version: (1, 2),
        deps: vec!["db".into(), "auth".into()] });
    m.insert("db".to_string(), Module { name: "db".into(), version: (3, 0), deps: vec![] });
    m.insert("auth".to_string(), Module { name: "auth".into(), version: (2, 1),
        deps: vec!["db".into()] });
    Arc::new(m)
}

fn version_max(a: (u32, u32), b: (u32, u32)) -> (u32, u32) {
    if a >= b { a } else { b }
}

#[test]
fn struct_n_string_seeds_vec_result_under_funnel() {
    // N = Module (owned struct). Seed = String (module name).
    // H = ModReport. R = ModReport.

    let registry = module_registry();
    let registry_for_grow = registry.clone();
    let grow = move |seed: &String| -> Module {
        registry_for_grow.get(seed).cloned().unwrap_or_else(
            || Module { name: seed.clone(), version: (0, 0), deps: vec![] })
    };
    let seeds = edgy_visit(move |m: &Module, cb: &mut dyn FnMut(&String)| {
        for d in &m.deps { cb(d); }
    });
    let report_fold = fold(
        |m: &Module| ModReport {
            names_seen:  vec![m.name.clone()],
            total_deps:  m.deps.len() as u32,
            max_version: m.version,
        },
        |h: &mut ModReport, c: &ModReport| {
            h.names_seen.extend(c.names_seen.iter().cloned());
            h.total_deps += c.total_deps;
            h.max_version = version_max(h.max_version, c.max_version);
        },
        |h: &ModReport| h.clone(),
    );

    let r = SeedPipeline::<crate::domain::Shared, Module, String, ModReport, ModReport>::new(
        grow, seeds, &report_fold,
    ).run_from_slice(
        &dom::exec(funnel::Spec::default(4)),
        &["app".to_string()],
        ModReport::default(),
    );

    // "app" seen + its subtree: "app", "db" (via app->db), "auth", "db" (via auth->db)
    assert!(r.names_seen.contains(&"app".to_string()));
    assert!(r.names_seen.contains(&"db".to_string()));
    assert!(r.names_seen.contains(&"auth".to_string()));
    assert_eq!(r.max_version, (3, 0));
    assert!(r.total_deps >= 3);  // app(2) + auth(1) ≥ 3
}

// ── HashMap as H, Vec<String> as R ───────────────────────

#[test]
fn hashmap_heap_and_result_under_funnel_with_shape_lifts() {
    // H = R = HashMap<u32, u32>. Histogram of node values across the
    // tree, plus a zipmap extracting the entry count as a sidecar.

    #[derive(Clone, Debug)]
    struct N { val: u32, kids: Vec<N> }

    let tree = N {
        val: 0,
        kids: vec![
            N { val: 1, kids: vec![ N { val: 3, kids: vec![] } ] },
            N { val: 2, kids: vec![] },
        ],
    };

    let graph = treeish(|n: &N| n.kids.clone());
    let f = fold(
        |n: &N| {
            let mut h = HashMap::new();
            h.insert(n.val, 1u32);
            h
        },
        |h: &mut HashMap<u32, u32>, c: &HashMap<u32, u32>| {
            for (&k, &v) in c { *h.entry(k).or_insert(0) += v; }
        },
        |h: &HashMap<u32, u32>| h.clone(),
    );

    let (r, n_entries): (HashMap<u32, u32>, usize) =
        TreeishPipeline::<crate::domain::Shared, N, HashMap<u32, u32>, HashMap<u32, u32>>::new(graph, &f)
        .lift()
        .wrap_init(|n: &N, orig: &dyn Fn(&N) -> HashMap<u32, u32>| {
            let mut h = orig(n);
            *h.entry(999).or_insert(0) += 1;
            h
        })
        .zipmap(|h: &HashMap<u32, u32>| h.len())
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &tree);

    // Every node contributes a 999 entry (4 nodes).
    assert_eq!(r.get(&999).copied(), Some(4));
    // Values 0, 1, 2, 3 each counted once.
    for k in [0, 1, 2, 3] {
        assert_eq!(r.get(&k).copied(), Some(1), "count for val={k}: {r:?}");
    }
    // zipmap sidecar: total number of keys (0, 1, 2, 3, 999 = 5).
    assert_eq!(n_entries, 5);
}

// ── Non-Clone N via Arc wrapper ──────────────────────────

struct NonClonePayload {
    id:   u64,
    blob: Vec<u8>,
}

#[test]
fn arc_wrapped_non_clone_n_under_funnel() {
    // N = Arc<NonClonePayload>. NonClonePayload itself is not Clone;
    // Arc<_> is Clone+Send+Sync. The library accepts N: Clone, so
    // Arc wrapping is the standard pattern for non-Clone payloads.

    let make = |id: u64, blob: Vec<u8>| Arc::new(NonClonePayload { id, blob });
    let root = make(0, vec![1, 2, 3]);

    let tree_data: Arc<Vec<Arc<NonClonePayload>>> = Arc::new(vec![
        make(10, vec![10; 4]),
        make(20, vec![20; 6]),
    ]);
    let tree_for_graph = tree_data.clone();
    let graph = treeish(move |n: &Arc<NonClonePayload>| {
        if n.id == 0 { tree_for_graph.iter().cloned().collect() } else { vec![] }
    });

    // H = (u64, usize): (id_sum, total_blob_bytes).
    let f = fold(
        |n: &Arc<NonClonePayload>| (n.id, n.blob.len()),
        |h: &mut (u64, usize), c: &(u64, usize)| { h.0 += c.0; h.1 += c.1; },
        |h: &(u64, usize)| *h,
    );

    let (id_sum, byte_sum) = TreeishPipeline::<crate::domain::Shared, Arc<NonClonePayload>, (u64, usize), (u64, usize)>::new(graph, &f)
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);

    // 0 + 10 + 20 = 30 ids; 3 + 4 + 6 = 13 bytes.
    assert_eq!(id_sum, 30);
    assert_eq!(byte_sum, 13);
}

// ── Deeply nested R via Explainer-style trace ────────────

#[test]
fn vec_of_structs_result_under_funnel() {
    // R = Vec<(String, u32)> — accumulates labelled depth pairs.
    #[derive(Clone, Debug)]
    struct Labelled { label: String, depth: u32 }

    let kids_for = |l: &Labelled| -> Vec<Labelled> {
        if l.depth < 3 {
            (0..2).map(|i| Labelled {
                label: format!("{}/{i}", l.label),
                depth: l.depth + 1,
            }).collect()
        } else {
            vec![]
        }
    };
    let graph = treeish(kids_for);
    let f = fold(
        |n: &Labelled| vec![(n.label.clone(), n.depth)],
        |h: &mut Vec<(String, u32)>, c: &Vec<(String, u32)>| {
            h.extend(c.iter().cloned())
        },
        |h: &Vec<(String, u32)>| h.clone(),
    );

    let r = TreeishPipeline::<crate::domain::Shared, Labelled, Vec<(String, u32)>, Vec<(String, u32)>>::new(graph, &f)
        .run_from_node(
            &dom::exec(funnel::Spec::default(4)),
            &Labelled { label: "root".into(), depth: 0 },
        );

    // Tree has 1 + 2 + 4 + 8 = 15 nodes (depth 0..3).
    assert_eq!(r.len(), 15);
    assert!(r.iter().any(|(label, _)| label == "root"));
    assert!(r.iter().any(|(label, d)| label == "root/0/1/0" && *d == 3));
}
