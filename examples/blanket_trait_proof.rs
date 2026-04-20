//! Proof of concept: blanket sugar trait with trait type params
//! (not Self:: projections) — the pattern that sidesteps Rust's
//! projection-normalisation wall.
//!
//! This example was the de-risk sandbox for the real library's
//! `LiftedSugarsShared` trait. The miniature here replicates the
//! essential structure:
//!
//!   - `Lift<D, N, H, R>` with associated outputs `N2, MapH, MapR`
//!   - `IdentityLift`, `ComposedLift<L1, L2>`, `WrapInitLift`,
//!     `MapRbiLift` (ShapeLift stand-ins)
//!   - `TreeishSource` with `Domain, N, H, R` associated types
//!   - `SeedPipeline<D, N, H, R>` (Stage 1) and
//!     `LiftedPipeline<Base, L>` (Stage 2)
//!
//! The crux is the `LiftedSugars<N, H, R>` trait at the bottom —
//! `N, H, R` are trait type parameters, NOT `Self::N` /
//! `Self::H` / `Self::R`. Default-method bodies use those params
//! directly. Two `impl`s:
//!
//!   1. `SeedPipeline<Shared, N, H, R>` — auto-lifts first, then
//!      composes via `IdentityLift`.
//!   2. `LiftedPipeline<Base, L>` — composes at the tip using
//!      `L::N2 / L::MapH / L::MapR` as the trait's N/H/R.
//!
//! Reason the earlier `Self::`-based shape failed: Rust would not
//! unify trait-level `Self::N` with the impl's concrete `N` inside
//! default-method bodies — projection-normalisation refused.
//! Moving `N, H, R` to trait type parameters avoids projection
//! entirely; the impl constrains the implementor's `TreeishSource::N`
//! to equal the trait param `N` via `where Self: TreeishSource<N = N>`,
//! and the bodies just use `N` directly.
//!
//! Run as: `cargo run --example blanket_trait_proof`

use std::marker::PhantomData;

// ── Domain marker ─────────────────────────────────────────────

pub struct Shared;

// ── Lift trait + atoms ────────────────────────────────────────

pub trait Lift<D, N, H, R> {
    type N2;
    type MapH;
    type MapR;
    fn name(&self) -> &'static str;
}

#[derive(Clone, Copy)]
pub struct IdentityLift;

impl<D, N, H, R> Lift<D, N, H, R> for IdentityLift {
    type N2 = N;
    type MapH = H;
    type MapR = R;
    fn name(&self) -> &'static str { "Identity" }
}

