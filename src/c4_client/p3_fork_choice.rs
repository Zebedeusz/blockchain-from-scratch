//! As a blockchain node watches the chain evolve, it must constantly be assessing which chain
//! is currently the best chain. We explored the concept of fork-choice briefly in the Blockchain chapter.
//!
//! The concepts are identical here, but now that we have a client tracking a proper block database,
//! we can explore more advanced fork choice algorithms. In particular, we can now explore GHOST.

use std::collections::BTreeMap;
use std::collections::HashMap;

use super::p1_data_structure::Block;
use super::p2_importing_blocks::ImportBlock;
use super::{BasicStorage, Storage};
use super::{Consensus, FullClient};
use crate::c1_state_machine::AccountedCurrency;
use crate::c1_state_machine::StateMachine;
use crate::c3_consensus::{ConsensusAuthority, Pow, SimplePoa};
use crate::c4_client::Header;
use crate::hash;

/// A means for a blockchain client to decide which chain is best among the many
/// that it potentially knows about.
///
/// Our client is generic over this bit of logic just like it is generic over the state machine and
/// consensus.
///
/// Some implementations are light and just make a quick comparison, like the longest chain rule.
/// Others are more complex and associate additional logic with block import, like GHOST.
pub trait ForkChoice<C: Consensus, SM: StateMachine>
where
    Block<C, SM>: std::hash::Hash,
{
    /// Return the hash of the best block currently known according to this fork choice rule.
    fn best_block(&self) -> Option<u64>;

    /// Perform some bookkeeping activities when importing a new block.
    fn import_hook(&mut self, block: Block<C, SM>);
}

/// The chain with the highest block height is the best
/// TODO: take another look at the implementation
pub struct LongestChain {
    best_header_height: u64,
    best_header_hash: u64,
}

impl<C: Consensus, SM: StateMachine> ForkChoice<C, SM> for LongestChain
where
    Block<C, SM>: std::hash::Hash,
{
    fn best_block(&self) -> Option<u64> {
        return Some(self.best_header_hash);
    }

    fn import_hook(&mut self, block: Block<C, SM>) {
        if block.header.height > self.best_header_height {
            self.best_header_height = block.header.height;
            self.best_header_hash = hash(&block);
        }
    }
}

impl Default for LongestChain {
    fn default() -> Self {
        Self {
            best_header_height: 0,
            best_header_hash: 0,
        }
    }
}

/// The chain with the most accumulated proof of work is the best.
/// This fork choice rule only makes sense with the PoW consensus engine
/// and the generics reflect that.
pub struct HeaviestChain {
    chain_weight_to_last_block_hash: BTreeMap<u64, u64>,
}

impl<SM: StateMachine> ForkChoice<Pow, SM> for HeaviestChain
where
    Block<Pow, SM>: std::hash::Hash,
{
    fn best_block(&self) -> Option<u64> {
        self.chain_weight_to_last_block_hash
            .iter()
            .last()
            .map(|(_, &v)| v)
    }

    fn import_hook(&mut self, block: Block<Pow, SM>) {
        let chain_weight = self
            .chain_weight_to_last_block_hash
            .iter()
            .find_map(|(k, v)| {
                if *v == block.header.parent {
                    Some(*k)
                } else {
                    None
                }
            });
        match chain_weight {
            Some(chain_weight_v) => {
                self.chain_weight_to_last_block_hash.remove(&chain_weight_v);
                self.chain_weight_to_last_block_hash
                    .insert(chain_weight_v + block.header.consensus_digest, hash(&block));
            }
            None => {
                self.chain_weight_to_last_block_hash
                    .insert(block.header.consensus_digest, hash(&block));
            }
        }
    }
}

impl Default for HeaviestChain {
    fn default() -> Self {
        Self {
            chain_weight_to_last_block_hash: BTreeMap::new(),
        }
    }
}

/// The chain with the most signatures from the Alice authority is the best.
/// This fork choice rule only makes sense with the PoA consensus engine
/// and the generics reflect that.
pub struct MostAliceSigs {
    chains_alice_sigs_to_last_block_hash: BTreeMap<u64, u64>,
}

