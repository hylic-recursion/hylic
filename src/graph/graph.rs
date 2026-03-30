use super::types::{Edgy, Treeish};

#[derive(Clone)]
pub struct Graph<Top, Node> {
    pub treeish: Treeish<Node>,
    pub top_edgy: Edgy<Top, Node>,
}

impl<Top, Node> Graph<Top, Node>
where Top: 'static, Node: 'static,
{
    pub fn new(treeish: Treeish<Node>, top_edgy: Edgy<Top, Node>) -> Self {
        Graph { treeish, top_edgy }
    }

    pub fn map_treeish<F>(&self, mapper: F) -> Self
    where F: FnOnce(Treeish<Node>) -> Treeish<Node>,
    {
        Graph { treeish: mapper(self.treeish.clone()), top_edgy: self.top_edgy.clone() }
    }

    pub fn map_top_edgy<F>(&self, mapper: F) -> Self
    where F: FnOnce(Edgy<Top, Node>) -> Edgy<Top, Node>,
    {
        Graph { treeish: self.treeish.clone(), top_edgy: mapper(self.top_edgy.clone()) }
    }
}
