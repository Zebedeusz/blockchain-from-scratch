//! In this section our client will begin maintaining a transaction pool.
//!
//! The transaction pool is where transactions that are not yet included in the blockchain
//! are queued before they are inserted into blocks.
//!
//! Maintaining a transaction pool includes:
//! * Accepting transactions from users
//! * Removing transactions that are included in blocks as they are imported
//! * Making the current transactions available for a block authoring process
//! * Re-queueing transactions from orphaned blocks when re-orgs happen (This one happens IRL; might not cover it in BFS; TBD)

use crate::c1_state_machine::{AccountedCurrency, AccountingTransaction};
use std::collections::VecDeque;

use super::{FullClient, StateMachine};

/// An abstraction over the notion of transaction pool.
pub trait TransactionPool<SM: StateMachine> {
    type Iterator<'a>: Iterator<Item = &'a SM::Transition>
    where
        Self: 'a,
        SM: 'a;

    /// Try to add a new transaction to the pool. Return whether the operation succeeded.
    fn try_insert(&mut self, t: SM::Transition) -> bool;

    /// Remove a given transaction from the pool if it exists there.
    fn remove(&mut self, t: SM::Transition);

    /// Get the total number of transactions in the pool
    fn size(&self) -> usize;

    /// Check whether the specified transaction exists in the pool
    fn contains(&self, t: SM::Transition) -> bool;

    /// The notion of next is opaque and implementation dependent.
    /// Different chains prioritize transactions differently, usually by economic means.
    fn next_from_pool(&mut self) -> Option<SM::Transition>;

    /// Return an iterator over all transactions in the pool.
    fn iter<'a>(&'a self) -> Self::Iterator<'a>;
}

// First we add some new user-facing methods to the client.
// These are basically wrappers around methods that the pool itself provides.
impl<C, SM, FC, P, S> FullClient<C, SM, FC, P, S>
where
    SM: StateMachine,
    P: TransactionPool<SM>,
{
    /// Submit a transaction to the client's transaction pool to hopefully
    /// be included in a future block.
    pub fn submit_transaction(&mut self, t: SM::Transition) {
        P::try_insert(&mut self.transaction_pool, t);
    }

    /// Get the total number of transactions in the node's
    /// transaction pool.
    pub fn pool_size(&self) -> usize {
        P::size(&self.transaction_pool)
    }

    /// Check whether a a given transaction is in the client's transaction pool.
    pub fn pool_contains(&self, t: SM::Transition) -> bool {
        P::contains(&self.transaction_pool, t)
    }
}

/// A simple state machine that is just a first-in-first-out queue.
pub struct SimplePool<SM: StateMachine>(VecDeque<SM::Transition>);

impl<SM: StateMachine> TransactionPool<SM> for SimplePool<SM>
where
    <SM as StateMachine>::Transition: PartialEq<<SM as StateMachine>::Transition> + Clone,
{
    type Iterator<'a> = std::collections::vec_deque::Iter<'a, SM::Transition>
    where
        SM: 'a;

    fn try_insert(&mut self, t: <SM as StateMachine>::Transition) -> bool {
        self.0.push_back(t);
        true
    }

    fn remove(&mut self, t: <SM as StateMachine>::Transition) {
        self.0.retain(|e| *e != t);
    }

    fn size(&self) -> usize {
        return self.0.len();
    }

    fn contains(&self, t: <SM as StateMachine>::Transition) -> bool {
        self.0.contains(&t)
    }

    fn next_from_pool(&mut self) -> Option<SM::Transition> {
        self.0.pop_front()
    }

    fn iter<'a>(&'a self) -> Self::Iterator<'a> {
        self.0.iter()
    }
}

