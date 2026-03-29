use std::fmt::Display;

pub trait MapFn<T>: Fn(T) -> T + 'static {}
impl<T, F: Fn(T) -> T + 'static> MapFn<T> for F {}

pub trait EdgyMapFn<T, U>: Fn(crate::graph::types::Edgy<T, U>) -> crate::graph::types::Edgy<T, U> + 'static {}
impl<T, U, F: Fn(crate::graph::types::Edgy<T, U>) -> crate::graph::types::Edgy<T, U> + 'static> EdgyMapFn<T, U> for F {}

pub fn push_indent_with_nl(s: &str, indent: &str) -> String {
    push_indent(&format!("\n{}", s), indent)
}

pub fn push_indent(s: &str, indent: &str) -> String {
    s.replace("\n", &format!("\n{}", indent))
}

pub fn trunc_mid(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max { return s.to_string(); }
    let keep = max.saturating_sub(3);
    let head = keep / 2;
    let tail = keep - head;
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(len - tail).collect();
    format!("{}...{}", h, t)
}

pub fn listize<T: Display>(list: &[T], prefix: &str) -> String {
    list.iter().map(|x| format!("{}{}", prefix, x)).collect::<Vec<_>>().join("\n")
}
