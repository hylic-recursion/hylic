//! Submit mechanism: fire-and-forget task submission with cooperative helping.
//!
//! Two traits define the contract:
//! - [`TaskSubmitter`]: cloneable handle — submit tasks, help process them.
//!   Stored in closures, passed across threads.
//! - [`TaskRunner`]: scoped execution context — produces submitters, helps
//!   from the calling thread. Created per execution, borrowed for the duration.
//!
//! Consumers (hylomorphic executor, ParEager) depend on these traits,
//! not on the concrete pool types. The pool module provides the implementation.

/// Cloneable handle for submitting fire-and-forget tasks and cooperatively
/// helping to process them. Carried across thread boundaries in closures.
pub trait TaskSubmitter: Clone + Send + 'static {
    /// Enqueue a task for eventual execution by a worker or helper.
    fn submit<F: FnOnce() + Send + 'static>(&self, f: F);

    /// Try to steal and execute one task. Returns true if work was done.
    fn help_once(&self) -> bool;
}

/// Scoped execution context: produces [`TaskSubmitter`] handles and
/// helps process tasks from the calling thread.
///
/// Created per fold/execution invocation, borrowed for its duration.
pub trait TaskRunner {
    type Submitter: TaskSubmitter;

    /// Create a submitter handle (cheap — Arc clones).
    fn submitter(&self) -> Self::Submitter;

    /// Try to steal and execute one task. Returns true if work was done.
    fn help_once(&self) -> bool;
}
