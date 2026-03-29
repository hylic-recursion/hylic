// for now on String

// PURPOSE: any kinds of N to be joined by formatting function into a String

use std::{fmt::Display, sync::Arc};

use derive_more::Display;

use crate::rake::{vec_compress, VecHeap, VecHeapCompress};
use crate::utils::{push_indent, MapFn};

/// Spec, leads to raco
#[derive(Clone, Display)]
#[display("JoinHeapCfg{{sep={},l/r={}{},indent={}}}", separator, join_left_str, join_right_str, indent)]
pub struct JoinHeapCfg<N> {
    pub format_n: Arc<dyn Fn(&N) -> String + Send + Sync>,
    pub separator: String,
    pub join_left_str: String,
    pub join_right_str: String,
    pub indent: String, // appended before any \n in children
}

/// Formatting impl
impl<N> JoinHeapCfg<N> {

    pub fn format(&self, node: &N, childstrings: &[String]) -> String {
        let processed_childstrings = childstrings
            // .iter()
            // .map(|s| push_indent(s, &self.indent))
            // .collect::<Vec<_>>()
            .join(&self.separator);
        let mut result = String::new();
        result.push_str((&self.format_n)(node).as_str());

        let mut childpart = String::new();
        childpart.push_str(&self.join_left_str);
        childpart.push_str(&processed_childstrings);
        childpart.push_str(&self.join_right_str);

        let pushed_childpart = push_indent(&childpart, &self.indent);
        result.push_str(&pushed_childpart);

        result
    }
}

/// Mapping functions on format
impl <N> JoinHeapCfg<N> {

    pub fn map_format_n<F>(&self, f: F) -> Self
    where
        N: 'static,
        F: MapFn<Box<dyn Fn(&N) -> String + Send + Sync>> + 'static,
    {
        let original_format_n = self.format_n.clone();
        return JoinHeapCfg{
            format_n: Arc::from(f(Box::new(move |n: &N| (original_format_n)(n)))),
            separator: self.separator.clone(),
            join_left_str: self.join_left_str.clone(),
            join_right_str: self.join_right_str.clone(),
            indent: self.indent.clone(),
        };
    }



}

// implement the rake compress on the structure
impl <N> JoinHeapCfg<N> where 
N: Clone + Display + 'static {
    pub fn new(
        format_n: impl Fn(&N) -> String + Send + Sync + 'static,
        separator: &str,
        join_left_string: &str,
        join_right_string: &str,
        indent: &str,
    ) -> Self {
        JoinHeapCfg {
            format_n: Arc::from(Box::new(format_n) as Box<dyn Fn(&N) -> String + Send + Sync>),
            separator: separator.to_string(),
            join_left_str: join_left_string.to_string(),
            join_right_str: join_right_string.to_string(),
            indent: indent.to_string(),
        }
    }

    pub fn default_oneline(
        format_n: impl Fn(&N) -> String + Send + Sync + 'static,
    ) -> Self {
        JoinHeapCfg::new(
            format_n,
            ", ",
            "[",
            "]",
            "",
        )
    }

    pub fn default_multiline(
        format_n: impl Fn(&N) -> String + Send + Sync + 'static,
    ) -> Self {
        JoinHeapCfg::new(
            format_n,
            ",\n",
            "[\n",
            "\n]",
            "  ",
        )
    }

    pub fn make_raco(
        &self,
    ) -> VecHeapCompress<N, String> {
        let selfclone = self.clone();
        vec_compress(
            move |heap: &VecHeap<N, String>| {
                selfclone.format(&heap.node, &heap.childresults)
            },
        )
    }
}

impl <N> Default for JoinHeapCfg<N> where
N: Display + Clone + 'static
{
    fn default() -> Self
    {
        JoinHeapCfg::default_multiline(|n: &N| n.to_string())
    }
}


