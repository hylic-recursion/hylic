//! Cookbook: graph-side transforms.
//!
//! Demonstrates filter_edges, wrap_visit, contramap_node at Stage-2
//! against a Module dependency graph.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::cata::pipeline::{TreeishPipeline, PipelineExec};
use crate::domain::shared::{self as dom, fold::fold};
use crate::cata::exec::funnel;
use crate::graph::treeish;
use crate::domain::Shared;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ModuleId(String);

fn registry() -> Arc<HashMap<ModuleId, Vec<ModuleId>>> {
    let mut m: HashMap<ModuleId, Vec<ModuleId>> = HashMap::new();
    m.insert(ModuleId("app".into()),  vec![ModuleId("db".into()), ModuleId("heavy".into())]);
    m.insert(ModuleId("db".into()),   vec![]);
    m.insert(ModuleId("heavy".into()), vec![ModuleId("db".into())]);
    Arc::new(m)
}

fn base_pipeline() -> TreeishPipeline<Shared, ModuleId, u32, u32> {
    let reg = registry();
    let t = treeish(move |n: &ModuleId| reg.get(n).cloned().unwrap_or_default());
    let f = fold(
        |_n: &ModuleId| 1u32,
        |h: &mut u32, c: &u32| *h += *c,
        |h: &u32| *h,
    );
    TreeishPipeline::new(t, &f)
}

#[test]
fn filter_edges_excludes_heavy_deps() {
    let root = ModuleId("app".into());
    let full: u32 = base_pipeline()
        .lift()
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);
    assert_eq!(full, 4); // app + db + heavy + db-under-heavy = 4

    let filtered: u32 = base_pipeline()
        .lift()
        .filter_edges(|m: &ModuleId| m.0 != "heavy")
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);
    assert_eq!(filtered, 2); // app + db
}

#[test]
fn wrap_visit_counts_edges_explored() {
    let visits: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let visits_for_closure = visits.clone();

    let root = ModuleId("app".into());
    let _r: u32 = base_pipeline()
        .lift()
        .wrap_visit(move |n: &ModuleId, cb: &mut dyn FnMut(&ModuleId),
                          orig: &dyn Fn(&ModuleId, &mut dyn FnMut(&ModuleId))| {
            let mut local = 0u32;
            orig(n, &mut |c: &ModuleId| { local += 1; cb(c); });
            *visits_for_closure.lock().unwrap() += local;
        })
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);

    // Edges visited: app→{db, heavy} (2) + heavy→{db} (1) + db→{} + db→{} = 3
    assert_eq!(*visits.lock().unwrap(), 3);
}

#[test]
fn contramap_node_at_stage2_wraps_in_newtype() {
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct Tagged(ModuleId);

    let root = Tagged(ModuleId("app".into()));
    let r: u32 = base_pipeline()
        .lift()
        .contramap_node(
            |m: &ModuleId| Tagged(m.clone()),
            |t: &Tagged| t.0.clone(),
        )
        .run_from_node(&dom::exec(funnel::Spec::default(4)), &root);
    assert_eq!(r, 4);
}
