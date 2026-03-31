//! Simulated module resolution benchmark.
//!
//! Two implementations of the same problem:
//! - "vanilla": written as a day-to-day Rust developer would, no hylic
//! - "hylic": using SeedGraph + Fold + Exec

use std::collections::HashMap;
use std::sync::Arc;
use hylic::graph::treeish_visit;
use hylic::fold;
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

pub fn hylic_fold(sim: &PreparedModuleSim) -> fold::Fold<String, u64, u64> {
    let pw = sim.spec.parse_work;
    let pio = sim.spec.parse_io_us;
    let aw = sim.spec.accumulate_work;
    fold::fold(
        move |_name: &String| -> u64 { simulate_parse(pw, pio) },
        move |heap: &mut u64, child: &u64| {
            if aw > 0 { *heap = heap.wrapping_add(busy_work(aw)); }
            *heap = heap.wrapping_add(*child);
        },
        |heap: &u64| -> u64 { *heap },
    )
}

pub fn hylic_treeish(reg: &Arc<HashMap<String, ModuleDef>>) -> hylic::graph::Treeish<String> {
    let reg = reg.clone();
    treeish_visit(move |name: &String, cb: &mut dyn FnMut(&String)| {
        for dep in &reg[name].deps { cb(dep); }
    })
}

pub fn run_hylic(mode: &str, sim: &PreparedModuleSim, pool: &Arc<WorkPool>) -> u64 {
    let fold = hylic_fold(sim);
    let graph = hylic_treeish(&sim.registry);
    super::hylic_runners::run_hylic_mode(mode, &fold, &graph, &sim.root_name, pool)
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

pub const VANILLA_MODES: [&str; 2] = ["vanilla-seq", "vanilla-rayon"];

pub fn run_module_mode(name: &str, sim: &PreparedModuleSim, pool: &Arc<WorkPool>) -> u64 {
    match name {
        "vanilla-seq"   => vanilla_seq(sim),
        "vanilla-rayon" => vanilla_rayon(sim),
        // All hylic-* modes dispatched through the shared runner
        _ if name.starts_with("hylic-") => run_hylic(name, sim, pool),
        _ => panic!("unknown module mode: {name}"),
    }
}

/// All module sim modes: vanilla baselines + all 6 hylic modes.
pub fn all_modes() -> Vec<&'static str> {
    let mut v: Vec<&str> = VANILLA_MODES.to_vec();
    v.extend_from_slice(&super::hylic_runners::HYLIC_MODES);
    v
}
