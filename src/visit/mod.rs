use std::marker::PhantomData;

/// Push-based iterator: produces elements by callback.
/// Generic over the closure type — zero allocation, fully inlineable.
///
/// Created by `Edgy::at(node)`, composed via `map`/`filter`/`flat_map`,
/// consumed by `for_each` or `collect_vec`.
pub struct Visit<T, F: FnMut(&mut dyn FnMut(&T))> {
    run: F,
    _phantom: PhantomData<fn(&T)>,
}

impl<T, F: FnMut(&mut dyn FnMut(&T))> Visit<T, F> {
    pub fn new(run: F) -> Self {
        Visit { run, _phantom: PhantomData }
    }

    pub fn for_each(mut self, cb: &mut dyn FnMut(&T)) {
        (self.run)(cb);
    }

    pub fn map<U>(mut self, f: impl Fn(&T) -> U) -> Visit<U, impl FnMut(&mut dyn FnMut(&U))> {
        Visit::new(move |cb: &mut dyn FnMut(&U)| {
            (self.run)(&mut |t: &T| {
                let u = f(t);
                cb(&u);
            });
        })
    }

    pub fn filter(mut self, pred: impl Fn(&T) -> bool) -> Visit<T, impl FnMut(&mut dyn FnMut(&T))> {
        Visit::new(move |cb: &mut dyn FnMut(&T)| {
            (self.run)(&mut |t: &T| {
                if pred(t) { cb(t); }
            });
        })
    }

    pub fn flat_visit<U, G: FnMut(&mut dyn FnMut(&U))>(
        mut self, f: impl Fn(&T) -> Visit<U, G>,
    ) -> Visit<U, impl FnMut(&mut dyn FnMut(&U))> {
        Visit::new(move |cb: &mut dyn FnMut(&U)| {
            (self.run)(&mut |t: &T| {
                f(t).for_each(cb);
            });
        })
    }

    pub fn collect_vec(self) -> Vec<T> where T: Clone {
        let mut v = Vec::new();
        self.for_each(&mut |t| v.push(t.clone()));
        v
    }

    pub fn fold<A>(self, init: A, mut acc: impl FnMut(A, &T) -> A) -> A {
        let mut state = Some(init);
        self.for_each(&mut |t| {
            state = Some(acc(state.take().unwrap(), t));
        });
        state.unwrap()
    }

    pub fn count(self) -> usize {
        self.fold(0, |n, _| n + 1)
    }
}

/// Convenience: create a Visit from a slice.
pub fn visit_slice<'a, T>(items: &'a [T]) -> Visit<T, impl FnMut(&mut dyn FnMut(&T)) + 'a> {
    Visit::new(move |cb: &mut dyn FnMut(&T)| {
        for item in items { cb(item); }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visit_for_each() {
        let data = vec![1, 2, 3];
        let mut sum = 0;
        visit_slice(&data).for_each(&mut |x| sum += x);
        assert_eq!(sum, 6);
    }

    #[test]
    fn visit_map() {
        let data = vec![1, 2, 3];
        let result = visit_slice(&data).map(|x| x * 10).collect_vec();
        assert_eq!(result, vec![10, 20, 30]);
    }

    #[test]
    fn visit_filter() {
        let data = vec![1, 2, 3, 4, 5];
        let result = visit_slice(&data).filter(|x| *x % 2 == 0).collect_vec();
        assert_eq!(result, vec![2, 4]);
    }

    #[test]
    fn visit_chain() {
        let data = vec![1, 2, 3, 4, 5];
        let result = visit_slice(&data)
            .map(|x| x * 2)
            .filter(|x| *x > 4)
            .collect_vec();
        assert_eq!(result, vec![6, 8, 10]);
    }

    #[test]
    fn visit_fold() {
        let data = vec![1, 2, 3, 4];
        assert_eq!(visit_slice(&data).fold(0, |acc, x| acc + x), 10);
    }

    #[test]
    fn visit_count() {
        let data = vec![1, 2, 3];
        assert_eq!(visit_slice(&data).count(), 3);
    }
}
