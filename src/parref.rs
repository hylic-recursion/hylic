use std::sync::{Arc, Mutex, OnceLock};

pub struct ParRef<T: Send + Sync + 'static> {
    inner: Arc<ParRefInner<T>>,
}

struct ParRefInner<T: Send + Sync> {
    cell: OnceLock<T>,
    compute: Mutex<Option<Box<dyn FnOnce() -> T + Send + Sync>>>,
}

impl<T: Send + Sync + 'static> ParRef<T> {
    pub fn new(f: impl FnOnce() -> T + Send + Sync + 'static) -> Self {
        ParRef { inner: Arc::new(ParRefInner {
            cell: OnceLock::new(),
            compute: Mutex::new(Some(Box::new(f))),
        })}
    }

    pub fn pure(value: T) -> Self {
        let cell = OnceLock::new();
        let _ = cell.set(value);
        ParRef { inner: Arc::new(ParRefInner {
            cell,
            compute: Mutex::new(None),
        })}
    }

    pub fn eval(&self) -> &T {
        self.inner.cell.get_or_init(|| {
            self.inner.compute.lock().unwrap().take()
                .expect("ParRef: compute already consumed but OnceLock empty")()
        })
    }

    pub fn map<U: Send + Sync + 'static>(
        &self,
        f: impl Fn(&T) -> U + Send + Sync + 'static,
    ) -> ParRef<U> {
        let upstream = self.clone();
        ParRef::new(move || f(upstream.eval()))
    }

    pub fn flat_map<U: Clone + Send + Sync + 'static>(
        &self,
        f: impl Fn(&T) -> ParRef<U> + Send + Sync + 'static,
    ) -> ParRef<U> {
        let upstream = self.clone();
        ParRef::new(move || f(upstream.eval()).eval().clone())
    }
}

impl<T: Send + Sync + Clone + 'static> ParRef<T> {
    pub fn zip_par<U: Send + Sync + Clone + 'static>(
        &self,
        other: &ParRef<U>,
    ) -> ParRef<(T, U)> {
        let a = self.clone();
        let b = other.clone();
        ParRef::new(move || {
            rayon::join(|| a.eval().clone(), || b.eval().clone())
        })
    }

    pub fn join_par(parrefs: Vec<ParRef<T>>) -> ParRef<Vec<T>> {
        ParRef::new(move || {
            use rayon::prelude::*;
            parrefs.par_iter().map(|u| u.eval().clone()).collect()
        })
    }
}

impl<T: Send + Sync> Clone for ParRef<T> {
    fn clone(&self) -> Self { ParRef { inner: self.inner.clone() } }
}

impl<T: Send + Sync + std::fmt::Debug> std::fmt::Debug for ParRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.inner.cell.get() {
            Some(v) => write!(f, "ParRef({:?})", v),
            None => write!(f, "ParRef(<pending>)"),
        }
    }
}
