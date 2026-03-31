//! Simulated module resolution benchmark.
//!
//! Two implementations of the same problem:
//! - "vanilla": written as a day-to-day Rust developer would, no hylic
//! - "hylic": using SeedGraph + Fold + Exec

use std::collections::HashMap;
use std::sync::Arc;
use hylic::graph::treeish;
use hylic::fold;
use hylic::cata::Exec;
use hylic::prelude::{ParLazy, ParEager, WorkPool};

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

// ── Hylic implementations ──────────────────────────────────

fn hylic_fold(parse_work: u64, parse_io_us: u64, acc_work: u64) -> fold::Fold<String, u64, u64> {
    fold::fold(
        move |_name: &String| -> u64 { simulate_parse(parse_work, parse_io_us) },
        move |heap: &mut u64, child: &u64| {
            if acc_work > 0 { *heap = heap.wrapping_add(busy_work(acc_work)); }
            *heap = heap.wrapping_add(*child);
        },
        |heap: &u64| -> u64 { *heap },
    )
}

fn hylic_treeish(reg: &Arc<HashMap<String, ModuleDef>>) -> hylic::graph::Treeish<String> {
    let reg = reg.clone();
    treeish(move |name: &String| {
        reg[name].deps.clone()
    })
}

pub fn hylic_fused(sim: &PreparedModuleSim) -> u64 {
    let fold = hylic_fold(sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    let graph = hylic_treeish(&sim.registry);
    Exec::fused().run(&fold, &graph, &sim.root_name)
}

pub fn hylic_rayon(sim: &PreparedModuleSim) -> u64 {
    let fold = hylic_fold(sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    let graph = hylic_treeish(&sim.registry);
    Exec::rayon().run(&fold, &graph, &sim.root_name)
}

pub fn hylic_parref_rayon(sim: &PreparedModuleSim) -> u64 {
    let fold = hylic_fold(sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    let graph = hylic_treeish(&sim.registry);
    Exec::rayon().run_lifted(&ParLazy::lift(), &fold, &graph, &sim.root_name)
}

pub fn hylic_eager(sim: &PreparedModuleSim, pool: &Arc<WorkPool>) -> u64 {
    let fold = hylic_fold(sim.spec.parse_work, sim.spec.parse_io_us, sim.spec.accumulate_work);
    let graph = hylic_treeish(&sim.registry);
    Exec::fused().run_lifted(&ParEager::lift(pool), &fold, &graph, &sim.root_name)
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

pub const MODULE_MODES: [&str; 6] = [
    "vanilla-seq", "vanilla-rayon", "hylic-fused", "hylic-rayon", "hylic-parref", "hylic-eager",
];

pub fn run_module_mode(name: &str, sim: &PreparedModuleSim, pool: &Arc<WorkPool>) -> u64 {
    match name {
        "vanilla-seq"   => vanilla_seq(sim),
        "vanilla-rayon" => vanilla_rayon(sim),
        "hylic-fused"   => hylic_fused(sim),
        "hylic-rayon"   => hylic_rayon(sim),
        "hylic-parref"  => hylic_parref_rayon(sim),
        "hylic-eager"   => hylic_eager(sim, pool),
        _ => panic!("unknown module mode: {name}"),
    }
}
