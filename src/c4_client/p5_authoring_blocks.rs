//! We are now ready to give out client the ability to author blocks.
//! Clients that perform this task are usually known as "miners", "authors", or "authorities".

use std::string;

use crate::c1_state_machine::AccountedCurrency;
use crate::c1_state_machine::AccountingTransaction;
use crate::c3_consensus::Pow;
use crate::c3_consensus::{Consensus, Header};
use crate::c4_client::BasicStorage;
use crate::c4_client::Block;
use crate::hash;

use super::p3_fork_choice::Ghost;
use super::{
    p3_fork_choice::ForkChoice, p4_transaction_pool::PriorityPool,
    p4_transaction_pool::TransactionPool, FullClient, StateMachine, Storage,
};

// You may need to add trait bounds to make this work.
impl<C, SM, FC, P, S> FullClient<C, SM, FC, P, S>
where
    SM: StateMachine,
    C: Consensus,
    FC: ForkChoice<C, SM>,
    P: TransactionPool<SM>,
    S: Storage<C, SM>,
    Block<C, SM>: std::hash::Hash + Clone,
    SM::State: std::hash::Hash + Clone,
    SM::Transition: std::hash::Hash + Clone,
{
    /// Author a new block with the given transactions on top of the given parent
    /// and import the new block into the local database.
    pub fn author_and_import_manual_block(
        &mut self,
        transactions: Vec<SM::Transition>,
        parent_hash: u64,
    ) -> Result<(), ()> {
        // ---- author part

        let parent_block = S::get_block(&self.storage, parent_hash);
        if parent_block.is_none() {
            return Err(());
        }
        let parent_block = parent_block.unwrap();

        let parent_state = S::get_state(&self.storage, parent_block.header.state_root);
        if parent_state.is_none() {
            return Err(());
        }
        let mut new_state = parent_state.unwrap();

        for t in &transactions {
            new_state = SM::next_state(&new_state, &t);
        }

        let mut block = Block::<C, SM> {
            header: Header::<C::Digest> {
                parent: parent_hash,
                height: parent_block.header.height + 1,
                state_root: hash(&new_state),
                extrinsics_root: hash(&transactions),
                consensus_digest: <C as Consensus>::Digest::default(),
            },
            body: transactions,
        };

        let sealed_header = C::seal(
            &self.consensus_engine,
            &parent_block.header.consensus_digest,
            block.header,
        );
        if sealed_header.is_none() {
            return Err(());
        }
        block.header = sealed_header.unwrap();

        // ---- import part

        S::add_block(&mut self.storage, block.clone());
        S::set_state(&mut self.storage, new_state.clone());

        FC::import_hook(&mut self.fork_choice, block.clone());

        return Ok(());
    }

    /// Author a new block with the transactions from the pool on top of the "best" block
    /// and import the new block into the local database.
    pub fn author_and_import_automatic_block(&mut self) -> Result<(), ()> {
        // ---- author part

        // parent block from ForkChoice instead
        let parent_block_hash = FC::best_block(&self.fork_choice).ok_or(())?;
        let parent_block = S::get_block(&self.storage, parent_block_hash).ok_or(())?;
        let mut new_state = S::get_state(&self.storage, parent_block.header.state_root).ok_or(())?;

        // transactions from the pool instead
        let mut used_transactions = Vec::new();
        for _ in 0..10 {
            let transaction = P::next_from_pool(&mut self.transaction_pool);
            if transaction.is_none() {
                break;
            }
            let transaction = transaction.unwrap();

            new_state = SM::next_state(&new_state, &transaction);
            used_transactions.push(transaction);
        }
        if used_transactions.len() == 0 {
            return Ok(());
        }

        let mut block = Block::<C, SM> {
            header: Header::<C::Digest> {
                parent: parent_block_hash,
                height: parent_block.header.height + 1,
                state_root: hash(&new_state),
                extrinsics_root: hash(&used_transactions),
                consensus_digest: <C as Consensus>::Digest::default(),
            },
            body: used_transactions.clone(),
        };

        let sealed_header = C::seal(
            &self.consensus_engine,
            &parent_block.header.consensus_digest,
            block.header,
        );
        if sealed_header.is_none() {
            return Err(());
        }
        block.header = sealed_header.unwrap();

        // remove from the pool transactions that are included in the block
        for tx in used_transactions {
            P::remove(&mut self.transaction_pool, tx);
        }

        // ---- import part

        S::add_block(&mut self.storage, block.clone());
        S::set_state(&mut self.storage, new_state.clone());
        S::set_current_state(&mut self.storage, new_state.clone());
        S::set_last_block(&mut self.storage, block.clone());

        FC::import_hook(&mut self.fork_choice, block.clone());

        return Ok(());
    }
}

// --- TESTS ---

