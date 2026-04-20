//! Local-domain additions to the default prelude.
//!
//! The default prelude is Shared-biased (defaults `fold`, `edgy`,
//! `exec` to Shared). When working in Local, you usually want either:
//!
//! 1. **Local-only code.** Use the Local domain module directly:
//!    ```no_run
//!    use hylic::prelude::*;                      // core types + traits
//!    use hylic::prelude::local::{Local, LiftedSugarsLocal};
//!    use hylic::domain::local::{fold, edgy, exec};
//!    ```
//!
//! 2. **Mixed code (rare).** Module-prefix the Local side to avoid
//!    name collisions with the Shared defaults:
//!    ```no_run
//!    use hylic::prelude::*;
//!    use hylic::prelude::local::{Local, LiftedSugarsLocal};
//!    use hylic::domain::local as ldom;
//!    // ldom::fold(...), ldom::edgy(...), ldom::exec(...)
//!    ```

pub use crate::domain::Local;
pub use crate::cata::pipeline::LiftedSugarsLocal;
