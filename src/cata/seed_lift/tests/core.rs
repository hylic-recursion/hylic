use super::*;

#[test]
fn convergence_node_entry() {
    let pipeline = make_pipeline();
    let original = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);
    let result = pipeline.run_node(&dom::FUSED, &0, 0u64);
    assert_eq!(original, result);
}

#[test]
fn seed_entry_grows_then_converges() {
    let pipeline = make_pipeline();
    let direct = dom::FUSED.run(&sum_fold(), &test_treeish(), &1);
    let result = pipeline.run_seed(&dom::FUSED, &1, 0u64);
    assert_eq!(direct, result);
}

#[test]
fn entry_branches_into_seeds() {
    let pipeline = make_pipeline();
    let r1 = dom::FUSED.run(&sum_fold(), &test_treeish(), &1);
    let r2 = dom::FUSED.run(&sum_fold(), &test_treeish(), &2);
    let result = pipeline.run_from_slice(&dom::FUSED, &[1, 2], 0u64);
    assert_eq!(result, r1 + r2);
}

#[test]
fn error_nodes_are_leaves() {
    use crate::domain::shared::fold;

    #[derive(Clone, Debug, PartialEq)]
    enum ResNode { Ok(u64, Vec<u64>), Err(String) }
    type Seed = u64;

    let nodes: Vec<ResNode> = vec![
        ResNode::Ok(10, vec![1, 2]),
        ResNode::Ok(20, vec![3]),
        ResNode::Err("bad".into()),
        ResNode::Ok(30, vec![]),
    ];

    let seeds_from_node = graph::edgy_visit({
        let nodes = nodes.clone();
        move |n: &ResNode, cb: &mut dyn FnMut(&Seed)| {
            if let ResNode::Ok(_, children) = n {
                for &idx in children { cb(&idx); }
            }
        }
    });

    let f = fold::fold(
        |n: &ResNode| match n { ResNode::Ok(v, _) => *v, ResNode::Err(_) => 0 },
        |h: &mut u64, c: &u64| *h += c,
        |h: &u64| *h,
    );

    let nodes_for_grow = nodes.clone();
    let pipeline = SeedPipeline::new(
        move |seed: &Seed| nodes_for_grow[*seed as usize].clone(),
        seeds_from_node, &f,
    );

    assert_eq!(pipeline.run_from_slice(&dom::FUSED, &[0], 0u64), 60);
    assert_eq!(pipeline.run_from_slice(&dom::FUSED, &[2], 0u64), 0);
}

#[test]
fn pipeline_with_funnel() {
    use crate::cata::exec::funnel;

    let pipeline = make_pipeline();
    let expected = dom::FUSED.run(&sum_fold(), &test_treeish(), &0);

    let result = pipeline.run_from_slice(&dom::exec(funnel::Spec::default(2)), &[0], 0u64);
    assert_eq!(result, expected);

    dom::exec(funnel::Spec::default(2)).session(|s| {
        let result = pipeline.run_from_slice(s.inner(), &[0], 0u64);
        assert_eq!(result, expected);
    });
}