fn prioritizer(t: AccountingTransaction) -> u64 {
    match t {
        AccountingTransaction::Mint { minter, amount } => return 5,
        AccountingTransaction::Burn { burner, amount } => return 4,
        AccountingTransaction::Transfer {
            sender,
            receiver,
            amount,
        } => return 6,
    }
}

fn prioritizer_same_prio(t: AccountingTransaction) -> u64 {
    match t {
        AccountingTransaction::Mint { minter, amount } => return 5,
        AccountingTransaction::Burn { burner, amount } => return 5,
        AccountingTransaction::Transfer {
            sender,
            receiver,
            amount,
        } => return 5,
    }
}

type AccountingTransactionPrioritizer = fn(t: AccountingTransaction) -> u64;

fn init_client_for_test(
    prioritizer: fn(AccountingTransaction) -> u64,
) -> FullClient<
    Pow,
    AccountedCurrency,
    Ghost,
    PriorityPool<AccountedCurrency, AccountingTransactionPrioritizer>,
    BasicStorage<Pow, AccountedCurrency>,
> {
    let consensus_engine = Pow {
        threshold: u64::MAX / 10,
    };
    let state_machine = AccountedCurrency {};
    let fork_choice = Ghost::default();
    let transaction_pool: PriorityPool<AccountedCurrency, AccountingTransactionPrioritizer> =
        PriorityPool::new(prioritizer, 4);
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

mod cl5_manual_authoring {
    use super::*;
    use crate::c1_state_machine::p4_accounted_currency::BalancesB;
    use crate::{c1_state_machine::User, c4_client::p1_data_structure::Block};
    use std::collections::HashMap;

    #[test]
    fn adds_the_block_and_adds_new_state() {
        // given
        let mut client = init_client_for_test(prioritizer);

        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];

        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());
        client.storage.set_state(genesis_state);

        // when
        assert!(client
            .author_and_import_manual_block(transactions.clone(), hash(&previous_block))
            .is_ok());

        // then
        assert_eq!(client.storage.blocks_map.len(), 2);
        assert_eq!(client.storage.states_map.len(), 2);

        let mut state_hash = 0;
        for (block_hash, block) in &client.storage.blocks_map {
            if *block_hash != hash(&previous_block) {
                assert_eq!(block.body, transactions);
                assert_eq!(block.header.parent, hash(&previous_block));
                assert_eq!(block.header.height, 1);

                state_hash = block.header.state_root;
            }
        }

        let state = client.storage.get_state(state_hash);
        assert!(state.is_some());
        let state = state.unwrap();

        let user_alice = User::Alice;
        assert!(state
            .balances
            .get(&user_alice)
            .is_some_and(|alices_balance| *alices_balance == 12));

        let user_bob = User::Bob;
        assert!(state
            .balances
            .get(&user_bob)
            .is_some_and(|bobs_balance| *bobs_balance == 16));
    }

    #[test]
    fn fails_if_state_not_in_storage() {
        // given
        let mut client = init_client_for_test(prioritizer);

        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];

        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());

        // when + then
        assert!(client
            .author_and_import_manual_block(transactions.clone(), hash(&previous_block))
            .is_err());
    }

    #[test]
    fn fails_if_block_not_in_storage() {
        // given
        let mut client = init_client_for_test(prioritizer);

        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];

        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.set_state(genesis_state);

        // when + then
        assert!(client
            .author_and_import_manual_block(transactions.clone(), hash(&previous_block))
            .is_err());
    }
}

mod cl5_automatic_authoring {
    use super::*;
    use crate::c1_state_machine::p4_accounted_currency::BalancesB;
    use crate::c4_client::p2_importing_blocks::ImportBlock;
    use crate::{c1_state_machine::User, c4_client::p1_data_structure::Block};
    use std::collections::HashMap;

    #[test]
    fn uses_internal_transactions_and_best_block_then_sets_current_state_and_block() {
        // --- GIVEN
        let mut client = init_client_for_test(prioritizer_same_prio);

        // adding transactions to the pool
        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];
        for tx in &transactions {
            assert!(client.transaction_pool.try_insert(tx.clone()));
        }
        assert_eq!(client.transaction_pool.size(), transactions.len());