impl<SM: StateMachine> ForkChoice<SimplePoa, SM> for MostAliceSigs
where
    Block<SimplePoa, SM>: std::hash::Hash,
{
    fn best_block(&self) -> Option<u64> {
        self.chains_alice_sigs_to_last_block_hash
            .iter()
            .last()
            .map(|(_, &v)| v)
    }

    fn import_hook(&mut self, block: Block<SimplePoa, SM>) {
        let mut has_alice_sig = false;
        if block.header.consensus_digest == ConsensusAuthority::Alice {
            has_alice_sig = true;
        }
        let chain_alice_sigs =
            self.chains_alice_sigs_to_last_block_hash
                .iter()
                .find_map(|(k, v)| {
                    if *v == block.header.parent {
                        Some(*k)
                    } else {
                        None
                    }
                });
        match chain_alice_sigs {
            Some(chain_alice_sigs_v) => {
                if !has_alice_sig {
                    return;
                }
                self.chains_alice_sigs_to_last_block_hash
                    .remove(&chain_alice_sigs_v);
                self.chains_alice_sigs_to_last_block_hash
                    .insert(chain_alice_sigs_v + 1, hash(&block));
            }
            None => {
                let mut to_insert = 0;
                if has_alice_sig {
                    to_insert = 1;
                }
                self.chains_alice_sigs_to_last_block_hash
                    .insert(to_insert, hash(&block));
            }
        }
    }
}

impl Default for MostAliceSigs {
    fn default() -> Self {
        Self {
            chains_alice_sigs_to_last_block_hash: BTreeMap::new(),
        }
    }
}

/// In the Greedy Heaviest Observed Subtree rule, the fork choice is iterative.
/// You start from the genesis block, and at each fork, you choose the side of the fork
/// that has the most accumulated proof of work on _all_ of its descendants.
pub struct Ghost {
    cum_chain_weight_to_blocks_in_chain: BTreeMap<u64, Vec<u64>>,
}

impl<SM: StateMachine> ForkChoice<Pow, SM> for Ghost
where
    Block<Pow, SM>: std::hash::Hash,
{
    fn best_block(&self) -> Option<u64> {
        self.cum_chain_weight_to_blocks_in_chain
            .iter()
            .last()
            .map(|(_, &ref v)| v.last().cloned())?
    }

    fn import_hook(&mut self, block: Block<Pow, SM>) {
        // a header.parent may be in the middle of a chain
        // so find header.parent in all of the stored chains
        // append the work done in the provided header to that chain
        // if chain not found - create new entry in the tree

        let maybe_found_chain = self
            .cum_chain_weight_to_blocks_in_chain
            .iter_mut()
            .find(|(_, v)| v.contains(&block.header.parent))
            .map(|(weight, chain)| (*weight, chain.clone())); // Clone the chain to avoid the borrow conflict

        match maybe_found_chain {
            Some((weight, mut chain)) => {
                chain.push(hash(&block));

                let new_weight = weight + block.header.consensus_digest;
                let new_chain = chain.clone();

                self.cum_chain_weight_to_blocks_in_chain.remove(&weight);
                self.cum_chain_weight_to_blocks_in_chain
                    .insert(new_weight, new_chain);
            }
            None => {
                self.cum_chain_weight_to_blocks_in_chain
                    .insert(block.header.consensus_digest, vec![hash(&block)]);
            }
        }
    }
}

impl Default for Ghost {
    fn default() -> Self {
        Self {
            cum_chain_weight_to_blocks_in_chain: BTreeMap::new(),
        }
    }
}

/// Finally, we will provide a convenience method directly on our client that simply calls
/// into the corresponding method on the ForkChoice rule. You may need to add some trait
/// bounds to make this work.
impl<C, SM, FC, P, S> FullClient<C, SM, FC, P, S>
where
    C: Consensus,
    SM: StateMachine,
    FC: ForkChoice<C, SM>,
    Block<C, SM>: std::hash::Hash,
{
    /// Return the hash of the best block currently known to the client
    fn best_block(&self) -> u64 {
        if let Some(v) = FC::best_block(&self.fork_choice) {
            v
        } else {
            0
        }
    }
}

