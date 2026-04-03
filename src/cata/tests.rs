use crate::domain::shared as dom;

#[derive(Clone)]
struct N { val: i32, children: Vec<N> }

#[test]
fn all_executors_match() {
    let tree = N { val: 1, children: vec![
        N { val: 2, children: vec![N { val: 4, children: vec![] }] },
        N { val: 3, children: vec![] },
    ]};
    let graph = dom::treeish(|n: &N| n.children.clone());
    let init = |n: &N| n.val as u64;
    let acc = |a: &mut u64, c: &u64| { *a += c; };
    let my_fold = dom::simple_fold(init, acc);

    for exec in [dom::DynExec::<N, u64>::fused(), dom::DynExec::rayon()] {
        assert_eq!(exec.run(&my_fold, &graph, &tree), 10);
    }
}

#[test]
fn all_executors_vec_fold() {
    #[derive(Clone)]
    struct T { name: String, children: Vec<T> }
    impl T {
        fn leaf(s: &str) -> Self { T { name: s.into(), children: vec![] } }
        fn branch(s: &str, ch: Vec<T>) -> Self { T { name: s.into(), children: ch } }
    }

    let tree = T::branch("a", vec![T::branch("b", vec![T::leaf("d"), T::leaf("e")]), T::leaf("c")]);
    let graph = dom::treeish(|n: &T| n.children.clone());
    use crate::prelude::{vec_fold, VecHeap};
    let format = |heap: &VecHeap<T, String>| {
        let ch = heap.childresults.join(", ");
        if ch.is_empty() { heap.node.name.clone() } else { format!("{}[{}]", heap.node.name, ch) }
    };
    let my_fold = vec_fold(format);

    for exec in [dom::DynExec::<T, String>::fused(), dom::DynExec::rayon()] {
        assert_eq!(exec.run(&my_fold, &graph, &tree), "a[b[d, e], c]");
    }
}

// Tests below require the parallel module (WorkPool, PoolIn, ParLazy, ParEager).
// They will be reimplemented when the new parallel infrastructure is ready.
// See .bak/tests_archive/cata_tests.rs for the original versions.

// fn parallel_lifts() { ... }
// fn lifts_domain_generic_comprehensive() { ... }