/// A transaction pool that assigns a priority to each transaction and then provides
/// them (to the authoring process, presumably) highest priority first.
///
/// It also refuses to queue transactions whose priority is below a certain threshold.
///
/// This is where the blockspace market takes place. A lot of interesting game theory
/// happens here.
pub struct PriorityPool<SM: StateMachine, P: Fn(SM::Transition) -> u64> {
    /// A means of determining a transaction's priority
    prioritizer: P,
    /// The minimum priority that will be accepted. Any transaction with a
    /// priority below this value will be rejected.
    minimum_priority: u64,
    queue: VecDeque<SM::Transition>,
}

impl<SM, P> TransactionPool<SM> for PriorityPool<SM, P>
where
    SM: StateMachine,
    P: Fn(SM::Transition) -> u64,
    SM::Transition: Clone + PartialEq,
{
    type Iterator<'a> = std::collections::vec_deque::Iter<'a, SM::Transition>
    where
    SM: 'a,
    P: 'a;

    fn try_insert(&mut self, t: <SM as StateMachine>::Transition) -> bool {
        let prio = (self.prioritizer)(t.clone());
        if prio < self.minimum_priority {
            return false;
        }

        // find first elem with lower prio
        // insert it just after that elem
        if let Some(pos) = self
            .queue
            .iter()
            .position(|e| (self.prioritizer)(e.clone()) < prio)
        {
            self.queue.insert(pos, t); // Insert at the correct position
        } else {
            self.queue.push_back(t); // Append if no lower-priority element is found
        }

        return true;
    }

    fn remove(&mut self, t: <SM as StateMachine>::Transition) {
        self.queue.retain(|e| *e != t);
    }

    fn size(&self) -> usize {
        self.queue.len()
    }

    fn contains(&self, t: <SM as StateMachine>::Transition) -> bool {
        self.queue.contains(&t)
    }

    fn next_from_pool(&mut self) -> Option<<SM as StateMachine>::Transition> {
        self.queue.pop_front()
    }

    fn iter<'a>(&'a self) -> Self::Iterator<'a> {
        self.queue.iter()
    }
}

impl<SM, P> PriorityPool<SM, P>
where
    SM: StateMachine,
    P: Fn(SM::Transition) -> u64,
    SM::Transition: Clone + PartialEq,
{
    pub fn new(prioritizer: P, minimum_priority: u64) -> Self {
        Self {
            prioritizer,
            minimum_priority,
            queue: VecDeque::new(),
        }
    }
}

impl<SM> Default for PriorityPool<SM, fn(SM::Transition) -> u64>
where
    SM: StateMachine,
    SM::Transition: Clone + PartialEq,
{
    fn default() -> Self {
        fn default_prioritizer<T>(_t: T) -> u64 {
            0
        }

        PriorityPool {
            prioritizer: default_prioritizer,
            minimum_priority: 0,
            queue: VecDeque::new(),
        }
    }
}

/// A transaction pool that censors some transactions.
///
/// It refuses to queue any transactions that are might be associated with terrorists.
pub struct CensoringPool<SM, P: Fn(SM::Transition) -> bool>
where
    SM: StateMachine,
{
    /// A means of determining whether a transaction may be from a terrorist
    might_be_terrorist: P,
    queue: VecDeque<SM::Transition>,
}

impl<SM, P> TransactionPool<SM> for CensoringPool<SM, P>
where
    SM: StateMachine,
    P: Fn(SM::Transition) -> bool,
    SM::Transition: Clone + PartialEq,
{
    type Iterator<'a> = std::collections::vec_deque::Iter<'a, SM::Transition>
    where
    SM: 'a,
    P: 'a;

    fn try_insert(&mut self, t: <SM as StateMachine>::Transition) -> bool {
        if (self.might_be_terrorist)(t.clone()) {
            return false;
        }
        self.queue.push_back(t.clone());
        return true;
    }

    fn remove(&mut self, t: <SM as StateMachine>::Transition) {
        self.queue.retain(|e| *e != t);
    }

    fn size(&self) -> usize {
        self.queue.len()
    }

    fn contains(&self, t: <SM as StateMachine>::Transition) -> bool {
        self.queue.contains(&t)
    }

    fn next_from_pool(&mut self) -> Option<<SM as StateMachine>::Transition> {
        self.queue.pop_front()
    }

    fn iter<'a>(&'a self) -> Self::Iterator<'a> {
        self.queue.iter()
    }
}

