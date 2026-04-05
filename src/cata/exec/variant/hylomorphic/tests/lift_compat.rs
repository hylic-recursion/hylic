//! Lift compatibility: hylo executor works with ParLazy and ParEager lifts.

use super::*;

#[test]
fn with_lift_lazy() {
    use crate::prelude::ParLazy;
    let tree = big_tree(60, 4);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
        let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(nt));
        assert_eq!(exec.run_lifted(&ParLazy::lift(pool), &fold, &graph, &tree), expected);
    });
}

#[test]
fn with_lift_eager() {
    use crate::prelude::{ParEager, EagerSpec};
    let tree = big_tree(60, 4);
    let fold = sum_fold();
    let graph = n_graph();
    let expected = dom::FUSED.run(&fold, &graph, &tree);
    let nt = n_threads();
    WorkPool::with(WorkPoolSpec::threads(nt), |pool| {
        let exec = HylomorphicIn::<crate::domain::Shared>::new(pool, HylomorphicSpec::default_for(nt));
        assert_eq!(exec.run_lifted(&ParEager::lift(pool, EagerSpec::default_for(nt)), &fold, &graph, &tree), expected);
    });
}
