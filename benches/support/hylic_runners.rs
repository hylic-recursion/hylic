use std::sync::Arc;
use hylic::cata::Exec;
use hylic::prelude::{ParLazy, ParEager, WorkPool};

use super::scenario::PreparedScenario;

pub struct HylicMode {
    pub name: &'static str,
    pool: Option<Arc<WorkPool>>,
}

impl HylicMode {
    fn run_inner(&self, s: &PreparedScenario) -> u64 {
        match self.name {
            "hylic-fused"       => Exec::fused().run(&s.fold, &s.treeish, &s.root),
            "hylic-rayon"       => Exec::rayon().run(&s.fold, &s.treeish, &s.root),
            "hylic-parref+fused" => Exec::fused().run_lifted(&ParLazy::lift(), &s.fold, &s.treeish, &s.root),
            "hylic-parref+rayon" => Exec::rayon().run_lifted(&ParLazy::lift(), &s.fold, &s.treeish, &s.root),
            "hylic-eager+fused" => {
                let pool = self.pool.as_ref().unwrap();
                Exec::fused().run_lifted(&ParEager::lift(pool), &s.fold, &s.treeish, &s.root)
            }
            "hylic-eager+rayon" => {
                let pool = self.pool.as_ref().unwrap();
                Exec::rayon().run_lifted(&ParEager::lift(pool), &s.fold, &s.treeish, &s.root)
            }
            _ => panic!("unknown mode: {}", self.name),
        }
    }
}

/// Create all hylic modes. The pool is created once and shared.
pub fn all_modes(pool: &Arc<WorkPool>) -> Vec<HylicMode> {
    vec![
        HylicMode { name: "hylic-fused",        pool: None },
        HylicMode { name: "hylic-rayon",         pool: None },
        HylicMode { name: "hylic-parref+fused",  pool: None },
        HylicMode { name: "hylic-parref+rayon",  pool: None },
        HylicMode { name: "hylic-eager+fused",   pool: Some(pool.clone()) },
        HylicMode { name: "hylic-eager+rayon",    pool: Some(pool.clone()) },
    ]
}

pub fn run(mode: &HylicMode, s: &PreparedScenario) -> u64 {
    mode.run_inner(s)
}
