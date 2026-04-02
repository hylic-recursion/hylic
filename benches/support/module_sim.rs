//! Simulated module resolution benchmark.
//!
//! Two implementations of the same problem:
//! - "vanilla": written as a day-to-day Rust developer would, no hylic
//! - "hylic": using SeedGraph + Fold + dom:: executor

use std::collections::HashMap;
use std::sync::Arc;
use hylic::domain::shared as dom;

use hylic::prelude::WorkPool;

use super::work::{busy_work, spin_wait_us};

// ── Shared types ───────────────────────────────────────────

#[derive(Clone)]
pub struct ModuleDef {
    pub name: String,
    pub deps: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ModuleResult {
    pub name: String,
    pub value: u64,
    pub children: Vec<ModuleResult>,
}

pub struct ModuleSimSpec {
    pub name: &'static str,
    pub module_count: usize,
    pub branch_factor: usize,
    pub parse_work: u64,
    pub parse_io_us: u64,
    pub accumulate_work: u64,
}

pub struct PreparedModuleSim {
    pub name: String,
    pub registry: Arc<HashMap<String, ModuleDef>>,
    pub root_name: String,
    pub spec: ModuleSimSpec,
}

/// Build a deterministic module registry as a balanced tree.
pub fn prepare(spec: &ModuleSimSpec) -> PreparedModuleSim {
    let mut registry = HashMap::new();
    let mut names: Vec<String> = Vec::new();

    for i in 0..spec.module_count {
        names.push(format!("mod_{}", i));
    }

    // Assign children breadth-first
    let mut next_child = 1usize;
    for i in 0..spec.module_count {
        let mut deps = Vec::new();
        for _ in 0..spec.branch_factor {
            if next_child >= spec.module_count { break; }
            deps.push(names[next_child].clone());
            next_child += 1;
        }
        registry.insert(names[i].clone(), ModuleDef {
            name: names[i].clone(),
            deps,
        });
    }

    PreparedModuleSim {
        name: spec.name.to_string(),
        registry: Arc::new(registry),
        root_name: names[0].clone(),
        spec: ModuleSimSpec {
            name: spec.name,
            module_count: spec.module_count,
            branch_factor: spec.branch_factor,
            parse_work: spec.parse_work,
            parse_io_us: spec.parse_io_us,
            accumulate_work: spec.accumulate_work,
        },
    }
}

fn simulate_parse(parse_work: u64, parse_io_us: u64) -> u64 {
    spin_wait_us(parse_io_us);
    if parse_work > 0 { busy_work(parse_work) } else { 0 }
}

fn result_value(r: &ModuleResult) -> u64 {
    r.value.wrapping_add(r.children.iter().map(|c| c.value).sum::<u64>())
}

// ── Vanilla implementations (no hylic) ─────────────────────

pub fn vanilla_seq(sim: &PreparedModuleSim) -> u64 {
    fn resolve(
        reg: &HashMap<String, ModuleDef>,
        name: &str,
        parse_work: u64,
        parse_io_us: u64,
        acc_work: u64,
    ) -> ModuleResult {
        let module = &reg[name];
        let value = simulate_parse(parse_work, parse_io_us);
        let children: Vec<ModuleResult> = module.deps.iter()
            .map(|dep| resolve(reg, dep, parse_work, parse_io_us, acc_work))
            .collect();
        let mut total = value;
        for c in &children {
            if acc_work > 0 { total = total.wrapping_add(busy_work(acc_work)); }
            total = total.wrapping_add(c.value);
        }
        ModuleResult { name: name.to_string(), value: total, children }
    }
    let r = resolve(&sim.registry, &sim.root_name,
        sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    result_value(&r)
}

pub fn vanilla_rayon(sim: &PreparedModuleSim) -> u64 {
    use rayon::prelude::*;

    fn resolve(
        reg: &Arc<HashMap<String, ModuleDef>>,
        name: &str,
        parse_work: u64,
        parse_io_us: u64,
        acc_work: u64,
    ) -> ModuleResult {
        let module = &reg[name];
        let value = simulate_parse(parse_work, parse_io_us);
        let children: Vec<ModuleResult> = if module.deps.len() <= 1 {
            module.deps.iter()
                .map(|dep| resolve(reg, dep, parse_work, parse_io_us, acc_work))
                .collect()
        } else {
            module.deps.par_iter()
                .map(|dep| resolve(reg, dep, parse_work, parse_io_us, acc_work))
                .collect()
        };
        let mut total = value;
        for c in &children {
            if acc_work > 0 { total = total.wrapping_add(busy_work(acc_work)); }
            total = total.wrapping_add(c.value);
        }
        ModuleResult { name: name.to_string(), value: total, children }
    }
    let r = resolve(&sim.registry, &sim.root_name,
        sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    result_value(&r)
}

// ── Hylic implementation (uses shared mode dispatch) ───────

pub fn hylic_fold(sim: &PreparedModuleSim) -> dom::Fold<String, u64, u64> {
    let pw = sim.spec.parse_work;
    let pio = sim.spec.parse_io_us;
    let aw = sim.spec.accumulate_work;
    let init = move |_name: &String| -> u64 { simulate_parse(pw, pio) };
    let acc = move |heap: &mut u64, child: &u64| {
        if aw > 0 { *heap = heap.wrapping_add(busy_work(aw)); }
        *heap = heap.wrapping_add(*child);
    };
    let fin = |heap: &u64| -> u64 { *heap };
    dom::fold(init, acc, fin)
}

pub fn hylic_treeish(reg: &Arc<HashMap<String, ModuleDef>>) -> hylic::domain::shared::Treeish<String> {
    let reg = reg.clone();
    dom::treeish_visit(move |name: &String, cb: &mut dyn FnMut(&String)| {
        for dep in &reg[name].deps { cb(dep); }
    })
}

// ── Scenario definitions ───────────────────────────────────

pub fn all_module_scenarios(large: bool) -> Vec<ModuleSimSpec> {
    let (sm, lg) = if large { (200, 1000) } else { (50, 200) };
    vec![
        ModuleSimSpec { name: "small-sparse/fast",  module_count: sm, branch_factor: 3,  parse_work: 10_000,  parse_io_us: 0,   accumulate_work: 1_000 },
        ModuleSimSpec { name: "small-dense/fast",   module_count: sm, branch_factor: 8,  parse_work: 10_000,  parse_io_us: 0,   accumulate_work: 1_000 },
        ModuleSimSpec { name: "small-sparse/slow",  module_count: sm, branch_factor: 3,  parse_work: 200_000, parse_io_us: 100, accumulate_work: 5_000 },
        ModuleSimSpec { name: "small-dense/slow",   module_count: sm, branch_factor: 8,  parse_work: 200_000, parse_io_us: 100, accumulate_work: 5_000 },
        ModuleSimSpec { name: "large-sparse/fast",  module_count: lg, branch_factor: 3,  parse_work: 10_000,  parse_io_us: 0,   accumulate_work: 1_000 },
        ModuleSimSpec { name: "large-dense/fast",   module_count: lg, branch_factor: 8,  parse_work: 10_000,  parse_io_us: 0,   accumulate_work: 1_000 },
        ModuleSimSpec { name: "large-sparse/slow",  module_count: lg, branch_factor: 3,  parse_work: 200_000, parse_io_us: 100, accumulate_work: 5_000 },
        ModuleSimSpec { name: "large-dense/slow",   module_count: lg, branch_factor: 8,  parse_work: 200_000, parse_io_us: 100, accumulate_work: 5_000 },
    ]
}

/// Build all module sim modes: vanilla baselines + hylic Shared modes.
/// Takes a callback because fold/graph are constructed internally.
pub fn with_all_modes<'a, F>(sim: &'a PreparedModuleSim, pool: &'a Arc<WorkPool>, f: F)
where F: FnOnce(&[super::modes::BenchMode<'_, u64>])
{
    use super::config as id;
    use super::modes::BenchMode;
    use hylic::cata::exec::{PoolIn, PoolSpec};
    use hylic::prelude::{ParLazy, ParEager};

    let fold = hylic_fold(sim);
    let graph = hylic_treeish(&sim.registry);
    let root = &sim.root_name;

    let par_lazy_fused  = ParLazy::lift::<hylic::domain::Shared, String, u64, u64>(pool);
    let par_lazy_rayon  = ParLazy::lift::<hylic::domain::Shared, String, u64, u64>(pool);
    let par_lazy_pool   = ParLazy::lift::<hylic::domain::Shared, String, u64, u64>(pool);
    let par_eager_fused = ParEager::lift::<hylic::domain::Shared, String, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(3));
    let par_eager_rayon = ParEager::lift::<hylic::domain::Shared, String, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(3));
    let par_eager_pool  = ParEager::lift::<hylic::domain::Shared, String, u64, u64>(pool, hylic::prelude::EagerSpec::default_for(3));

    let pool_exec  = PoolIn::<hylic::domain::Shared>::new(pool, PoolSpec::default_for(3));
    let pool_exec2 = PoolIn::<hylic::domain::Shared>::new(pool, PoolSpec::default_for(3));
    let pool_exec3 = PoolIn::<hylic::domain::Shared>::new(pool, PoolSpec::default_for(3));

    let modes: Vec<BenchMode<u64>> = vec![
        // ── baselines ─────────────────────────────────
        BenchMode { name: id::VANILLA_SEQ,          run: Box::new(|| vanilla_seq(sim)) },
        BenchMode { name: id::VANILLA_RAYON,         run: Box::new(|| vanilla_rayon(sim)) },

        // ── hylic direct executors ────────────────────
        BenchMode { name: id::FUSED_SHARED,          run: Box::new(|| dom::FUSED.run(&fold, &graph, root)) },
        BenchMode { name: id::RAYON_SHARED,          run: Box::new(|| dom::RAYON.run(&fold, &graph, root)) },
        BenchMode { name: id::POOL_SHARED,           run: Box::new(|| pool_exec.run(&fold, &graph, root)) },

        // ── hylic ParLazy lift ────────────────────────
        BenchMode { name: id::PARREF_FUSED_SHARED,   run: Box::new(|| dom::FUSED.run_lifted(&par_lazy_fused, &fold, &graph, root)) },
        BenchMode { name: id::PARREF_RAYON_SHARED,   run: Box::new(|| dom::RAYON.run_lifted(&par_lazy_rayon, &fold, &graph, root)) },
        BenchMode { name: id::PARREF_POOL_SHARED,    run: Box::new(|| pool_exec2.run_lifted(&par_lazy_pool, &fold, &graph, root)) },

        // ── hylic ParEager lift ───────────────────────
        BenchMode { name: id::EAGER_FUSED_SHARED,    run: Box::new(|| dom::FUSED.run_lifted(&par_eager_fused, &fold, &graph, root)) },
        BenchMode { name: id::EAGER_RAYON_SHARED,    run: Box::new(|| dom::RAYON.run_lifted(&par_eager_rayon, &fold, &graph, root)) },
        BenchMode { name: id::EAGER_POOL_SHARED,     run: Box::new(|| pool_exec3.run_lifted(&par_eager_pool, &fold, &graph, root)) },
    ];

    f(&modes);
}
