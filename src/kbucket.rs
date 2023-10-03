//! Based on https://github.com/tristanls/k-bucket/blob/master/index.js

#[derive(Debug)]
pub struct Kbucket<I: AsRef<[u8]>, V: Contact<Id = I>> {
    node_id: I,
    nodes_per_kbucket: usize,
    nodes_to_ping: usize,
    root: Node<V>,
}

pub trait Contact: PartialEq + std::fmt::Debug {
    type Id: AsRef<[u8]>;
    fn id(&self) -> &Self::Id;

    fn distance(&self, other_id: &Self::Id) -> usize {
        xor_distance(self.id().as_ref(), other_id.as_ref())
    }

    /// Returns `true` if `other` should replace `self`
    fn should_replace<'a>(&'a self, other: &'a Self) -> bool {
        // TODO:
        true
    }
}

fn xor_distance(first_id: &[u8], second_id: &[u8]) -> usize {
    let mut distance: usize = 0;

    let min = first_id.len().min(second_id.len());
    let max = first_id.len().max(second_id.len());

    for (a, b) in first_id.iter().zip(second_id.iter()) {
        distance = distance
            .wrapping_mul(256)
            .wrapping_add((*a as usize) ^ (*b as usize));
    }

    for _ in min..max {
        distance = distance.wrapping_mul(256).wrapping_add(255);
    }

    distance
}

#[derive(Debug)]
enum Node<V: Contact> {
    Inner {
        left: Box<Node<V>>,
        right: Box<Node<V>>,
    },
    Leaf {
        contacts: Vec<V>,
        dont_split: bool,
    },
}

impl<V: Contact> Node<V> {
    fn contacts(&self) -> &[V] {
        match self {
            Node::Inner { .. } => &[][..],
            Node::Leaf { contacts, .. } => &contacts,
        }
    }
}

impl<I: AsRef<[u8]>, V: Contact<Id = I>> Kbucket<I, V> {
    pub fn new(node_id: I, nodes_per_kbucket: Option<usize>, nodes_to_ping: Option<usize>) -> Self {
        let nodes_per_kbucket = nodes_per_kbucket.unwrap_or(20);
        let nodes_to_ping = nodes_to_ping.unwrap_or(3);

        Self {
            node_id,
            nodes_per_kbucket,
            nodes_to_ping,
            root: Node::Leaf {
                contacts: Vec::new(),
                dont_split: false,
            },
        }
    }

    pub fn add(&mut self, contact: V) {
        let mut bit_index = 0u32;
        let mut node = &mut self.root;

        while let Node::Inner { left, right, .. } = node {
            // this is not a leaf node but an inner node with 'low' and 'high'
            // branches; we will check the appropriate bit of the identifier and
            // delegate to the appropriate node for further processing
            node = match determine_node(contact.id(), bit_index) {
                Direction::Left => left.as_mut(),
                Direction::Right => right.as_mut(),
            };
            bit_index += 1;
        }

        let Node::Leaf {
            contacts,
            dont_split,
            ..
        } = node
        else {
            panic!("should not happen");
        };

        // check if the contact already exists
        let index = index_of(contacts, contact.id());

        if let Some(index) = index {
            update(contacts, index, contact);
            return;
        }

        if contacts.len() < self.nodes_per_kbucket {
            contacts.push(contact);
            // this.emit('added', contact)
            return;
        }

        // the bucket is full
        if *dont_split {
            // we are not allowed to split the bucket
            // we need to ping the first this.numberOfNodesToPing
            // in order to determine if they are alive
            // only if one of the pinged nodes does not respond, can the new contact
            // be added (this prevents DoS flodding with new invalid contacts)
            // this.emit('ping', node.contacts.slice(0, this.numberOfNodesToPing), contact)
            return;
        }

        *node = split(&self.node_id, contacts, bit_index);
        self.add(contact);
    }

