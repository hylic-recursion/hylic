pub fn push_indent(s: &str, indent: &str) -> String {
    s.replace("\n", &format!("\n{}", indent))
}