#[derive(Clone, Copy)]
pub struct ComposedLift<L1, L2> { #[allow(dead_code)] inner: L1, #[allow(dead_code)] outer: L2 }

impl<L1, L2> ComposedLift<L1, L2> {
    pub fn compose(inner: L1, outer: L2) -> Self { ComposedLift { inner, outer } }
}

impl<D, N, H, R, L1, L2> Lift<D, N, H, R> for ComposedLift<L1, L2>
where
    L1: Lift<D, N, H, R>,
    L2: Lift<D, L1::N2, L1::MapH, L1::MapR>,
{
    type N2 = L2::N2;
    type MapH = L2::MapH;
    type MapR = L2::MapR;
    fn name(&self) -> &'static str { "Composed" }
}

// ── Stand-in ShapeLifts (real library has the full CPS closure pack) ──

pub struct WrapInitLift<N, H, R>(PhantomData<(N, H, R)>);
impl<N, H, R> Clone for WrapInitLift<N, H, R> { fn clone(&self) -> Self { WrapInitLift(PhantomData) } }
impl<N, H, R> Copy for WrapInitLift<N, H, R> {}

impl<D, N, H, R> Lift<D, N, H, R> for WrapInitLift<N, H, R> {
    type N2 = N; type MapH = H; type MapR = R;
    fn name(&self) -> &'static str { "WrapInit" }
}

pub fn wrap_init_lift<N, H, R>() -> WrapInitLift<N, H, R> { WrapInitLift(PhantomData) }

pub struct MapRbiLift<N, H, R, RNew>(PhantomData<(N, H, R, RNew)>);
impl<N, H, R, RNew> Clone for MapRbiLift<N, H, R, RNew> { fn clone(&self) -> Self { MapRbiLift(PhantomData) } }
impl<N, H, R, RNew> Copy for MapRbiLift<N, H, R, RNew> {}

impl<D, N, H, R, RNew> Lift<D, N, H, R> for MapRbiLift<N, H, R, RNew> {
    type N2 = N; type MapH = H; type MapR = RNew;
    fn name(&self) -> &'static str { "MapRbi" }
}

pub fn map_r_bi_lift<N, H, R, RNew>() -> MapRbiLift<N, H, R, RNew> { MapRbiLift(PhantomData) }

// ── TreeishSource (stand-in for hylic's real trait) ───────────

pub trait TreeishSource {
    type Domain;
    type N;
    type H;
    type R;
    fn describe(&self) -> &'static str;
}

// ── Stage-1: SeedPipeline ─────────────────────────────────────

pub struct SeedPipeline<D, N, H, R>(PhantomData<(D, N, H, R)>);

impl<N, H, R> SeedPipeline<Shared, N, H, R> {
    pub fn new() -> Self { SeedPipeline(PhantomData) }
    pub fn lift(self) -> LiftedPipeline<Self, IdentityLift> {
        LiftedPipeline { base: self, pre_lift: IdentityLift }
    }
}

impl<N, H, R> TreeishSource for SeedPipeline<Shared, N, H, R> {
    type Domain = Shared; type N = N; type H = H; type R = R;
    fn describe(&self) -> &'static str { "SeedPipeline" }
}

// ── Stage-2: LiftedPipeline ───────────────────────────────────

pub struct LiftedPipeline<Base, L> { #[allow(dead_code)] base: Base, pre_lift: L }

impl<Base, L> LiftedPipeline<Base, L>
where Base: TreeishSource<Domain = Shared>,
      L: Lift<Shared, Base::N, Base::H, Base::R>,
{
    pub fn then_lift_raw<L2>(self, outer: L2) -> LiftedPipeline<Base, ComposedLift<L, L2>>
    where L2: Lift<Shared, L::N2, L::MapH, L::MapR>,
    { LiftedPipeline { base: self.base, pre_lift: ComposedLift::compose(self.pre_lift, outer) } }

    pub fn describe_chain(&self) -> &'static str { self.pre_lift.name() }
}

impl<Base, L> TreeishSource for LiftedPipeline<Base, L>
where Base: TreeishSource<Domain = Shared>,
      L: Lift<Shared, Base::N, Base::H, Base::R>,
{
    type Domain = Shared; type N = L::N2; type H = L::MapH; type R = L::MapR;
    fn describe(&self) -> &'static str { "LiftedPipeline" }
}

// ── THE EXPERIMENT: blanket sugar trait with trait type params ──

pub trait LiftedSugars<N, H, R>:
    TreeishSource<Domain = Shared, N = N, H = H, R = R> + Sized
{
    type With<L2>
    where L2: Lift<Shared, N, H, R>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, N, H, R>;

    fn wrap_init(self) -> Self::With<WrapInitLift<N, H, R>> {
        self.then_lift(wrap_init_lift::<N, H, R>())
    }

    fn map_r_bi<RNew>(self) -> Self::With<MapRbiLift<N, H, R, RNew>> {
        self.then_lift(map_r_bi_lift::<N, H, R, RNew>())
    }
}

impl<N, H, R> LiftedSugars<N, H, R> for SeedPipeline<Shared, N, H, R>
where N: 'static, H: 'static, R: 'static,
{
    type With<L2> = LiftedPipeline<Self, ComposedLift<IdentityLift, L2>>
    where L2: Lift<Shared, N, H, R>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, N, H, R>,
    { self.lift().then_lift_raw(l) }
}

impl<Base, L> LiftedSugars<L::N2, L::MapH, L::MapR> for LiftedPipeline<Base, L>
where Base: TreeishSource<Domain = Shared>,
      L: Lift<Shared, Base::N, Base::H, Base::R>,
      L::N2: 'static, L::MapH: 'static, L::MapR: 'static,
{
    type With<L2> = LiftedPipeline<Base, ComposedLift<L, L2>>
    where L2: Lift<Shared, L::N2, L::MapH, L::MapR>;

    fn then_lift<L2>(self, l: L2) -> Self::With<L2>
    where L2: Lift<Shared, L::N2, L::MapH, L::MapR>,
    { self.then_lift_raw(l) }
}

// ── Verification ─────────────────────────────────────────────

fn main() {
    // (1) SeedPipeline::wrap_init() — no .lift() ceremony required,
    //     the trait auto-lifts.
    {
        let p: SeedPipeline<Shared, u32, u64, u64> = SeedPipeline::new();
        let out = p.wrap_init();
        assert_eq!(out.describe(), "LiftedPipeline");
        assert_eq!(out.describe_chain(), "Composed");
        println!("(1) SeedPipeline.wrap_init()  OK");
    }

    // (2) LiftedPipeline::wrap_init() — composes at the tip.
    {
        let p: SeedPipeline<Shared, u32, u64, u64> = SeedPipeline::new();
        let lp = p.lift();
        let out = lp.wrap_init();
        assert_eq!(out.describe(), "LiftedPipeline");
        println!("(2) LiftedPipeline.wrap_init() OK");
    }

    // (3) Chain across stages: .wrap_init() auto-lifts,
    //     subsequent .wrap_init() and .map_r_bi::<String>() compose.
    {
        let p: SeedPipeline<Shared, u32, u64, u64> = SeedPipeline::new();
        let out = p
            .wrap_init()
            .wrap_init()
            .map_r_bi::<String>();
        assert_eq!(out.describe(), "LiftedPipeline");
        println!("(3) chaining across stages    OK");
    }

    // (4) Type change actually propagates through the trait's With<L2>:
    //     require_r_is_string accepts only types whose TreeishSource::R
    //     is String.
    {
        let p: SeedPipeline<Shared, u32, u64, u64> = SeedPipeline::new();
        let out = p.map_r_bi::<String>();
        fn require_r_is_string<T: TreeishSource<R = String>>(_: &T) {}
        require_r_is_string(&out);
        println!("(4) map_r_bi::<String> changes tip R = String  OK");
    }

    println!("\nAll 4 checks passed — blanket-trait pattern works.");
}