    /// Removes contact with the provided id.
    pub fn remove(&mut self, id: I) {
        let mut bit_index = 0u32;
        let mut node = &mut self.root;

        while let Node::Inner { left, right, .. } = node {
            node = match determine_node(&id, bit_index) {
                Direction::Left => left.as_mut(),
                Direction::Right => right.as_mut(),
            };
            bit_index += 1;
        }

        let Node::Leaf { contacts, .. } = node else {
            panic!("should not happen");
        };

        let index = index_of(contacts, &id);
        if let Some(index) = index {
            let contact = contacts.remove(index);
            // this.emit('removed', contact)
        }
    }

    ///  Get a contact by its exact ID.
    /// If this is a leaf, loop through the bucket contents and return the correct
    ///  contact if we have it or null if not. If this is an inner node, determine
    ///  which branch of the tree to traverse and repeat.
    pub fn get(&self, id: I) -> Option<&V> {
        let mut bit_index = 0u32;
        let mut node = &self.root;

        while let Node::Inner { left, right, .. } = node {
            node = match determine_node(&id, bit_index) {
                Direction::Left => left,
                Direction::Right => right,
            };
            bit_index += 1;
        }

        let Node::Leaf { contacts, .. } = node else {
            panic!("should not happen");
        };

        // index of uses contact id for matching
        index_of(contacts, &id).and_then(|i| contacts.get(i))
    }

    /// Counts the total number of contacts in the tree.
    pub fn len(&self) -> usize {
        let mut count = 0;
        // TODO: avoid allocations
        let mut nodes = vec![&self.root];

        while let Some(node) = nodes.pop() {
            match node {
                Node::Inner { left, right } => {
                    nodes.push(left.as_ref());
                    nodes.push(right.as_ref());
                }
                Node::Leaf { contacts, .. } => {
                    count += contacts.len();
                }
            }
        }
        count
    }

    /// Get the n closest contacts to the provided node id. "Closest" here means:
    /// closest according to the XOR metric of the contact node id.
    pub fn closest(&self, id: I, n: Option<usize>) -> Vec<&V> {
        let cap = n.unwrap_or(1);
        let mut contacts = Vec::with_capacity(cap);
        let mut nodes = vec![&self.root];
        let mut bit_index = 0u32;

        while let Some(node) = nodes.pop() {
            if let Some(n) = n {
                if contacts.len() >= n {
                    break;
                }
            }
            match node {
                Node::Inner { left, right } => {
                    match determine_node(&id, bit_index) {
                        Direction::Left => {
                            nodes.push(right);
                            nodes.push(left);
                        }
                        Direction::Right => {
                            nodes.push(left);
                            nodes.push(right);
                        }
                    }
                    bit_index += 1;
                }
                Node::Leaf { contacts: c, .. } => {
                    contacts.extend(c);
                }
            }
        }

        contacts.sort_by(|a, b| a.distance(&id).cmp(&b.distance(&id)));

        if let Some(n) = n {
            contacts.truncate(n);
        }
        contacts
    }

    fn iter(&self) -> Iter<'_, V> {
        Iter {
            nodes: vec![&self.root],
            contacts: None,
            i: 0,
        }
    }
}

#[derive(Debug)]
pub struct Iter<'a, V: Contact> {
    nodes: Vec<&'a Node<V>>,
    contacts: Option<&'a [V]>,
    i: usize,
}

impl<'a, V: Contact> Iterator for Iter<'a, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(contacts) = self.contacts {
                match contacts.get(self.i) {
                    Some(contact) => {
                        self.i += 1;
                        return Some(contact);
                    }
                    None => {
                        self.contacts = None;
                        self.i = 0;
                    }
                }
            }

            let node = self.nodes.pop()?;
            match node {
                Node::Inner { left, right } => {
                    self.nodes.push(left.as_ref());
                    self.nodes.push(right.as_ref());
                }
                Node::Leaf { contacts, .. } => {
                    self.contacts = Some(&contacts);
                    self.i = 0;
                }
            }
        }
    }
}

