// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// TODO
// ----
// docs - mod docs, item docs
// tests
// impl Default, Extend
// impl DoubleEndedIter and ExactSizeIter for RopeChars
// better allocation
// balancing?

use std::fmt;
use std::ops::Range;
use util::utf8_char_width;

// A Rope, based on an unbalanced binary tree. The rope is somewhat special in
// that it tracks positions in the source text. So when locating a position in
// the rope, the user can use either a current position in the text or a
// position in the source text, which the Rope will adjust to a current position
// whilst searching.
pub struct Rope {
    root: Node,
    len: usize,
    // FIXME: Allocation is very dumb at the moment, we always add another
    // buffer for every inserted string and we never resuse or collect old
    // memory
    storage: Vec<Vec<u8>>
}

// An iterator over the chars in a rope.
pub struct RopeChars<'rope> {
    data: RopeSlice<'rope>,
    cur_node: usize,
    cur_byte: usize,
    abs_byte: usize,
}


impl_rope!(Rope);

impl Rope {
    // Create an empty rope.
    pub fn new() -> Rope {
        Rope {
            root: Node::empty_inner(),
            len: 0,
            storage: vec![],
        }
    }

    // Uses text as initial storage.
    pub fn from_string(text: String) -> Rope {
        // TODO should split very large texts into segments as we insert

        let mut result = Rope::new();
        result.insert(0, text);
        result
    }

    pub fn insert(&mut self, start: usize, text: String) {
        self.insert_inner(start,
                          text,
                          |this, node| this.root.insert(node, start))
    }

    fn insert_inner<F>(&mut self,
                       start: usize,
                       text: String,
                       do_insert: F)
        where F: Fn(&mut Rope, Box<Node>) -> NodeAction
    {
        if text.len() == 0 {
            return;
        }

        debug_assert!(start <= self.len, "insertion out of bounds of rope");

        let len = text.len();
        let storage = text.into_bytes();
        let new_node = Box::new(Node::new_leaf(&storage[..][0] as *const u8, len));
        self.storage.push(storage);

        match do_insert(self, new_node) {
            NodeAction::Change(n, adj) => {
                assert!(adj as usize == len);
                self.root = *n;
            }
            NodeAction::Adjust(adj) => {
                assert!(adj as usize == len);
            }
            _ => panic!("Unexpected action")
        }
        self.len += len;
    }

    pub fn remove(&mut self, start: usize, end: usize) {
        self.remove_inner(start, end, |this| this.root.remove(start, end))
    }
}

generate_ropeslice_struct!(Lnode);

impl<'rope> Iterator for RopeChars<'rope> {
    type Item = (char, usize);
    fn next(&mut self) -> Option<(char, usize)> {
        if self.cur_node >= self.data.nodes.len() {
            return None;
        }

        let byte = self.abs_byte;
        let node = self.data.nodes[self.cur_node];
        if self.cur_byte >= node.len {
            self.cur_byte = 0;
            self.cur_node += 1;
            return self.next();
        }

        let result = self.read_char();
        return Some((result, byte));
    }
}

impl<'rope> RopeChars<'rope> {
    fn read_char(&mut self) -> char {
        let first_byte = self.read_byte();
        let width = utf8_char_width(first_byte);
        if width == 1 {
            return first_byte as char
        }
        if width == 0 {
            panic!("non-utf8 char in rope");
        }
        let mut buf = [first_byte, 0, 0, 0];
        {
            let mut start = 1;
            while start < width {
                buf[start] = self.read_byte();
                start += 1;
            }
        }
        match ::std::str::from_utf8(&buf[..width]).ok() {
            Some(s) => s.chars().nth(0).expect("FATAL: we checked presence of this before"),
            None => panic!("bad chars in rope")
        }
    }

    fn read_byte(&mut self) -> u8 {
        let node = self.data.nodes[self.cur_node];
        let addr = node.text as usize + self.cur_byte;
        self.cur_byte += 1;
        self.abs_byte += 1;
        let addr = addr as *const u8;
        unsafe {
            *addr
        }
    }
}

impl ::std::str::FromStr for Rope {
    type Err = ();
    fn from_str(text: &str) -> Result<Rope, ()> {
        // TODO should split large texts into segments as we insert

        let mut result = Rope::new();
        result.insert_copy(0, text);
        Ok(result)
    }
}

