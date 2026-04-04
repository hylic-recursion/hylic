//! Pool bridge: concrete view types connecting base/ → submit + fork_join.
//!
//! [`ViewHandle`] implements [`TaskSubmitter`](super::submit::TaskSubmitter).
//! [`PoolExecView`] implements [`TaskRunner`](super::submit::TaskRunner) and provides `join`.

mod view;

pub use super::base::{WorkPool, WorkPoolSpec};
pub use view::{PoolExecView, ViewHandle};

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::fork_join::fork_join_map;

    #[test]
    fn join_basic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(|| 1 + 2, || 3 + 4);
            assert_eq!((a, b), (3, 7));
        });
    }

    #[test]
    fn join_nested() {
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(
                || { let (x, y) = view.join(|| 1, || 2); x + y },
                || { let (x, y) = view.join(|| 3, || 4); x + y },
            );
            assert_eq!(a + b, 10);
        });
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn join_propagates_panic() {
        WorkPool::with(WorkPoolSpec::threads(2), |pool| {
            let view = PoolExecView::new(pool);
            view.join(|| 1, || -> i32 { panic!("boom") });
        });
    }

    #[test]
    fn join_zero_workers() {
        WorkPool::with(WorkPoolSpec::threads(0), |pool| {
            let view = PoolExecView::new(pool);
            let (a, b) = view.join(|| 10, || 20);
            assert_eq!(a + b, 30);
        });
    }

    #[test]
    fn fork_join_map_basic() {
        let items: Vec<i32> = (0..64).collect();
        WorkPool::with(WorkPoolSpec::threads(3), |pool| {
            let view = PoolExecView::new(pool);
            let results = fork_join_map(&view, &items, &|&x| x * 2, 0, 6);
            let expected: Vec<i32> = (0..64).map(|x| x * 2).collect();
            assert_eq!(results, expected);
        });
    }

    #[test]
    fn fork_join_map_preserves_order() {
        let items: Vec<usize> = (0..100).collect();
        WorkPool::with(WorkPoolSpec::threads(4), |pool| {
            let view = PoolExecView::new(pool);
            let results = fork_join_map(&view, &items, &|&x| x, 0, 8);
            assert_eq!(results, items);
        });
    }

    #[test]
    fn sequential_pool_stress() {
        for iteration in 0..20 {
            WorkPool::with(WorkPoolSpec::threads(3), |pool| {
                let view = PoolExecView::new(pool);
                let items: Vec<i32> = (0..64).collect();
                let results = fork_join_map(&view, &items, &|&x| x * 2, 0, 6);
                let expected: Vec<i32> = (0..64).map(|x| x * 2).collect();
                assert_eq!(results, expected, "iteration {iteration}");
            });
        }
    }

    #[test]
    fn concurrent_pools() {
        let t1 = std::thread::spawn(|| {
            WorkPool::with(WorkPoolSpec::threads(2), |pool| {
                let view = PoolExecView::new(pool);
                let (a, b) = view.join(|| 10, || 20);
                assert_eq!(a + b, 30);
            });
        });
        let t2 = std::thread::spawn(|| {
            WorkPool::with(WorkPoolSpec::threads(2), |pool| {
                let view = PoolExecView::new(pool);
                let (a, b) = view.join(|| 100, || 200);
                assert_eq!(a + b, 300);
            });
        });
        t1.join().unwrap();
        t2.join().unwrap();
    }
}