fn update<V: Contact>(contacts: &mut Vec<V>, index: usize, contact: V) {
    let incumbent = &contacts[index];
    let should_replace = incumbent.should_replace(&contact);

    // if the selection is our old contact and the candidate is some new
    // contact, then there is nothing to do
    if !should_replace && incumbent != &contact {
        return;
    }

    // remove old contact
    let old_contact = contacts.remove(index);

    // add more recent contact version
    let to_insert = if should_replace { contact } else { old_contact };
    contacts.push(to_insert);
    // this.emit('updated', incumbent, selection)
}

/// Splits the node, redistributes contacts to the new nodes, and marks the
/// node that was split as an inner node of the binary tree of nodes by
/// setting this.root.contacts = null
fn split<I: AsRef<[u8]>, V: Contact>(
    node_id: &I,
    contacts: &mut Vec<V>,
    bit_index: u32,
) -> Node<V> {
    let mut left_contacts = Vec::new();
    let mut right_contacts = Vec::new();

    // redistribute existing contacts amongst the two newly created nodes
    for contact in contacts.drain(..) {
        match determine_node(contact.id(), bit_index) {
            Direction::Left => left_contacts.push(contact),
            Direction::Right => right_contacts.push(contact),
        }
    }

    // don't split the "far away" node
    // we check where the local node would end up and mark the other one as
    // "dontSplit" (i.e. "far away")
    let self_direction = determine_node(node_id, bit_index);

    Node::Inner {
        left: Box::new(Node::Leaf {
            contacts: left_contacts,
            dont_split: self_direction == Direction::Right,
        }),
        right: Box::new(Node::Leaf {
            contacts: right_contacts,
            dont_split: self_direction == Direction::Left,
        }),
    }
}

/// Determines whether the id at the bit_index is 0 or 1.
/// Returns left leaf if `id` at `bit_index` is 0, right leaf otherwise
fn determine_node<I: AsRef<[u8]>>(id: &I, bit_index: u32) -> Direction {
    // **NOTE** remember that id is a [u8] and has granularity of
    // bytes (8 bits), whereas the bitIndex is the _bit_ index (not byte)

    // id's that are too short are put in low bucket (1 byte = 8 bits)
    // (bit_index >> 3) finds how many bytes the bit_index describes
    // bit_index % 8 checks if we have extra bits beyond byte multiples
    // if number of bytes is <= no. of bytes described by bitIndex and there
    // are extra bits to consider, this means id has less bits than what
    // bit_index describes, id therefore is too short, and will be put in low bucket
    let bytes_described_by_bit_index = bit_index >> 3;
    let bit_index_within_byte = bit_index % 8;
    if (id.as_ref().len() <= bytes_described_by_bit_index as usize) && (bit_index_within_byte != 0)
    {
        return Direction::Left;
    }

    let byte_under_consideration = id.as_ref()[bytes_described_by_bit_index as usize];

    // byte_under_consideration is an integer from 0 to 255 represented by 8 bits
    // where 255 is 11111111 and 0 is 00000000
    // in order to find out whether the bit at bitIndexWithinByte is set
    // we construct (1 << (7 - bitIndexWithinByte)) which will consist
    // of all bits being 0, with only one bit set to 1
    // for example, if bitIndexWithinByte is 3, we will construct 00010000 by
    // (1 << (7 - 3)) -> (1 << 4) -> 16
    if ((byte_under_consideration as u32) & (1 << (7 - bit_index_within_byte))) > 0 {
        return Direction::Right;
    }

    Direction::Left
}