// --- TESTS ---

fn block_from_header<C: Consensus, SM: StateMachine>(header: Header<C::Digest>) -> Block<C, SM> {
    return Block::<C, SM> {
        header,
        body: Vec::new(),
    };
}

fn init_client_for_test() -> impl ImportBlock<Pow, AccountedCurrency> {
    let consensus_engine = Pow {
        threshold: u64::MAX / 10,
    };
    let state_machine = AccountedCurrency {};
    let fork_choice = LongestChain::default();
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

mod cl3_longest {
    use super::*;

    #[test]
    fn cl3_longest_chain_imports_first_header() {
        let mut fork_choice = LongestChain::default();

        let block = Block::<Pow, AccountedCurrency> {
            header: Header {
                height: 1,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <LongestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block.clone(),
        );

        let best_block =
            <LongestChain as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block), best_block.unwrap());
    }

    #[test]
    fn cl3_longest_chain_does_not_import_block_from_shorter_chain() {
        let mut fork_choice = LongestChain::default();

        let block = Block::<Pow, AccountedCurrency> {
            header: Header {
                height: 5,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <LongestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block.clone(),
        );

        let block_shorter = Block::<Pow, AccountedCurrency> {
            header: Header {
                height: 2,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <LongestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_shorter.clone(),
        );

        let best_block =
            <LongestChain as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block), best_block.unwrap());
    }
}

mod cl3_heaviest {
    use super::*;

    #[test]
    fn cl3_heaviest_chain_imports_first_header() {
        let mut fork_choice = HeaviestChain::default();

        let block = Block::<Pow, AccountedCurrency>::default();
        <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block.clone(),
        );

        let best_block =
            <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block), best_block.unwrap());
    }

    #[test]
    fn cl3_heaviest_chain_does_not_import_header_from_lighter_chain() {
        let mut fork_choice = HeaviestChain::default();

        // 1st chain - heavier
        let block_heavier = Block::<Pow, AccountedCurrency> {
            header: Header {
                parent: 111,
                height: 1,
                consensus_digest: 12,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_heavier.clone(),
        );

        // 2nd chain - lighter
        let block_lighter = Block::<Pow, AccountedCurrency> {
            header: Header {
                parent: 120,
                height: 1,
                consensus_digest: 10,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_lighter.clone(),
        );

        let mut best_block =
            <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block_heavier), best_block.unwrap());

        let block2 = Block::<Pow, AccountedCurrency> {
            header: Header {
                parent: hash(&best_block),
                height: 2,
                consensus_digest: 15,
                ..Default::default()
            },
            body: Vec::new(),
        };
        <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block2.clone(),
        );

        best_block =
            <HeaviestChain as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block2), best_block.unwrap());
    }
}

#[test]
fn btreemaptest() {
    let solar_distance = BTreeMap::from([
        (12, "Mercury"),
        (11, "Venus"),
        (14144, "Earth"),
        (1, "Mars"),
    ]);

    let mx = BTreeMap::from([(1, "a"), (12, "b"), (2, "c"), (3, "d")]);

    solar_distance
        .last_key_value()
        .inspect(|(k, v)| assert!(v.eq(&&"Earth")));
}

mod cl3_most_alice_sigs {
    use super::MostAliceSigs;
    use super::*;

