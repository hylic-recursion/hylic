use super::types::{Edgy, Treeish};

#[derive(Clone)]
pub struct Graph<Top, Node> {
    pub treeish: Treeish<Node>,
    pub top_edgy: Edgy<Top, Node>,
}

impl<Top, Node> Graph<Top, Node> {
    pub fn new(treeish: Treeish<Node>, top_edgy: Edgy<Top, Node>) -> Self {
        Graph { treeish, top_edgy }
    }
}