// --- TESTS ---

mod cl4_prio_pool {
    use crate::c1_state_machine::User;

    use super::*;

    #[test]
    fn empty_pool() {
        let mut pool: PriorityPool<AccountedCurrency, fn(_) -> u64> = PriorityPool::default();

        let elem = AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 12,
        };

        // removal of an element from an empty pool, shouldn't panic
        PriorityPool::remove(&mut pool, elem.clone());

        assert_eq!(PriorityPool::contains(&pool, elem.clone()), false);
        assert_eq!(PriorityPool::size(&pool), 0);
    }

    #[test]
    fn one_element() {
        let mut pool: PriorityPool<AccountedCurrency, fn(_) -> u64> = PriorityPool::default();

        let elem = AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 12,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem.clone()));
        assert_eq!(PriorityPool::size(&pool), 1);
        assert_eq!(PriorityPool::contains(&pool, elem.clone()), true);

        // removal of an element from an empty pool, shouldn't panic
        PriorityPool::remove(&mut pool, elem.clone());
        assert_eq!(PriorityPool::size(&pool), 0);
        assert_eq!(PriorityPool::contains(&pool, elem.clone()), false);
    }

    #[test]
    fn multiple_elements() {
        let mut pool: PriorityPool<AccountedCurrency, fn(_) -> u64> = PriorityPool::default();

        let elem = AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 12,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem.clone()));
        assert_eq!(PriorityPool::size(&pool), 1);
        assert_eq!(PriorityPool::contains(&pool, elem.clone()), true);

        let elem2 = AccountingTransaction::Mint {
            minter: User::Bob,
            amount: 11,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem2.clone()));
        assert_eq!(PriorityPool::size(&pool), 2);
        assert_eq!(PriorityPool::contains(&pool, elem2.clone()), true);
    }

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

    #[test]
    fn multiple_elements_prio_on_tx_type_and_min_prio() {
        let mut pool: PriorityPool<AccountedCurrency, fn(AccountingTransaction) -> u64> =
            PriorityPool::new(prioritizer, 5 as u64);

        let elem = AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 12,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem.clone()));
        assert_eq!(PriorityPool::size(&pool), 1);
        assert_eq!(PriorityPool::contains(&pool, elem.clone()), true);

        let elem2 = AccountingTransaction::Mint {
            minter: User::Bob,
            amount: 11,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem2.clone()));
        assert_eq!(PriorityPool::size(&pool), 2);
        assert_eq!(PriorityPool::contains(&pool, elem2.clone()), true);
        // still 1st element is at the top
        assert_eq!(PriorityPool::next_from_pool(&mut pool), Some(elem.clone()));

        let elem_transfer = AccountingTransaction::Transfer {
            sender: User::Alice,
            receiver: User::Bob,
            amount: 12,
        };
        assert!(PriorityPool::try_insert(&mut pool, elem_transfer.clone()));
        assert_eq!(PriorityPool::size(&pool), 2);
        assert_eq!(PriorityPool::contains(&pool, elem_transfer.clone()), true);
        // now transfer is at the top
        assert_eq!(
            PriorityPool::next_from_pool(&mut pool),
            Some(elem_transfer.clone())
        );

        let elem_burn = AccountingTransaction::Burn {
            burner: User::Alice,
            amount: 12,
        };
        // does not insert - prio too low
        assert_eq!(
            PriorityPool::try_insert(&mut pool, elem_burn.clone()),
            false
        );
        assert_eq!(PriorityPool::size(&pool), 1);
        // not in
        assert_eq!(PriorityPool::contains(&pool, elem_burn.clone()), false);
    }
}
