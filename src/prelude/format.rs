// for now on String

// PURPOSE: any kinds of N to be joined by formatting function into a String

use std::{fmt::Display, sync::Arc};

use derive_more::Display;

use crate::prelude::vec_fold::{vec_fold, VecHeap, VecFold};
use crate::utils::push_indent;

pub type FormatFn<N> = Box<dyn Fn(&N) -> String + Send + Sync>;

/// Spec, leads to fold
#[derive(Clone, Display)]
#[display("TreeFormatCfg{{sep={},l/r={}{},indent={}}}", separator, join_left_str, join_right_str, indent)]
pub struct TreeFormatCfg<N> {
    pub format_n: Arc<dyn Fn(&N) -> String + Send + Sync>,
    pub separator: String,
    pub join_left_str: String,
    pub join_right_str: String,
    pub indent: String, // appended before any \n in children
}

/// Formatting impl
impl<N> TreeFormatCfg<N> {

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
impl <N> TreeFormatCfg<N> {

    pub fn map_format_n<F>(&self, f: F) -> Self
    where
        N: 'static,
        F: FnOnce(FormatFn<N>) -> FormatFn<N> + 'static,
    {
        let original_format_n = self.format_n.clone();
        return TreeFormatCfg{
            format_n: Arc::from(f(Box::new(move |n: &N| (original_format_n)(n)))),
            separator: self.separator.clone(),
            join_left_str: self.join_left_str.clone(),
            join_right_str: self.join_right_str.clone(),
            indent: self.indent.clone(),
        };
    }



}

// implement the fold finalize on the structure
impl <N> TreeFormatCfg<N> where 
N: Clone + Display + 'static {
    pub fn new(
        format_n: impl Fn(&N) -> String + Send + Sync + 'static,
        separator: &str,
        join_left_string: &str,
        join_right_string: &str,
        indent: &str,
    ) -> Self {
        TreeFormatCfg {
            format_n: Arc::from(Box::new(format_n) as FormatFn<N>),
            separator: separator.to_string(),
            join_left_str: join_left_string.to_string(),
            join_right_str: join_right_string.to_string(),
            indent: indent.to_string(),
        }
    }

    pub fn default_oneline(
        format_n: impl Fn(&N) -> String + Send + Sync + 'static,
    ) -> Self {
        TreeFormatCfg::new(
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
        TreeFormatCfg::new(
            format_n,
            ",\n",
            "[\n",
            "\n]",
            "  ",
        )
    }

    pub fn make_fold(
        &self,
    ) -> VecFold<N, String> {
        let selfclone = self.clone();
        vec_fold(
            move |heap: &VecHeap<N, String>| {
                selfclone.format(&heap.node, &heap.childresults)
            },
        )
    }
}

impl <N> Default for TreeFormatCfg<N> where
N: Display + Clone + 'static
{
    fn default() -> Self
    {
        TreeFormatCfg::default_multiline(|n: &N| n.to_string())
    }
}


