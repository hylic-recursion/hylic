use std::sync::{Arc, OnceLock};

pub struct UIO<T: Send + Sync + 'static> {
    inner: Arc<UIOInner<T>>,
}

struct UIOInner<T: Send + Sync> {
    cell: OnceLock<T>,
    compute: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T: Send + Sync + 'static> UIO<T> {
    pub fn new(f: impl Fn() -> T + Send + Sync + 'static) -> Self {
        UIO { inner: Arc::new(UIOInner {
            cell: OnceLock::new(),
            compute: Box::new(f),
        })}
    }

    pub fn pure(value: T) -> Self {
        let cell = OnceLock::new();
        let _ = cell.set(value);
        UIO { inner: Arc::new(UIOInner {
            cell,
            compute: Box::new(|| unreachable!()),
        })}
    }

    pub fn eval(&self) -> &T {
        self.inner.cell.get_or_init(|| (self.inner.compute)())
    }

    pub fn map<U: Send + Sync + 'static>(
        &self,
        f: impl Fn(&T) -> U + Send + Sync + 'static,
    ) -> UIO<U> {
        let upstream = self.clone();
        UIO::new(move || f(upstream.eval()))
    }

    pub fn flat_map<U: Clone + Send + Sync + 'static>(
        &self,
        f: impl Fn(&T) -> UIO<U> + Send + Sync + 'static,
    ) -> UIO<U> {
        let upstream = self.clone();
        UIO::new(move || f(upstream.eval()).eval().clone())
    }
}

impl<T: Send + Sync + Clone + 'static> UIO<T> {
    pub fn zip_par<U: Send + Sync + Clone + 'static>(
        &self,
        other: &UIO<U>,
    ) -> UIO<(T, U)> {
        let a = self.clone();
        let b = other.clone();
        UIO::new(move || {
            rayon::join(|| a.eval().clone(), || b.eval().clone())
        })
    }

    pub fn join_par(uios: Vec<UIO<T>>) -> UIO<Vec<T>> {
        UIO::new(move || {
            use rayon::prelude::*;
            uios.par_iter().map(|u| u.eval().clone()).collect()
        })
    }
}

impl<T: Send + Sync> Clone for UIO<T> {
    fn clone(&self) -> Self { UIO { inner: self.inner.clone() } }
}

impl<T: Send + Sync + std::fmt::Debug> std::fmt::Debug for UIO<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.inner.cell.get() {
            Some(v) => write!(f, "UIO({:?})", v),
            None => write!(f, "UIO(<pending>)"),
        }
    }
}