    #[test]
    fn best_block_is_one_with_most_alice_sigs() {
        let mut fork_choice = MostAliceSigs::default();

        // 1st chain - 2 Alice sigs
        let header_alice_1: Header<<SimplePoa as Consensus>::Digest> = Header {
            height: 1,
            consensus_digest: ConsensusAuthority::Alice,
            ..Default::default()
        };
        let block_alice_1 = block_from_header(header_alice_1.clone());
        let header_alice_2: Header<<SimplePoa as Consensus>::Digest> = Header {
            height: 2,
            consensus_digest: ConsensusAuthority::Alice,
            parent: hash(&block_alice_1),
            ..Default::default()
        };
        let block_alice_2 = block_from_header(header_alice_2);

        <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_alice_1.clone(),
        );
        <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_alice_2.clone(),
        );

        // 2nd chain - 1 Alice sig
        let header_alice_3: Header<<SimplePoa as Consensus>::Digest> = Header {
            parent: 12,
            height: 1,
            consensus_digest: ConsensusAuthority::Alice,
            ..Default::default()
        };
        let block_alice_3 = block_from_header(header_alice_3);
        <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_alice_3.clone(),
        );

        // 3rd chain - 3 Bob sigs
        let header_bob: Header<<SimplePoa as Consensus>::Digest> = Header {
            parent: 11,
            height: 1,
            consensus_digest: ConsensusAuthority::Bob,
            ..Default::default()
        };
        let mut block_bob = block_from_header(header_bob.clone());
        <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_bob.clone(),
        );
        for _ in 0..2 {
            let header: Header<<SimplePoa as Consensus>::Digest> = Header {
                parent: hash(&block_bob),
                height: header_bob.height + 1,
                consensus_digest: ConsensusAuthority::Bob,
                ..Default::default()
            };
            let block = block_from_header(header.clone());
            <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::import_hook(
                &mut fork_choice,
                block.clone(),
            );
            block_bob = block;
        }

        let best_block = <MostAliceSigs as ForkChoice<SimplePoa, AccountedCurrency>>::best_block(
            &mut fork_choice,
        );

        assert!(best_block.is_some());
        assert_eq!(hash(&block_alice_2), best_block.unwrap());
    }
}

mod cl3_ghost {
    use super::*;

    #[test]
    fn best_block_is_one_with_highest_cum_work() {
        let mut fork_choice = Ghost::default();

        // 1st chain - 2 blocks with 10 work
        let header_1: Header<<Pow as Consensus>::Digest> = Header {
            parent: 111,
            height: 1,
            state_root: 0,
            extrinsics_root: 0,
            consensus_digest: 2,
        };
        let block_1 = block_from_header(header_1.clone());

        let mut header_2: Header<<Pow as Consensus>::Digest> = Header {
            parent: hash(&block_1),
            height: 2,
            state_root: 0,
            extrinsics_root: 0,
            consensus_digest: 8,
        };
        let block_2 = block_from_header(header_2.clone());

        <Ghost as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_1.clone(),
        );

        <Ghost as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_2.clone(),
        );

        // 2nd chain - 1 block with 12 work
        let header_3: Header<<Pow as Consensus>::Digest> = Header {
            parent: 12,
            height: 1,
            state_root: 0,
            extrinsics_root: 0,
            consensus_digest: 12,
        };
        let block_3 = block_from_header(header_3.clone());

        <Ghost as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_3.clone(),
        );

        // 3rd chain - 3 blocks with 6 work
        let mut header_4: Header<<Pow as Consensus>::Digest> = Header {
            parent: 11,
            height: 1,
            state_root: 0,
            extrinsics_root: 0,
            consensus_digest: 2,
        };
        let block_4 = block_from_header(header_4.clone());

        <Ghost as ForkChoice<Pow, AccountedCurrency>>::import_hook(
            &mut fork_choice,
            block_4.clone(),
        );

        for _ in 0..2 {
            let header: Header<<Pow as Consensus>::Digest> = Header {
                parent: hash(&block_4),
                height: 1,
                state_root: 0,
                extrinsics_root: 0,
                consensus_digest: 2,
            };
            let block = block_from_header(header.clone());
            <Ghost as ForkChoice<Pow, AccountedCurrency>>::import_hook(
                &mut fork_choice,
                block.clone(),
            );
            header_4 = header;
        }

        let best_block =
            <Ghost as ForkChoice<Pow, AccountedCurrency>>::best_block(&mut fork_choice);

        assert!(best_block.is_some());
        assert_eq!(hash(&block_3), best_block.unwrap());
    }
}
