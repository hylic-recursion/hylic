//! ParLazy: lazy parallel fold as a Lift — domain-generic.
//!
//! Phase 1 (sequential): builds a data tree of LazyNode values.
//! Phase 2 (parallel): evaluates bottom-up via fork_join_map,
//! borrowing the fold through SyncRef.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use crate::cata::Lift;
use crate::domain::{ConstructFold, Domain};
use crate::ops::FoldOps;
use crate::prelude::parallel::pool::{WorkPool, PoolExecView, SyncRef, fork_join_map};

struct LazyNode<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
    result: OnceLock<R>,
}

pub struct LazyHeap<H, R> {
    heap: H,
    children: Vec<Arc<LazyNode<H, R>>>,
}

pub struct LazyResult<H, R> {
    node: Arc<LazyNode<H, R>>,
}

impl<H, R> Clone for LazyResult<H, R> {
    fn clone(&self) -> Self { LazyResult { node: self.node.clone() } }
}

fn eval_node<N, H: Clone, R: Clone + Send, F: FoldOps<N, H, R>>(
    node: &LazyNode<H, R>,
    fold: &SyncRef<'_, F>,
    view: &PoolExecView,
) -> R {
    node.result.get_or_init(|| {
        let mut h = node.heap.clone();
        let n = node.children.len();
        if n == 0 {
            fold.finalize(&h)
        } else if n == 1 {
            let r = eval_node(&node.children[0], fold, view);
            fold.accumulate(&mut h, &r);
            fold.finalize(&h)
        } else {
            let results = fork_join_map(
                view, &node.children,
                &|child: &Arc<LazyNode<H, R>>| eval_node(child, fold, view),
                0, 8,
            );
            for r in &results { fold.accumulate(&mut h, r); }
            fold.finalize(&h)
        }
    }).clone()
}

pub struct ParLazy;

impl ParLazy {
    pub fn lift<D, N, H, R>(pool: &Arc<WorkPool>) -> Lift<D, N, H, R, N, LazyHeap<H, R>, LazyResult<H, R>>
    where
        D: Domain<N> + ConstructFold<N>,
        <D as Domain<N>>::Fold<H, R>: Clone,
        N: Clone + 'static,
        H: Clone + 'static,
        R: Clone + Send + 'static,
    {
        let pool = pool.clone();
        let stash: Rc<RefCell<Option<<D as Domain<N>>::Fold<H, R>>>> = Rc::new(RefCell::new(None));
        let stash_write = stash.clone();
        let stash_read = stash.clone();

        Lift::new(
            |treeish| treeish,
            move |original_fold: <D as Domain<N>>::Fold<H, R>| {
                *stash_write.borrow_mut() = Some(original_fold.clone());
                let f = original_fold;
                unsafe { D::make_fold(
                    move |node: &N| -> LazyHeap<H, R> {
                        LazyHeap { heap: f.init(node), children: Vec::new() }
                    },
                    |heap: &mut LazyHeap<H, R>, child: &LazyResult<H, R>| {
                        heap.children.push(child.node.clone());
                    },
                    |heap: &LazyHeap<H, R>| -> LazyResult<H, R> {
                        LazyResult {
                            node: Arc::new(LazyNode {
                                heap: heap.heap.clone(),
                                children: heap.children.clone(),
                                result: OnceLock::new(),
                            }),
                        }
                    },
                ) }
            },
            |n: &N| n.clone(),
            move |result: LazyResult<H, R>| {
                let fold = stash_read.borrow_mut().take()
                    .expect("ParLazy: fold not stashed");
                let view = PoolExecView::new(&pool);
                let sync_fold = SyncRef(&fold);
                eval_node::<N, H, R, _>(&result.node, &sync_fold, &view)
            },
        )
    }
}
