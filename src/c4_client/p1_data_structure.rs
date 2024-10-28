//! Before we implement any serious  methods on our client, we must, create the `Block` and `Header`
//! data structures one last time like we did in Chapter 1. the logic you wrote there
//! will be useful here as well and can probably be reused to some extent.
//!
//! Throughout the Blockchain chapter, we created a blockchain data structure that had:
//! 1. a built-in addition accumulator state machine
//! 2. A built-in pow consensus mechanism
//!
//! In the State Machine and Consensus chapters, we designed abstractions over both
//! the state machine and the consensus. We also implemented several examples of each
//! trait.
//!
//! This will be the last time we have to write this blockchain data structure,
//! because this time it will be fully generic over both the state machine and consensus
//! logic, thanks to our traits.
//!
//! This abstraction is the key idea behind blockchain _frameworks_ like Substrate or the Cosmos SDK.

use crate::hash;

use super::p4_transaction_pool::TransactionPool;
use super::{Consensus, ForkChoice, Header, StateMachine, Storage};

use super::FullClient;
type Hash = u64;

impl<Digest> Header<Digest>
where
    Digest: Default + std::hash::Hash,
{
    /// Returns a new valid genesis header.
    fn genesis(genesis_state_root: Hash) -> Self {
        return Header {
            parent: 0,
            height: 0,
            state_root: genesis_state_root,
            extrinsics_root: hash(&Vec::<u8>::new()),
            consensus_digest: Digest::default(),
        };
    }

    /// Create and return a valid child header.
    fn child(&self, state_root: Hash, extrinsics_root: Hash) -> Self {
        return Header {
            parent: hash(&self),
            height: self.height + 1,
            state_root,
            extrinsics_root,
            consensus_digest: Digest::default(),
        };
    }

    /// Verify a single child header.
    fn verify_child(&self, child: &Self) -> bool {
        if child.parent != hash(&self) {
            return false;
        }
        if child.height != self.height + 1 {
            return false;
        }
        return true;
    }

    /// Verify that all the given headers form a valid chain from this header to the tip.
    fn verify_sub_chain(&self, chain: &[Self]) -> bool {
        let mut parent = self;
        for i in 0..chain.len() {
            let next_header = chain.get(i).unwrap();
            if !parent.verify_child(&next_header) {
                return false;
            }
            parent = next_header;
        }
        return true;
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Block<C: Consensus, SM: StateMachine> {
    pub header: Header<C::Digest>,
    pub body: Vec<SM::Transition>,
}

impl<C: Consensus, SM: StateMachine> Block<C, SM>
where
    C::Digest: Default + std::hash::Hash,
    SM::State: std::hash::Hash + Clone,
    SM::Transition: std::hash::Hash + Clone,
{
    /// Returns a new valid genesis block. By convention this block has no extrinsics.
    pub fn genesis(genesis_state: &SM::State) -> Self {
        return Block {
            header: Header::genesis(hash(&genesis_state)),
            body: Vec::<SM::Transition>::new(),
        };
    }

    /// Create and return a valid child block.
    pub fn child(&self, pre_state: &SM::State, extrinsics: Vec<SM::Transition>) -> Self {
        let mut new_state = pre_state.clone();
        for e in extrinsics.clone() {
            new_state = SM::next_state(&new_state.clone(), &e)
        }

        return Block {
            header: Header::child(&self.header, hash(&new_state), hash(&extrinsics)),
            body: extrinsics,
        };
    }

    /// Verify that all the given blocks form a valid chain from this block to the tip.
    pub fn verify_sub_chain(&self, pre_state: &SM::State, chain: &[Self]) -> bool {
        let mut headers = Vec::new();

        let mut curr_state = pre_state.clone();
        for i in 0..chain.len() {
            let next_block = chain.get(i).unwrap();

            for extr in &next_block.body.clone() {
                curr_state = SM::next_state(&curr_state.clone(), &extr)
            }

            if hash(&curr_state) != next_block.header.state_root {
                return false;
            }

            if hash(&next_block.body) != next_block.header.extrinsics_root {
                return false;
            }

            headers.push(next_block.header.clone());
        }

        return self.header.verify_sub_chain(&headers);
    }
}

/// Create and return a block chain that is n blocks long starting from the given genesis state.
/// The blocks should not contain any transactions.
fn create_empty_chain<C, SM>(n: u64, genesis_state: &SM::State) -> Vec<Block<C, SM>>
where
    C: Consensus,
    SM: StateMachine,
    C::Digest: Default + std::hash::Hash,
    SM::State: std::hash::Hash + Clone,
    SM::Transition: std::hash::Hash + Clone,
    Block<C, SM>: Clone,
{
    let mut chain = Vec::<Block<C, SM>>::new();

    let g = Block::<C, SM>::genesis(genesis_state);
    chain.push(g.clone());

    for _ in 1..n {
        let new_block = Block::child(&g, &genesis_state.clone(), Vec::new());
        chain.push(new_block);
    }

    return chain;
}

// To wrap this section up, we will implement the first two simple methods on our client.
// These methods simply create a new instance of the client initialized with a proper
// genesis block.
impl<C, SM, FC, P, S> FullClient<C, SM, FC, P, S>
where
    SM: StateMachine,
    C: Consensus,
    FC: ForkChoice<C>,
    P: TransactionPool<SM>,
    S: Storage<C, SM>,
    Block<C, SM>: std::hash::Hash,
{
    fn new(
        genesis_state: SM::State,
        state_machine: SM,
        consensus_engine: C,
        fork_choice: FC,
        transaction_pool: P,
        mut storage: S,
    ) -> Self {
        storage.set_state(genesis_state);
        let mut client = FullClient {
            consensus_engine,
            state_machine,
            fork_choice,
            transaction_pool,
            storage,
        };
        return client;
    }
}

// The default client is initialized with the default genesis state.
// Depending on the state machine definition there may not _be_ a default
// genesis state. There is only a default client when there is also a
// default genesis state.
impl<C, SM, FC, P, S> Default for FullClient<C, SM, FC, P, S>
where
    C: Consensus,
    SM: StateMachine,
    FC: ForkChoice<C>,
    S: Storage<C, SM>,
    Block<C, SM>: std::hash::Hash,
{
    fn default() -> Self {
        todo!("Exerise 10")
    }
}
