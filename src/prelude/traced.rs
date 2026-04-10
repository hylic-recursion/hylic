use crate::graph::{Treeish, treeish_visit};

type Index = usize;
#[derive(Clone, Debug)]
pub enum Traced<N> {
    Node(N, Index, Box<Traced<N>>),
    Root(N)
}

impl<N> Traced<N> {
    pub fn get_node(&self) -> &N {
        match self {
            Traced::Node(node, _, _) => node,
            Traced::Root(node) => node,
        }
    }

    pub fn get_parent(&self) -> Option<&Traced<N>> {
        match self {
            Traced::Node(_, _, parent) => Some(parent),
            Traced::Root(_) => None,
        }
    }
}

pub fn traced_treeish<N: Clone + 'static>(base_impl: Treeish<N>) -> Treeish<Traced<N>> {
    treeish_visit(move |node: &Traced<N>, cb: &mut dyn FnMut(&Traced<N>)| {
        let mut index = 0;
        base_impl.visit(node.get_node(), &mut |child: &N| {
            let traced = Traced::Node(child.clone(), index, Box::new(node.clone()));
            cb(&traced);
            index += 1;
        });
    })
}
