//! InlineLift — closure-constructed N-changing lift for the
//! H/R-preserving case (`MapH = H, MapR = R`).
//!
//! Purpose: let users express a context-dependent N-change
//! (depth-annotator, visit-tag, ...) without writing a named
//! struct + `impl Lift<N, H, R>`. The three closure parameters
//! are the witness and the coordinating transforms:
//!
//! - `lift_node`:     `&N → N2` — composed with grow to produce
//!                    `grow_new: Seed → N2`. This is the otherwise
//!                    hidden N-change witness, given a name here.
//! - `build_treeish`: `&Treeish<N> → Treeish<N2>` — lets the lift
//!                    build its new treeish using full access to
//!                    the original. Context-dependent lifts walk
//!                    the input here.
//! - `fold_contra`:   `&N2 → N` — backward bijection used to
//!                    contramap the fold to the new node type.
//!
//! ## Invertibility constraint
//!
//! `fold_contra: &N2 → N` must **round-trip to the original
//! representation**. The fold operates over `N` internally; every
//! `N2` value reached during traversal is mapped back to `N` via
//! `fold_contra` before the inner fold sees it. If the `N → N2`
//! transformation loses information (e.g. projecting N onto a
//! summary), no closure pack can reconstitute the missing data and
//! `inline_lift` is the wrong tool — write a full
//! `impl Lift<N, H, R>` struct instead, whose `apply` body
//! constructs a fold over `N2` directly without round-tripping.
//!
//! For context-dependent lifts whose `MapH` or `MapR` wraps H/R
//! non-trivially (e.g. annotate each node's visit count into the
//! heap), closures cannot carry the required associated-type
//! projections; the user writes a named struct + `impl Lift<N, H, R>`
//! instead. See `technical-insights/06-rust-encoding-and-bounds.md`.

use std::marker::PhantomData;
use std::sync::Arc;
use crate::domain::shared::fold::Fold;
use crate::graph::Treeish;
use crate::ops::lift::core::Lift;

type LiftNodeFn<N, N2>     = Arc<dyn Fn(&N) -> N2 + Send + Sync>;
type BuildTreeishFn<N, N2> = Arc<dyn Fn(&Treeish<N>) -> Treeish<N2> + Send + Sync>;
type FoldContraFn<N, N2>   = Arc<dyn Fn(&N2) -> N + Send + Sync>;

pub struct InlineLift<N, N2> {
    lift_node:     LiftNodeFn<N, N2>,
    build_treeish: BuildTreeishFn<N, N2>,
    fold_contra:   FoldContraFn<N, N2>,
    _m: PhantomData<fn() -> (N, N2)>,
}

impl<N, N2> Clone for InlineLift<N, N2> {
    fn clone(&self) -> Self {
        InlineLift {
            lift_node:     self.lift_node.clone(),
            build_treeish: self.build_treeish.clone(),
            fold_contra:   self.fold_contra.clone(),
            _m: PhantomData,
        }
    }
}

pub fn inline_lift<N, N2, LN, LT, FC>(
    lift_node:     LN,
    build_treeish: LT,
    fold_contra:   FC,
) -> InlineLift<N, N2>
where
    N: 'static, N2: 'static,
    LN: Fn(&N) -> N2 + Send + Sync + 'static,
    LT: Fn(&Treeish<N>) -> Treeish<N2> + Send + Sync + 'static,
    FC: Fn(&N2) -> N + Send + Sync + 'static,
{
    InlineLift {
        lift_node:     Arc::new(lift_node),
        build_treeish: Arc::new(build_treeish),
        fold_contra:   Arc::new(fold_contra),
        _m: PhantomData,
    }
}

impl<N, H, R, N2> Lift<N, H, R> for InlineLift<N, N2>
where N: Clone + 'static, H: Clone + 'static, R: Clone + 'static,
      N2: Clone + 'static,
{
    type N2   = N2;
    type MapH = H;
    type MapR = R;

    fn apply<Seed, T>(
        &self,
        grow:    Arc<dyn Fn(&Seed) -> N + Send + Sync>,
        treeish: Treeish<N>,
        fold:    Fold<N, H, R>,
        cont: impl FnOnce(
            Arc<dyn Fn(&Seed) -> N2 + Send + Sync>,
            Treeish<N2>,
            Fold<N2, H, R>,
        ) -> T,
    ) -> T
    where Seed: Clone + 'static,
    {
        let ln = self.lift_node.clone();
        let bt = self.build_treeish.clone();
        let fc = self.fold_contra.clone();

        let grow_new: Arc<dyn Fn(&Seed) -> N2 + Send + Sync> = {
            let ln = ln.clone();
            Arc::new(move |s: &Seed| ln(&grow(s)))
        };
        let treeish_new: Treeish<N2> = bt(&treeish);
        let fold_new: Fold<N2, H, R> = {
            let fc = fc.clone();
            fold.contramap(move |n2: &N2| fc(n2))
        };

        cont(grow_new, treeish_new, fold_new)
    }
}
