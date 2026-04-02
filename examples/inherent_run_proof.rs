//! Proof: can FusedIn<D> have an inherent run() with N, H, R on the method?

use std::marker::PhantomData;

trait FoldOps<N, H, R> {
    fn init(&self, node: &N) -> H;
    fn accumulate(&self, heap: &mut H, result: &R);
    fn finalize(&self, heap: &H) -> R;
}

trait TreeOps<N> {
    fn visit(&self, node: &N, cb: &mut dyn FnMut(&N));
}

trait Domain<N: 'static>: 'static {
    type Fold<H: 'static, R: 'static>: FoldOps<N, H, R>;
    type Treeish: TreeOps<N>;
}

struct Shared;
struct SharedFold<N, H, R>(PhantomData<(N, H, R)>);

impl<N, H, R> FoldOps<N, H, R> for SharedFold<N, H, R> {
    fn init(&self, _: &N) -> H { unimplemented!() }
    fn accumulate(&self, _: &mut H, _: &R) { unimplemented!() }
    fn finalize(&self, _: &H) -> R { unimplemented!() }
}

struct SharedTreeish<N>(PhantomData<N>);
impl<N> TreeOps<N> for SharedTreeish<N> {
    fn visit(&self, _: &N, _: &mut dyn FnMut(&N)) {}
}

impl<N: 'static> Domain<N> for Shared {
    type Fold<H: 'static, R: 'static> = SharedFold<N, H, R>;
    type Treeish = SharedTreeish<N>;
}

// THE KEY: inherent run() with N, H, R on the method, D on the impl
struct FusedIn<D>(PhantomData<D>);

impl<D> FusedIn<D> {
    pub fn run<N: 'static, H: 'static, R: 'static>(
        &self,
        fold: &<D as Domain<N>>::Fold<H, R>,
        graph: &<D as Domain<N>>::Treeish,
        root: &N,
    ) -> R
    where D: Domain<N>
    {
        let mut heap = fold.init(root);
        graph.visit(root, &mut |child: &N| {
            let r = self.run(fold, graph, child);
            fold.accumulate(&mut heap, &r);
        });
        fold.finalize(&heap)
    }
}

fn main() {
    let exec = FusedIn::<Shared>(PhantomData);
    let fold = SharedFold::<i32, u64, u64>(PhantomData);
    let graph = SharedTreeish::<i32>(PhantomData);
    // Does this compile? N=i32, H=u64, R=u64 inferred from arguments.
    // No trait import needed — run() is inherent.
    let _ = exec.run(&fold, &graph, &42i32);
    println!("Inherent run() compiles!");
}