impl<'a> fmt::Display for RopeSlice<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        if self.nodes.len() == 0 {
            return Ok(());
        }

        let last_idx = self.nodes.len() - 1;
        for (i, n) in self.nodes.iter().enumerate() {
            let mut ptr = n.text;
            let mut len = n.len;
            if i == 0 {
                ptr = (ptr as usize + self.start) as *const u8;
                len -= self.start;
            }
            if i == last_idx {
                len = self.len;
            }
            unsafe {
                try!(write!(fmt,
                            "{}",
                            ::std::str::from_utf8(::std::slice::from_raw_parts(ptr, len)).unwrap()));
            }
        }
        Ok(())
    }
}

impl<'a> fmt::Debug for RopeSlice<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let last_idx = self.nodes.len() - 1;
        for (i, n) in self.nodes.iter().enumerate() {
            let mut ptr = n.text;
            let mut len = n.len;
            if i == 0 {
                ptr = (ptr as usize + self.start) as *const u8;
                len -= self.start;
            } else {
                try!(write!(fmt, "|"));
            }
            if i == last_idx {
                len = self.len;
            }
            unsafe {
                try!(write!(fmt,
                            "\"{}\"",
                            ::std::str::from_utf8(::std::slice::from_raw_parts(ptr, len)).unwrap()));
            }
        }
        Ok(())
    }
}

impl fmt::Display for Rope {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.root)
    }
}

impl fmt::Debug for Rope {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{:?}", self.root)
    }
}

impl fmt::Display for Node {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Node::InnerNode(Inode { ref left, ref right, .. }) => {
                if let Some(ref left) = *left {
                    write!(fmt, "{}", left)
                } else {
                    Ok(())
                }.and_then(|_| if let Some(ref right) = *right {
                    write!(fmt, "{}", right)
                } else {
                    Ok(())
                })
            }
            Node::LeafNode(Lnode{ ref text, len, .. }) => {
                unsafe {
                    write!(fmt,
                           "{}",
                           ::std::str::from_utf8(::std::slice::from_raw_parts(*text, len)).unwrap())
                }
            }
        }
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            Node::InnerNode(Inode { ref left, ref right, weight, .. }) => {
                try!(write!(fmt, "("));
                if let Some(ref left) = *left {
                    try!(write!(fmt, "left: {:?}", &**left));
                } else {
                    try!(write!(fmt, "left: ()"));
                }
                try!(write!(fmt, ", "));
                if let Some(ref right) = *right {
                    try!(write!(fmt, "right: {:?}", &**right));
                } else {
                    try!(write!(fmt, "right: ()"));
                }
                write!(fmt, "; {})", weight)
            }
            Node::LeafNode(Lnode{ ref text, len, .. }) => {
                unsafe {
                    write!(fmt,
                           "(\"{}\"; {})",
                           ::std::str::from_utf8(::std::slice::from_raw_parts(*text, len)).unwrap(),
                           len)
                }
            }
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
enum Node {
    InnerNode(Inode),
    LeafNode(Lnode),
}

#[derive(Clone, Eq, PartialEq)]
struct Inode {
    weight: usize,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

#[derive(Clone, Eq, PartialEq)]
struct Lnode {
    text: *const u8,
    len: usize,
}

impl Node {
    fn empty_inner() -> Node {
        Node::InnerNode(Inode {
            left: None,
            right: None,
            weight: 0,
        })
    }

    fn new_inner(left: Option<Box<Node>>,
                 right: Option<Box<Node>>,
                 weight: usize)
    -> Node {
        Node::InnerNode(Inode {
            left: left,
            right: right,
            weight: weight,
        })
    }

    fn new_leaf(text: *const u8, len: usize) -> Node {
        Node::LeafNode(Lnode {
            text: text,
            len: len,
        })
    }

    fn len(&self) -> usize {
        match *self {
            Node::InnerNode(Inode { weight, ref right, .. }) => {
                match *right {
                    Some(ref r) => weight + r.len(),
                    None => weight
                }
            }
            Node::LeafNode(Lnode { len, .. }) => len,
        }
    }

    // Most of these methods are just doing dynamic dispatch, TODO use a macro

    // precond: start < end
    fn remove(&mut self, start: usize, end: usize) -> NodeAction {
        match *self {
            Node::InnerNode(ref mut i) => i.remove(start, end),
            Node::LeafNode(ref mut l) => l.remove(start, end),
        }
    }