fn index_of<I: AsRef<[u8]>, V: Contact<Id = I>>(contacts: &[V], id: &I) -> Option<usize> {
    contacts.iter().position(|c| c.id().as_ref() == id.as_ref())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Contact for [u8; 1] {
        type Id = [u8; 1];

        fn id(&self) -> &[u8; 1] {
            self
        }
    }

    impl Contact for [u8; 2] {
        type Id = [u8; 2];
        fn id(&self) -> &[u8; 2] {
            self
        }
    }
    impl Contact for [u8; 3] {
        type Id = [u8; 3];
        fn id(&self) -> &[u8; 3] {
            self
        }
    }

    fn arr(a: u8) -> [u8; 1] {
        [a]
    }
    fn arr2(a: u8, b: u8) -> [u8; 2] {
        [a, b]
    }

    #[test]
    fn test_closest_nodes_are_returned() {
        let mut k_bucket = Kbucket::new([0u8; 1], None, None);
        for i in 0u8..0x12 {
            k_bucket.add(arr(i));
        }
        let contact = arr(0x15); // 00010101
        let contacts = k_bucket.closest(*contact.id(), Some(3));
        assert_eq!(contacts.len(), 3);
        dbg!(&contacts);
        assert_eq!(contacts[0].id(), &arr(0x11)); // distance: 00000100
        assert_eq!(contacts[1].id(), &arr(0x10)); // distance: 00000101
        assert_eq!(contacts[2].id(), &arr(0x05)); // distance: 00010000
    }

    #[test]
    fn test_closest_nodes_are_returned_including_exact_match() {
        let mut k_bucket = Kbucket::new([44u8; 1], None, None);
        for i in 0u8..0x12 {
            k_bucket.add(arr(i));
        }

        let contact = arr(0x11); // 00010001
        let contacts = k_bucket.closest(*contact.id(), Some(3));
        assert_eq!(contacts[0].id(), &arr(0x11)); // distance: 00000000
        assert_eq!(contacts[1].id(), &arr(0x10)); // distance: 00000001
        assert_eq!(contacts[2].id(), &arr(0x01)); // distance: 00010000
    }

    #[test]
    fn test_closest_nodes_are_returned_if_if_not_enough() {
        let mut k_bucket = Kbucket::new([0u8; 2], None, None);
        for i in 0u8..k_bucket.nodes_per_kbucket as u8 {
            k_bucket.add(arr2(0x80, i));
            k_bucket.add(arr2(0x01, i));
        }
        k_bucket.add(arr2(0x00, 0x01));

        assert_eq!(k_bucket.len(), 41);

        let contact = arr2(0x00, 0x03); // 0000000000000011
        let contacts = k_bucket.closest(*contact.id(), Some(22));

        dbg!(&k_bucket);
        assert_eq!(contacts.len(), 22);

        assert_eq!(contacts[0].id(), &arr2(0x00, 0x01)); // distance: 0000000000000010
        assert_eq!(contacts[1].id(), &arr2(0x01, 0x03)); // distance: 0000000100000000
        assert_eq!(contacts[2].id(), &arr2(0x01, 0x02)); // distance: 0000000100000010
        assert_eq!(contacts[3].id(), &arr2(0x01, 0x01));
        assert_eq!(contacts[4].id(), &arr2(0x01, 0x00));
        assert_eq!(contacts[5].id(), &arr2(0x01, 0x07));
        assert_eq!(contacts[6].id(), &arr2(0x01, 0x06));
        assert_eq!(contacts[7].id(), &arr2(0x01, 0x05));
        assert_eq!(contacts[8].id(), &arr2(0x01, 0x04));
        assert_eq!(contacts[9].id(), &arr2(0x01, 0x0b));
        assert_eq!(contacts[10].id(), &arr2(0x01, 0x0a));
        assert_eq!(contacts[11].id(), &arr2(0x01, 0x09));
        assert_eq!(contacts[12].id(), &arr2(0x01, 0x08));
        assert_eq!(contacts[13].id(), &arr2(0x01, 0x0f));
        assert_eq!(contacts[14].id(), &arr2(0x01, 0x0e));
        assert_eq!(contacts[15].id(), &arr2(0x01, 0x0d));
        assert_eq!(contacts[16].id(), &arr2(0x01, 0x0c));
        assert_eq!(contacts[17].id(), &arr2(0x01, 0x13));
        assert_eq!(contacts[18].id(), &arr2(0x01, 0x12));
        assert_eq!(contacts[19].id(), &arr2(0x01, 0x11));
        assert_eq!(contacts[20].id(), &arr2(0x01, 0x10));
        assert_eq!(contacts[21].id(), &arr2(0x80, 0x03)); // distance: 1000000000000000
    }

    #[test]
    fn test_adding_a_contact_places_it_in_root_node() {
        let mut k_bucket = Kbucket::new([b'z'], None, None);
        let contact = [b'a'];
        k_bucket.add(contact);
        match k_bucket.root {
            Node::Leaf {
                contacts,
                dont_split,
            } => {
                assert!(!dont_split);
                assert_eq!(contacts, vec![contact]);
            }
            Node::Inner { .. } => panic!("invalid root"),
        }
    }

    #[test]
    fn test_adding_existing_contact_no_change_to_length() {
        let mut k_bucket = Kbucket::new([b'z'], None, None);
        let contact = [b'a'];
        k_bucket.add(contact);
        k_bucket.add([b'a']);
        match k_bucket.root {
            Node::Leaf {
                contacts,
                dont_split,
            } => {
                assert!(!dont_split);
                assert_eq!(contacts, vec![contact]);
            }
            Node::Inner { .. } => panic!("invalid root"),
        }
    }

    #[test]
    fn test_adding_max_number_does_not_split() {
        let mut k_bucket = Kbucket::new([b'z'], None, None);
        for i in 0..k_bucket.nodes_per_kbucket {
            k_bucket.add([i as u8]);
        }
        match k_bucket.root {
            Node::Leaf {
                contacts,
                dont_split,
            } => {
                assert_eq!(contacts.len(), 20);
                assert!(!dont_split);
            }
            Node::Inner { .. } => panic!("invalid split"),
        }
    }

    #[test]
    fn test_adding_max_number_plus_1_does_split() {
        let mut k_bucket = Kbucket::new([b'z'], None, None);
        for i in 0..k_bucket.nodes_per_kbucket + 1 {
            k_bucket.add([i as u8]);
        }
        match k_bucket.root {
            Node::Leaf { .. } => {
                panic!("invalid split");
            }
            Node::Inner { .. } => {}
        }
    }

    #[test]
    fn test_splitting_far_away() {
        let mut k_bucket = Kbucket::new([0x00u8], None, None);
        for i in 0..k_bucket.nodes_per_kbucket + 1 {
            k_bucket.add([i as u8]);
        }

        // above algorithm will split left node 4 times and put 0x00 through 0x0f
        // in the left node, and put 0x10 through 0x14 in right node
        // since localNodeId is 0x00, we expect every right node to be "far" and
        // therefore marked as "dontSplit = true"
        // there will be one "left" node and four "right" nodes (t.expect(5))

        fn traverse<V: Contact<Id = [u8; 1]>>(node: &Node<V>, dont_split: bool) {
            match node {
                Node::Inner { left, right } => {
                    traverse(left, false);
                    traverse(right, true);
                }
                Node::Leaf { dont_split: ds, .. } => {
                    assert_eq!(dont_split, *ds);
                }
            }
        }

        traverse(&k_bucket.root, false);
    }

    // TODO: figure out ordering
    // #[test]
    // fn test_iter_all_contacts_sorted_low_high_buckets() {
    //     let mut k_bucket = Kbucket::new([0x00u8, 0x01u8, 0u8], None, None);
    //     let mut expected_ids = Vec::new();
    //     for i in 0..k_bucket.nodes_per_kbucket {
    //         k_bucket.add([0x80, i as u8, 0]); // make sure all go into "far away" bucket
    //         expected_ids.push([0x80, i as u8, 0]);
    //     }

    //     // cause a split to happen
    //     k_bucket.add([0, 0x80, 19]);

    //     let contacts: Vec<_> = k_bucket.iter().collect();
    //     assert_eq!(contacts.len(), k_bucket.nodes_per_kbucket + 1);
    //     assert_eq!(contacts[0].id(), &[0, 0x80, 19]);
    //     for (i, id) in contacts[1..].iter().enumerate() {
    //         assert_eq!(**id, expected_ids[i])
    //     }
    // }
}
