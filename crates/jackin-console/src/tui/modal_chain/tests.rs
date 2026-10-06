// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::ModalChain;

#[test]
fn modal_chain_tracks_current_and_parents() {
    let mut chain = ModalChain::new();
    chain.open("root");
    chain.open_sub("child");
    assert_eq!(chain.current(), Some(&"child"));
    assert_eq!(chain.parents(), &["root"]);

    chain.pop();
    assert_eq!(chain.current(), Some(&"root"));
    assert!(chain.parents().is_empty());

    let root = chain.take_current().expect("active product modal");
    assert!(!chain.is_open());
    chain.set_current(root);
    chain.clear();
    assert!(!chain.is_open());
    assert!(chain.parents().is_empty());
}

#[test]
fn modal_chain_open_pair_stacks_parent_and_child() {
    let mut chain = ModalChain::new();
    chain.open_pair("parent", "child");
    assert_eq!(chain.current(), Some(&"child"));
    assert_eq!(chain.parents(), &["parent"]);
    assert!(chain.is_open());
}

#[test]
fn modal_chain_open_discards_the_existing_chain() {
    let mut chain = ModalChain::new();
    chain.open_pair("parent", "child");
    chain.open("fresh");
    assert_eq!(chain.current(), Some(&"fresh"));
    assert!(chain.parents().is_empty());
}
