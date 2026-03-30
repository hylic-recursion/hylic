use crate::graph::treeish;
use crate::fold;
use crate::cata::ALL;
use super::UIO;

#[test]
fn uio_basic() {
    let u = UIO::new(|| 42);
    assert_eq!(*u.eval(), 42);
    assert_eq!(*u.eval(), 42);
}

#[test]
fn uio_map() {
    let u = UIO::new(|| 10);
    assert_eq!(*u.map(|x| x * 2).eval(), 20);
}

#[test]
fn uio_join_par() {
    let uios: Vec<UIO<i32>> = (0..5).map(|i| UIO::new(move || i * i)).collect();
    assert_eq!(*UIO::join_par(uios).eval(), vec![0, 1, 4, 9, 16]);
}

#[derive(Clone)]
struct N { val: i32, children: Vec<N> }

#[test]
fn all_executors_match() {
    let tree = N { val: 1, children: vec![
        N { val: 2, children: vec![N { val: 4, children: vec![] }] },
        N { val: 3, children: vec![] },
    ]};
    let graph = treeish(|n: &N| n.children.clone());
    let raco = fold::simple_fold(|n: &N| n.val as u64, |a: &mut u64, c: &u64| { *a += c; });

    for exec in ALL {
        assert_eq!(exec.run(&raco, &graph, &tree), 10, "failed for {:?}", exec);
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
    let graph = treeish(|n: &T| n.children.clone());
    let raco = fold::vec_fold(|heap: &fold::VecHeap<T, String>| {
        let ch = heap.childresults.join(", ");
        if ch.is_empty() { heap.node.name.clone() } else { format!("{}[{}]", heap.node.name, ch) }
    });

    for exec in ALL {
        assert_eq!(exec.run(&raco, &graph, &tree), "a[b[d, e], c]", "failed for {:?}", exec);
    }
}
