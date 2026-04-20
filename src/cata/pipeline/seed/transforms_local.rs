//! Stage-1 Local sugars on SeedPipeline. Mirror of Shared with
//! Rc-based closure storage and non-Send bounds.

use std::rc::Rc;
use crate::domain::Local;
use crate::domain::local::Fold;
use crate::domain::local::edgy::Edgy;
use super::SeedPipeline;

impl<N, Seed, H, R> SeedPipeline<Local, N, Seed, H, R>
where N: Clone + 'static, Seed: Clone + 'static,
      H: Clone + 'static, R: Clone + 'static,
{
    pub fn filter_seeds_local<P>(self, pred: P) -> SeedPipeline<Local, N, Seed, H, R>
    where P: Fn(&Seed) -> bool + 'static,
    {
        let pred = Rc::new(pred);
        self.reshape(
            |grow| grow,
            move |seeds: Edgy<N, Seed>| seeds.filter(move |s: &Seed| pred(s)),
            |fold| fold,
        )
    }

    pub fn wrap_grow_local<W>(self, wrapper: W) -> SeedPipeline<Local, N, Seed, H, R>
    where W: Fn(&Seed, &dyn Fn(&Seed) -> N) -> N + 'static,
    {
        let wrapper = Rc::new(wrapper);
        self.reshape(
            move |grow: Rc<dyn Fn(&Seed) -> N>| -> Rc<dyn Fn(&Seed) -> N>
            {
                let w = wrapper.clone();
                let orig = grow.clone();
                Rc::new(move |s: &Seed| w(s, &|s: &Seed| orig(s)))
            },
            |seeds| seeds,
            |fold| fold,
        )
    }

    pub fn contramap_node_local<N2, Co, Contra>(
        self, co: Co, contra: Contra,
    ) -> SeedPipeline<Local, N2, Seed, H, R>
    where N2: Clone + 'static,
          Co:     Fn(&N) -> N2 + 'static,
          Contra: Fn(&N2) -> N + 'static,
    {
        let co = Rc::new(co);
        let contra = Rc::new(contra);
        let co_for_grow = co.clone();
        let contra_for_seeds = contra.clone();
        let contra_for_fold = contra.clone();
        self.reshape(
            move |grow: Rc<dyn Fn(&Seed) -> N>| -> Rc<dyn Fn(&Seed) -> N2>
            {
                let co = co_for_grow;
                Rc::new(move |s: &Seed| co(&grow(s)))
            },
            move |seeds: Edgy<N, Seed>| -> Edgy<N2, Seed> {
                let contra = contra_for_seeds;
                seeds.contramap(move |n2: &N2| contra(n2))
            },
            move |fold: Fold<N, H, R>| -> Fold<N2, H, R> {
                let contra = contra_for_fold;
                fold.contramap(move |n2: &N2| contra(n2))
            },
        )
    }

    pub fn map_seed_local<Seed2, ToNew, FromNew>(
        self, to_new: ToNew, from_new: FromNew,
    ) -> SeedPipeline<Local, N, Seed2, H, R>
    where Seed2: Clone + 'static,
          ToNew:   Fn(&Seed) -> Seed2 + 'static,
          FromNew: Fn(&Seed2) -> Seed + 'static,
    {
        let to_new = Rc::new(to_new);
        let from_new = Rc::new(from_new);
        let from_for_grow = from_new.clone();
        self.reshape(
            move |grow: Rc<dyn Fn(&Seed) -> N>| -> Rc<dyn Fn(&Seed2) -> N>
            {
                let from_new = from_for_grow;
                Rc::new(move |s2: &Seed2| grow(&from_new(s2)))
            },
            move |seeds: Edgy<N, Seed>| -> Edgy<N, Seed2> {
                let to_new = to_new;
                seeds.map(move |s: &Seed| to_new(s))
            },
            |fold| fold,
        )
    }
}
