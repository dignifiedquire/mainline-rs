//! Based on https://github.com/tristanls/k-bucket/blob/master/index.js

#[derive(Debug)]
pub struct Kbucket<V> {
    node_id: [u8; 20],
    nodes_per_kbucket: usize,
    nodes_to_ping: usize,
    root: Node<V>,
}

#[derive(Debug)]
enum Node<V> {
    Inner {
        dont_split: bool,
        left: Option<Box<Node<V>>>,
        right: Option<Box<Node<V>>>,
    },
    Leaf {
        contacts: Vec<V>,
        dont_split: bool,
        left: Option<Box<Node<V>>>,
        right: Option<Box<Node<V>>>,
    },
}

// TODO: add arbiter
// TODO: add distance

impl<V> Kbucket<V> {
    pub fn new(
        node_id: [u8; 20],
        nodes_per_kbucket: Option<usize>,
        nodes_to_ping: Option<usize>,
    ) -> Self {
        let nodes_per_kbucket = nodes_per_kbucket.unwrap_or(20);
        let nodes_to_ping = nodes_to_ping.unwrap_or(3);

        Self {
            node_id,
            nodes_per_kbucket,
            nodes_to_ping,
            root: Node::Leaf {
                contacts: Vec::new(),
                dont_split: false,
                left: None,
                right: None,
            },
        }
    }
}
