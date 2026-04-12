use std::sync::{Arc, Mutex};
use super::*;

#[test]
fn wrap_grow_adds_logging() {
    let pipeline = make_pipeline();
    let log: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new(Vec::new()));

    let transformed = pipeline.wrap_grow({
        let log = log.clone();
        move |seed: &S, original: &dyn Fn(&S) -> N| {
            log.lock().unwrap().push(*seed);
            original(seed)
        }
    });

    transformed.run_from_slice(&dom::FUSED, &[0], 0u64);
    let logged = log.lock().unwrap();
    assert!(logged.contains(&0));
    assert!(logged.contains(&1));
    assert!(logged.contains(&2));
    assert!(logged.contains(&3));
}

#[test]
fn filter_seeds_prunes_dependencies() {
    let pipeline = make_pipeline();
    let pruned = pipeline.filter_seeds(|seed: &S| *seed != 2);
    let result = pruned.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 4); // 0+1+3, skipped 2
}

#[test]
fn wrap_finalize_transforms_output() {
    let pipeline = make_pipeline().wrap_finalize(
        |h: &u64, original: &dyn Fn(&u64) -> u64| original(h) * 2
    );
    // Entry: (0+36)*2=72
    let result = pipeline.run_from_slice(&dom::FUSED, &[0], 0u64);
    assert_eq!(result, 72);
}
