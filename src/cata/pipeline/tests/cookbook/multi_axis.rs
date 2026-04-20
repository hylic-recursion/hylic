//! Cookbook: combined Stage-1 + Stage-2 + SeedLift composition in a
//! resolver-style flow.

use std::sync::Arc;
use crate::cata::pipeline::{SeedPipeline, PipelineExecSeed, LiftedSugarsShared};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::domain::Shared;
use crate::graph::edgy_visit;

#[derive(Clone, Debug)]
struct ModSpec { name: String, deps: Vec<String> }

fn registry() -> Arc<std::collections::HashMap<String, ModSpec>> {
    let mut m = std::collections::HashMap::new();
    m.insert("app".into(), ModSpec { name: "app".into(),
        deps: vec!["lib".into(), "util".into()] });
    m.insert("lib".into(), ModSpec { name: "lib".into(), deps: vec!["util".into()] });
    m.insert("util".into(), ModSpec { name: "util".into(), deps: vec![] });
    Arc::new(m)
}

#[test]
fn stage1_reshape_then_stage2_chain() {
    let reg = registry();
    let reg_grow = reg.clone();
    let reg_seeds = reg.clone();

    let base_fold = fold(
        |_n: &ModSpec| 1u32,
        |h: &mut u32, c: &u32| *h += c,
        |h: &u32| *h,
    );
    let seeds = edgy_visit(move |n: &ModSpec, cb: &mut dyn FnMut(&String)| {
        for d in &n.deps { cb(d); }
    });
    let pipeline: SeedPipeline<Shared, ModSpec, String, u32, u32> =
        SeedPipeline::new(
            move |s: &String| reg_grow.get(s).cloned().unwrap(),
            seeds,
            &base_fold,
        );

    let r: (u32, &'static str) = pipeline
        // Stage-1 filter: exclude 'util' dep directly.
        .filter_seeds({
            let reg_seeds = reg_seeds.clone();
            move |s: &String| reg_seeds.get(s).map(|m| m.name != "util").unwrap_or(false)
        })
        // Stage-2 fold wrap: every leaf counts as 10.
        .lift()
        .wrap_init(|_n: &ModSpec, orig: &dyn Fn(&ModSpec) -> u32| orig(_n) * 10)
        // Stage-2 zipmap: classify by result.
        .zipmap(|r: &u32| if *r > 15 { "deep" } else { "shallow" })
        .run_from_slice(&dom::exec(funnel::Spec::default(4)), &["app".to_string()], 0u32);

    // With 'util' filtered out: app → {lib}. lib → {} (util filtered).
    // wrap_init: app=10, lib=10. fold: lib(10) then app(10 + 10) = 20.
    assert_eq!(r.0, 20);
    assert_eq!(r.1, "deep");
}