    fn insert(&mut self, node: Box<Node>, start: usize) -> NodeAction {
        match *self {
            Node::InnerNode(ref mut i) => i.insert(node, start),
            Node::LeafNode(ref mut l) => l.insert(node, start),
        }
    }

    fn find_slice<'a>(&'a self, start: usize, end: usize, slice: &mut RopeSlice<'a>) {
        match *self {
            Node::InnerNode(ref i) => i.find_slice(start, end, slice),
            Node::LeafNode(ref l) => l.find_slice(start, end, slice),
        }
    }

    fn replace(&mut self, start: usize, new_str: &str) {
        match *self {
            Node::InnerNode(ref mut i) => i.replace(start, new_str),
            Node::LeafNode(ref mut l) => l.replace(start, new_str),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum NodeAction {
    None,
    Remove,
    Adjust(isize), // Arg is the length of the old node - the length of the newly adjusted node.
    Change(Box<Node>, isize) // Args are the new node and the change in length.
}

impl Inode {
    fn remove(&mut self, start: usize, end: usize) -> NodeAction {
        debug!("Inode::remove: {}, {}, {}", start, end, self.weight);

        let left_action = if start <= self.weight {
            if let Some(ref mut left) = self.left {
                left.remove(start, end)
            } else {
                panic!();
            }
        } else {
            NodeAction::None
        };

        let right_action = if end > self.weight {
            if let Some(ref mut right) = self.right {
                let start = if start < self.weight {
                    0
                } else {
                    start - self.weight
                };
                right.remove(start, end - self.weight)
            } else {
                panic!();
            }
        } else {
            NodeAction::None
        };


        if left_action == NodeAction::Remove && right_action == NodeAction::Remove ||
           left_action == NodeAction::Remove && self.right.is_none() ||
           right_action == NodeAction::Remove && self.left.is_none() {
            return NodeAction::Remove;
        }

        if left_action == NodeAction::Remove {
            return NodeAction::Change(self.right.clone().unwrap(),
                                      -(self.weight as isize));
        }
        if right_action == NodeAction::Remove {
            return NodeAction::Change(self.left.clone().unwrap(),
                                      -(self.right.as_ref().map(|n| n.len()).unwrap() as isize));
        }

        let mut total_adj = 0;
        if let NodeAction::Change(ref n, adj) = left_action {
            self.left = Some(n.clone());
            self.weight = (self.weight as isize + adj) as usize;
            total_adj += adj;
        }
        if let NodeAction::Change(ref n, adj) = right_action {
            self.right = Some(n.clone());
            total_adj += adj;
        }

        if let NodeAction::Adjust(adj) = left_action {
            self.weight = (self.weight as isize + adj) as usize;
            total_adj += adj;
        }
        if let NodeAction::Adjust(adj) = right_action {
            total_adj += adj;
        }

        return NodeAction::Adjust(total_adj);
    }

    fn insert(&mut self, node: Box<Node>, start: usize) -> NodeAction {
        let mut total_adj = 0;
        if start <= self.weight {
            let action = if let Some(ref mut left) = self.left {
                left.insert(node, start)
            } else {
                assert!(self.weight == 0);
                let len = node.len() as isize;
                NodeAction::Change(node, len)
            };

            match action {
                NodeAction::Change(n, adj) => {
                    self.left = Some(n);
                    self.weight += adj as usize;
                    total_adj += adj;
                }
                NodeAction::Adjust(adj) => {
                    self.weight += adj as usize;
                    total_adj += adj;
                }
                _ => panic!("Unexpected action"),
            }
        } else {
            let action = if let Some(ref mut right) = self.right {
                assert!(start >= self.weight);
                right.insert(node, start - self.weight)
            } else {
                let len = node.len() as isize;
                NodeAction::Change(node, len)
            };

            match action {
                NodeAction::Change(n, adj) => {
                    self.right = Some(n);
                    total_adj += adj;
                }
                NodeAction::Adjust(adj) => total_adj += adj,
                _ => panic!("Unexpected action"),
            }
        }

        NodeAction::Adjust(total_adj)
    }

    fn find_slice<'a>(&'a self, start: usize, end: usize, slice: &mut RopeSlice<'a>) {
        debug!("Inode::find_slice: {}, {}, {}", start, end, self.weight);
        if start < self.weight {
            self.left.as_ref().unwrap().find_slice(start, end, slice);
        }
        if end > self.weight {
            let start = if start < self.weight {
                0
            } else {
                start - self.weight
            };
            self.right.as_ref().unwrap().find_slice(start, end - self.weight, slice)
        }
    }

    fn replace(&mut self, start: usize, new_str: &str) {
        debug!("Inode::replace: {}, {}, {}", start, new_str, self.weight);
        let end = start + new_str.len();
        if start < self.weight {
            if let Some(ref mut left) = self.left {
                left.replace(start, &new_str[..::std::cmp::min(self.weight-start, new_str.len())]);
            } else {
                panic!();
            }
        }
        if end > self.weight {
            let (start, offset) = if start < self.weight {
                (0, self.weight - start)
            } else {
                (start - self.weight, 0)
            };
            if let Some(ref mut right) = self.right {
                right.replace(start, &new_str[offset..]);
            } else {
                panic!();
            }
        }
    }
}

impl Lnode {
    fn remove(&mut self, start: usize, end: usize) -> NodeAction {
        debug!("Lnode::remove: {}, {}, {}", start, end, self.len);
        assert!(start <= self.len);

        if start == 0 && end >= self.len {
            // The removal span includes us, remove ourselves.
            return NodeAction::Remove;
        }

        let old_len = self.len;
        if start == 0 {
            // Truncate the left of the node.
            self.text = (self.text as usize + end) as *const u8;
            self.len = old_len - end;
            let delta = self.len as isize - old_len as isize;
            return NodeAction::Adjust(delta);
        }

        if end >= self.len {
            // Truncate the right of the node.
            self.len = start;
            return NodeAction::Adjust(self.len as isize - old_len as isize);
        }

        let delta = -((end - start) as isize);
        // Split the node (span to remove is in the middle of the node).
        let new_node = Node::new_inner(
            Some(Box::new(Node::new_leaf(self.text, start))),
            Some(Box::new(Node::new_leaf((self.text as usize + end) as *const u8,
                                    old_len - end))),
            start);
        return NodeAction::Change(Box::new(new_node), delta);
    }

    fn insert(&mut self, node: Box<Node>, start: usize) -> NodeAction {
        let len = node.len();
        if start == 0 {
            // Insert at the start of the node
            let new_node = Box::new(Node::new_inner(Some(node),
                                                    Some(Box::new(Node::LeafNode(self.clone()))),
                                                    len));
            return NodeAction::Change(new_node, len as isize)
        }

        if start == self.len {
            // Insert at the end of the node
            let new_node = Box::new(Node::new_inner(Some(Box::new(Node::LeafNode(self.clone()))),
                                                    Some(node),
                                                    self.len));
            return NodeAction::Change(new_node, len as isize)
        }

        // Insert into the middle of the node
        let left = Some(Box::new(Node::new_leaf(self.text, start)));
        let new_left = Box::new(Node::new_inner(left, Some(node), start));
        let right = Some(Box::new(Node::new_leaf((self.text as usize + (start)) as *const u8,
                                                  self.len - start)));
        let new_node = Box::new(Node::new_inner(Some(new_left), right, start + len));

        return NodeAction::Change(new_node, len as isize)
    }

    fn find_slice<'a>(&'a self, start: usize, end: usize, slice: &mut RopeSlice<'a>) {
        debug!("Lnode::find_slice: {}, {}, {}", start, end, self.len);
        debug_assert!(start < self.len, "Shouldn't have called this fn, we're out of bounds");

        slice.nodes.push(self);
        let mut len = ::std::cmp::min(end, self.len);
        if start > 0 {
            slice.start = start;
            len -= start;
        }
        slice.len = len;
    }

    fn replace(&mut self, start: usize, new_str: &str) {
        println!("Lnode::replace: {}, {}, {}", start, new_str, self.len);
        debug_assert!(start + new_str.bytes().len() <= self.len);

        let addr = (self.text as usize + start) as *mut u8;
        unsafe {
            ::std::ptr::copy_nonoverlapping(new_str.as_ptr(), addr, new_str.bytes().len());
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new() {
        let r = Rope::new();
        assert!(r.len() == 0);
        assert!(r.to_string() == "");

        let r = Rope::from_string("Hello world!".to_string());
        assert!(r.len() == 12);
        assert!(r.to_string() == "Hello world!");
    }

    #[test]
    fn test_from_string() {
        let r: Rope = "Hello world!".parse().unwrap();
        assert!(r.to_string() == "Hello world!");
    }

    #[test]
    fn test_slice_empty_rope() {
        let r: Rope = Rope::new();
        let _ = r.full_slice();
    }

    #[test]
    fn test_remove() {
        let mut r: Rope = "Hello world!".parse().unwrap();
        r.remove(0, 10);
        assert!(r.to_string() == "d!");

        let mut r: Rope = "Hello world!".parse().unwrap();
        r.remove(4, 12);
        assert!(r.to_string() == "Hell");

        let mut r: Rope = "Hello world!".parse().unwrap();
        r.remove(4, 10);
        assert!(r.to_string() == "Helld!");
    }

    #[test]
    fn test_insert_copy() {
        let mut r: Rope = "Hello world!".parse().unwrap();
        r.insert_copy(0, "foo");
        assert!(r.to_string() == "fooHello world!");
        assert!(r.slice(2..8).to_string() == "oHello");

        let mut r: Rope = "Hello world!".parse().unwrap();
        r.insert_copy(12, "foo");
        assert!(r.to_string() == "Hello world!foo");
        assert!(r.slice(2..8).to_string() == "llo wo");

        let mut r: Rope = "Hello world!".parse().unwrap();
        r.insert_copy(5, "foo");
        assert!(r.to_string() == "Hellofoo world!");
        assert!(r.slice(2..8).to_string() == "llofoo");
    }

    #[test]
    fn test_push_copy() {
        let mut r: Rope = "Hello world!".parse().unwrap();
        r.push_copy("foo");
        assert!(r.to_string() == "Hello world!foo");
        assert!(r.slice(2..8).to_string() == "llo wo");
    }

    #[test]
    fn test_insert_replace() {
        let mut r: Rope = "hello worl\u{00bb0}!".parse().unwrap();
        r.insert_copy(5, "bb");
        assert!(r.to_string() == "hellobb worlர!");
        r.replace(0, 'H');
        r.replace(15, '~');
        r.replace_str(5, "fo\u{00cb0}");
        assert!(r.to_string() == "Hellofoರrlர~");
        assert!(r.slice(0..10).to_string() == "Hellofoರ");
        assert!(r.slice(5..10).to_string() == "foರ");
        assert!(r.slice(10..15).to_string() == "rlர");

        let expected = "Hellofoರrlர~";
        let mut byte_pos = 0;
        for ((c, b), e) in r.chars().zip(expected.chars()) {
            assert!(c == e);
            assert!(b == byte_pos);
            byte_pos += e.len_utf8();
        }
    }

    #[test]
    fn test_slice_iter_from_start() {
        let mut r: Rope = "Helloworld!".parse().unwrap();
        // insert some data to make sure the rope is split into multiple segments
        r.insert_copy(5, " ");

        let mut slice = r.slice(0..4);
        assert_eq!(Some(b'H'), slice.next());
        assert_eq!(Some(b'e'), slice.next());
        assert_eq!(Some(b'l'), slice.next());
        assert_eq!(Some(b'l'), slice.next());
        assert_eq!(Some(b'o'), slice.next());
        assert_eq!(None, slice.next());
    }

    #[test]
    fn test_slice_iter_from_middle() {
        let mut r: Rope = "Helloworld!".parse().unwrap();
        // insert some data to make sure the rope is split into multiple segments
        r.insert_copy(5, " ");

        let mut slice = r.slice(3..9);
        assert_eq!(Some(b'l'), slice.next());
        assert_eq!(Some(b'o'), slice.next());
        assert_eq!(Some(b' '), slice.next());
        assert_eq!(Some(b'w'), slice.next());
        assert_eq!(Some(b'o'), slice.next());
        assert_eq!(Some(b'r'), slice.next());
        assert_eq!(None, slice.next());
    }

    #[test]
    fn test_slice_without_split() {
        let r: Rope = "Hello world!".parse().unwrap();

        let mut slice = r.slice(3..9);
        assert_eq!(Some(b'l'), slice.next());
        assert_eq!(Some(b'o'), slice.next());
        assert_eq!(Some(b' '), slice.next());
        assert_eq!(Some(b'w'), slice.next());
        assert_eq!(Some(b'o'), slice.next());
        assert_eq!(Some(b'r'), slice.next());
        assert_eq!(None, slice.next());
    }
}
