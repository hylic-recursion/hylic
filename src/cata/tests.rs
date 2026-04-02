use crate::domain::shared as dom;
use crate::prelude::{WorkPool, WorkPoolSpec};

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

#[test]
fn parallel_lifts() {
    let tree = N { val: 1, children: vec![
        N { val: 2, children: vec![N { val: 4, children: vec![] }] },
        N { val: 3, children: vec![] },
    ]};
    let graph = dom::treeish(|n: &N| n.children.clone());
    let init = |n: &N| n.val as u64;
    let acc = |a: &mut u64, c: &u64| { *a += c; };
    let my_fold = dom::simple_fold(init, acc);

    use crate::prelude::{ParLazy, ParEager, EagerSpec};
    WorkPool::with(WorkPoolSpec::threads(3), |pool| {
        // Shared domain
        assert_eq!(dom::FUSED.run_lifted(&ParLazy::lift(pool), &my_fold, &graph, &tree), 10);
        assert!(pool.is_idle(), "pool not idle after ParLazy");
        assert_eq!(dom::FUSED.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &my_fold, &graph, &tree), 10);
        assert!(pool.is_idle(), "pool not idle after ParEager");
    });
}

/// Build a balanced tree with `n` nodes for testing.
fn big_tree(n: usize, bf: usize) -> N {
    fn build(id: &mut i32, remaining: &mut usize, bf: usize) -> N {
        let val = *id;
        *id += 1;
        *remaining = remaining.saturating_sub(1);
        let mut children = Vec::new();
        for _ in 0..bf {
            if *remaining == 0 { break; }
            children.push(build(id, remaining, bf));
        }
        N { val, children }
    }
    let mut id = 1;
    let mut remaining = n;
    build(&mut id, &mut remaining, bf)
}

#[test]
fn lifts_domain_generic_comprehensive() {
    use crate::domain::local;
    use crate::cata::exec::{PoolIn, PoolSpec};
    use crate::prelude::{ParLazy, ParEager, EagerSpec};

    // 60-node tree, branch factor 4
    let tree = big_tree(60, 4);

    // Reference answer via Shared + Fused (trusted baseline)
    let shared_fold = dom::simple_fold(
        |n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; });
    let shared_graph = dom::treeish(|n: &N| n.children.clone());
    let expected = dom::FUSED.run(&shared_fold, &shared_graph, &tree);

    // Local domain fold + treeish
    let make_local_fold = || local::fold(
        |n: &N| n.val, |a: &mut i32, c: &i32| { *a += c; }, |h: &i32| *h);
    let make_local_graph = || local::treeish_visit(|n: &N, cb: &mut dyn FnMut(&N)| {
        for c in &n.children { cb(c); }
    });

    WorkPool::with(WorkPoolSpec::threads(3), |pool| {
        let pool_shared = PoolIn::<crate::domain::Shared>::new(pool, PoolSpec::default_for(3));
        let pool_local = PoolIn::<crate::domain::Local>::new(pool, PoolSpec::default_for(3));

        // ── Shared domain: all executor × lift combos ──
        assert_eq!(dom::FUSED.run_lifted(&ParLazy::lift(pool), &shared_fold, &shared_graph, &tree), expected, "Lazy+Fused+Shared");
        assert_eq!(dom::RAYON.run_lifted(&ParLazy::lift(pool), &shared_fold, &shared_graph, &tree), expected, "Lazy+Rayon+Shared");
        assert_eq!(pool_shared.run_lifted(&ParLazy::lift(pool), &shared_fold, &shared_graph, &tree), expected, "Lazy+Pool+Shared");

        assert_eq!(dom::FUSED.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &shared_fold, &shared_graph, &tree), expected, "Eager+Fused+Shared");
        assert_eq!(dom::RAYON.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &shared_fold, &shared_graph, &tree), expected, "Eager+Rayon+Shared");
        assert_eq!(pool_shared.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &shared_fold, &shared_graph, &tree), expected, "Eager+Pool+Shared");

        // ── Local domain: Fused + Pool × lift combos ──
        let lf = make_local_fold();
        let lg = make_local_graph();
        assert_eq!(local::FUSED.run_lifted(&ParLazy::lift(pool), &lf, &lg, &tree), expected, "Lazy+Fused+Local");

        let lf = make_local_fold();
        let lg = make_local_graph();
        assert_eq!(pool_local.run_lifted(&ParLazy::lift(pool), &lf, &lg, &tree), expected, "Lazy+Pool+Local");

        let lf = make_local_fold();
        let lg = make_local_graph();
        assert_eq!(local::FUSED.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &lf, &lg, &tree), expected, "Eager+Fused+Local");

        let lf = make_local_fold();
        let lg = make_local_graph();
        assert_eq!(pool_local.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(3)), &lf, &lg, &tree), expected, "Eager+Pool+Local");

        assert!(pool.is_idle(), "pool not idle after all lift tests");
    });
}
