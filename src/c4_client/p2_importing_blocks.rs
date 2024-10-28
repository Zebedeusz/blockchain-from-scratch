//! We are implementing our client with the most fundamental task, which is importing
//! blocks and headers. Full clients import entire blocks while light clients only import headers.

use std::collections::HashMap;
use std::u64;

use crate::c1_state_machine::{AccountedCurrency, AccountingTransaction};
use crate::c1_state_machine::{Balances, User};
use crate::c3_consensus::change_difficulty;
use crate::c3_consensus::Forked;
use crate::c3_consensus::Pow;
use crate::hash;

use super::BasicStorage;
use super::{Block, Consensus, FullClient, StateMachine, Storage};

/// A trait that represents the ability to import complete blocks of the chain.
///
/// The main method here is `import_block` but several other methods are provided
/// to access data about imported blocks.
pub trait ImportBlock<C: Consensus, SM: StateMachine> {
    /// Attempt to import a block.
    /// Returns whether the import was successful or not.
    fn import_block(&mut self, _: Block<C, SM>) -> bool;

    fn get_last_block(&self) -> Block<C, SM>;

    fn current_state(&self) -> SM::State;

    /// Retrieve the full body of an imported block.
    /// Returns None if the block is not known.
    fn get_block(&self, block_hash: u64) -> Option<Block<C, SM>>;

    // Retrieve the state associated with a given block.
    // Returns None if the block is not known.
    // fn get_state(&self, block_hash: u64) -> Option<SM::State>;

    // Check whether a given block is a leaf (aka tip) of the chain.
    // A leaf block has no known children.
    // Returns None if the block is not known.
    // fn is_leaf(&self, block_hash: u64) -> Option<bool>;

    // Get a list of all the leaf nodes in the chain.
    // fn all_leaves(&self) -> Vec<u64>;
}

impl<C, SM, FC, P, S> ImportBlock<C, SM> for FullClient<C, SM, FC, P, S>
where
    C: Consensus,
    SM: StateMachine,
    S: Storage<C, SM>,
    Block<C, SM>: std::hash::Hash + Clone,
    SM::State: std::hash::Hash,
    SM::Transition: std::hash::Hash,
{
    fn import_block(&mut self, block: Block<C, SM>) -> bool {
        let last_block: Block<C, SM> = self.storage.get_last_block();

        if block.header.height - 1 != last_block.header.height {
            return false;
        }
        if block.header.parent != hash(&last_block.header) {
            return false;
        }

        if !self
            .consensus_engine
            .validate(&last_block.header.consensus_digest, &block.header)
        {
            return false;
        }

        let mut current_state = self.storage.current_state();
        for tr in &block.body {
            current_state = SM::next_state(&current_state, &tr);
        }

        if hash(&current_state) != block.header.state_root {
            return false;
        }
        if hash(&block.body) != block.header.extrinsics_root {
            return false;
        }

        self.storage.set_state(current_state);
        self.storage.add_block(block);

        return true;
    }

    fn get_last_block(&self) -> Block<C, SM> {
        self.storage.get_last_block()
    }

    fn current_state(&self) -> <SM as StateMachine>::State {
        self.storage.current_state()
    }

    fn get_block(&self, block_hash: u64) -> Option<Block<C, SM>> {
        self.storage.get_block(block_hash)
    }

    // fn get_state(&self, block_hash: u64) -> Option<<SM as StateMachine>::State> {
    //     todo!("Exercise 3")
    // }

    // fn is_leaf(&self, block_hash: u64) -> Option<bool> {
    //     todo!("Exercise 4")
    // }

    // fn all_leaves(&self) -> Vec<u64> {
    //     todo!("Exercise 5")
    // }
}

// --- TESTS ---

impl std::hash::Hash for Block<Pow, AccountedCurrency> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.header.hash(state);
        self.body.hash(state);
    }
}

fn init_client_for_test() -> impl ImportBlock<Pow, AccountedCurrency> {
    let consensus_engine = Pow {
        threshold: u64::MAX / 10,
    };
    let state_machine = AccountedCurrency {};
    let fork_choice = change_difficulty(
        3,
        consensus_engine.threshold,
        consensus_engine.threshold / 10,
    );
    let transaction_pool = ();

    let storage: BasicStorage<Pow, AccountedCurrency> =
        BasicStorage::<Pow, AccountedCurrency>::new();

    FullClient {
        consensus_engine,
        state_machine,
        fork_choice,
        transaction_pool,
        storage,
    }
}

#[test]
fn cl2_import_valid_block() {
    let mut client = init_client_for_test();

    let mut block = Block::<Pow, AccountedCurrency>::genesis(
        &<AccountedCurrency as StateMachine>::State::default(),
    );

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let valid_next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    let imported = client.import_block(valid_next_block);
    assert!(imported);
}

#[test]
fn cl2_import_block_with_invalid_parent() {
    let mut client = init_client_for_test();

    let mut block = Block::<Pow, AccountedCurrency>::genesis(
        &<AccountedCurrency as StateMachine>::State::default(),
    );
    block.header.parent = 0;

    let imported = client.import_block(block);
    assert!(!imported);
}

#[test]
fn cl2_import_block_with_invalid_height() {
    let mut client = init_client_for_test();

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let mut next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    next_block.header.height = 17;

    let imported = client.import_block(next_block);
    assert!(!imported);
}

#[test]
fn cl2_import_block_with_invalid_state_root() {
    let mut client = init_client_for_test();

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let mut next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    next_block.header.state_root = 12;

    let imported = client.import_block(next_block);
    assert!(!imported);
}

#[test]
fn cl2_import_block_with_invalid_transaction_root() {
    let mut client = init_client_for_test();

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let mut next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    next_block.header.extrinsics_root = 12;

    let imported = client.import_block(next_block);
    assert!(!imported);
}

#[test]
fn cl2_import_block_with_invalid_seal() {
    let mut client = init_client_for_test();

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let mut next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    next_block.header.consensus_digest = u64::MAX;

    let imported = client.import_block(next_block);
    assert!(!imported);
}

#[test]
fn cl2_get_block_existing() {
    let client = init_client_for_test();

    let last_block = client.get_last_block();
    assert_eq!(last_block.header.height, 0);

    let block = client.get_block(hash(&last_block));
    assert!(block.is_some());
    assert_eq!(block.unwrap().header.height, last_block.header.height);
}

#[test]
fn cl2_get_block_not_existing() {
    let client = init_client_for_test();

    let block = client.get_block(12);
    assert!(!block.is_some());
}

#[test]
fn cl2_import_valid_block_and_get_it() {
    let mut client = init_client_for_test();

    let mut block = Block::<Pow, AccountedCurrency>::genesis(
        &<AccountedCurrency as StateMachine>::State::default(),
    );

    let current_state = client.current_state();
    let extrinsic = AccountingTransaction::Mint {
        minter: User::Alice,
        amount: 0,
    };

    let valid_next_block = client
        .get_last_block()
        .child(&current_state, vec![extrinsic]);

    let imported = client.import_block(valid_next_block.clone());
    assert!(imported);

    let block = client.get_block(hash(&valid_next_block));
    assert!(block.is_some());
    assert_eq!(block.unwrap().header.height, valid_next_block.header.height);
}