        // adding block and state to storage
        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());
        client.storage.set_state(genesis_state.clone());
        client.storage.set_current_state(genesis_state.clone());
        client.storage.set_last_block(previous_block.clone());

        // adding block to fork choice
        client.fork_choice.import_hook(previous_block.clone());

        // --- WHEN
        let res = client.author_and_import_automatic_block();

        // --- THEN
        assert!(res.is_ok());

        // storage was updated
        // storage is initialized with one genesis block, then one more was added in the "given" step, then one was added during block authoring
        assert_eq!(client.storage.blocks_map.len(), 3);
        assert_eq!(client.storage.states_map.len(), 3);

        let last_block = client.storage.get_last_block();
        assert_eq!(last_block.body, transactions);
        assert_eq!(last_block.header.parent, hash(&previous_block));
        assert_eq!(last_block.header.height, 1);

        let state = client.current_state();
        let user_alice = User::Alice;
        assert!(state
            .balances
            .get(&user_alice)
            .is_some_and(|alices_balance| *alices_balance == 12));

        let user_bob = User::Bob;
        assert!(state
            .balances
            .get(&user_bob)
            .is_some_and(|bobs_balance| *bobs_balance == 16));

        // fork choice best block is the new one
        assert!(client
            .fork_choice
            .best_block()
            .is_some_and(|b| b == hash(&last_block)));

        // transactions were removed from the pool
        assert_eq!(client.transaction_pool.size(), 0);
    }

    #[test]
    fn no_best_block_from_fork_choice() {
        // --- GIVEN
        let mut client = init_client_for_test(prioritizer_same_prio);

        // adding transactions to the pool
        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];
        for tx in &transactions {
            assert!(client.transaction_pool.try_insert(tx.clone()));
        }
        assert_eq!(client.transaction_pool.size(), transactions.len());

        // adding block and state to storage
        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());
        client.storage.set_state(genesis_state.clone());
        client.storage.set_current_state(genesis_state.clone());
        client.storage.set_last_block(previous_block.clone());

        // no block in fork choice

        // --- WHEN
        let res = client.author_and_import_automatic_block();

        // --- THEN
        assert!(res.is_err());

        // storage not updated
        assert_eq!(client.storage.blocks_map.len(), 2);
        assert_eq!(client.storage.states_map.len(), 2);

        let state = client.current_state();
        assert_eq!(state.balances, genesis_state.balances)
    }

    #[test]
    fn no_parent_block_in_storage() {
        // --- GIVEN
        let mut client = init_client_for_test(prioritizer_same_prio);

        // adding transactions to the pool
        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];
        for tx in &transactions {
            assert!(client.transaction_pool.try_insert(tx.clone()));
        }
        assert_eq!(client.transaction_pool.size(), transactions.len());

        // adding block and state to storage
        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.set_state(genesis_state.clone());
        client.storage.set_current_state(genesis_state.clone());

        // adding block to fork choice
        client.fork_choice.import_hook(previous_block.clone());

        // --- WHEN
        let res = client.author_and_import_automatic_block();

        // --- THEN
        assert!(res.is_err());

        // storage not updated
        assert_eq!(client.storage.blocks_map.len(), 1);
        assert_eq!(client.storage.states_map.len(), 2);

        let state = client.current_state();
        assert_eq!(state.balances, genesis_state.balances)
    }

    #[test]
    fn no_state_for_parent_block_in_storage() {
        // --- GIVEN
        let mut client = init_client_for_test(prioritizer_same_prio);

        // adding transactions to the pool
        let transactions = vec![
            AccountingTransaction::Mint {
                minter: User::Alice,
                amount: 10,
            },
            AccountingTransaction::Mint {
                minter: User::Bob,
                amount: 20,
            },
            AccountingTransaction::Burn {
                burner: User::Bob,
                amount: 2,
            },
            AccountingTransaction::Transfer {
                sender: User::Bob,
                receiver: User::Alice,
                amount: 2,
            },
        ];
        for tx in &transactions {
            assert!(client.transaction_pool.try_insert(tx.clone()));
        }
        assert_eq!(client.transaction_pool.size(), transactions.len());

        // adding block and state to storage
        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());
        client.storage.set_last_block(previous_block.clone());

        // adding block to fork choice
        client.fork_choice.import_hook(previous_block.clone());

        // --- WHEN
        let res = client.author_and_import_automatic_block();

        // --- THEN
        assert!(res.is_err());

        // storage not updated
        assert_eq!(client.storage.blocks_map.len(), 2);
        assert_eq!(client.storage.states_map.len(), 1);
    }

    #[test]
    fn no_transactions_in_tx_pool() {
        // --- GIVEN
        let mut client = init_client_for_test(prioritizer_same_prio);

        // no transactions in the pool
        assert_eq!(client.transaction_pool.size(), 0);

        // adding block and state to storage
        let genesis_state = BalancesB {
            balances: HashMap::from([(User::Charlie, 1)]),
        };
        let previous_block = Block::<Pow, AccountedCurrency>::genesis(&genesis_state);

        client.storage.add_block(previous_block.clone());
        client.storage.set_state(genesis_state.clone());
        client.storage.set_current_state(genesis_state.clone());
        client.storage.set_last_block(previous_block.clone());

        // adding block to fork choice
        client.fork_choice.import_hook(previous_block.clone());

        // --- WHEN
        let res = client.author_and_import_automatic_block();

        // --- THEN
        assert!(res.is_ok());

        // storage not updated
        assert_eq!(client.storage.blocks_map.len(), 2);
        assert_eq!(client.storage.states_map.len(), 2);

        let state = client.current_state();
        assert_eq!(state.balances, genesis_state.balances)
    }
}
