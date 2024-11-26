//! The state machines we have written so far model individual devices that are typically used
//! by a single user at a time. State machines can also model multi user systems. Blockchains
//! strive to provide reliable public infrastructure. And the public is very much multiple users.
//!
//! In this module and the next we explore two common techniques at modeling multi-user state
//! machines. In this module we explore accounts, and in the next we explore UTXOs.
//!
//! In this module we design a state machine that tracks the currency balances of several users.
//! Each user is associated with an account balance and users are able to send money to other users.

use super::{StateMachine, User};
use std::{collections::HashMap, hash::Hash};

/// This state machine models a multi-user currency system. It tracks the balance of each
/// user and allows users to send funds to one another.
#[derive(Clone, Hash)]
pub struct AccountedCurrency;

/// The main balances mapping.
///
/// Each entry maps a user id to their corresponding balance.
/// There exists an existential deposit of at least 1. That is
/// to say that an account gets removed from the map entirely
/// when its balance falls back to 0.
pub type Balances = HashMap<User, u64>;

#[derive(Clone, Default)]
pub struct BalancesB {
    pub balances: HashMap<User, u64>,
}

impl Hash for BalancesB {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (user, balance) in &self.balances {
            user.hash(state);
            balance.hash(state);
        }
    }
}

/// The state transitions that users can make in an accounted currency system
#[derive(Clone, Hash, PartialEq, Debug)]
pub enum AccountingTransaction {
    /// Create some new money for the given minter in the given amount
    Mint { minter: User, amount: u64 },
    /// Destroy some money from the given account in the given amount
    /// If the burn amount exceeds the account balance, burn the entire
    /// amount and remove the account from storage
    Burn { burner: User, amount: u64 },
    /// Send some tokens from one account to another
    Transfer {
        sender: User,
        receiver: User,
        amount: u64,
    },
}

/// We model this system as a state machine with three possible transitions
impl StateMachine for AccountedCurrency {
    type State = BalancesB;
    type Transition = AccountingTransaction;

    fn next_state(starting_state: &BalancesB, t: &AccountingTransaction) -> BalancesB {
        match t {
            AccountingTransaction::Mint { minter, amount } => {
                let mut new_state = starting_state.clone();
                if *amount <= 0 {
                    return new_state;
                }

                new_state
                    .balances
                    .entry(minter.clone())
                    .and_modify(|e| *e += amount)
                    .or_insert(*amount);
                new_state
            }
            AccountingTransaction::Burn { burner, amount } => {
                let mut new_state = starting_state.clone();
                let mut new_balances: HashMap<User, u64> = new_state.balances.clone();

                if let Some(&value) = new_balances.get(&burner) {
                    if value <= *amount {
                        new_balances.remove(&burner);
                        return new_state;
                    }
                }

                new_balances
                    .entry(burner.clone())
                    .and_modify(|e| *e -= amount);
                new_state.balances = new_balances;
                new_state
            }
            AccountingTransaction::Transfer {
                sender,
                receiver,
                amount,
            } => {
                let mut new_state = starting_state.clone();
                let mut new_balances: HashMap<User, u64> = new_state.balances.clone();

                if !new_balances.contains_key(sender) {
                    return new_state;
                }

                if let Some(&value) = new_balances.get(&sender) {
                    if value < *amount {
                        return new_state;
                    } else if value == *amount {
                        new_balances.remove(&sender);
                    } else {
                        new_balances
                            .entry(sender.clone())
                            .and_modify(|e| *e -= amount);
                    }
                }

                new_balances
                    .entry(receiver.clone())
                    .and_modify(|e| *e += amount)
                    .or_insert(*amount);

                new_state.balances = new_balances;
                new_state
            }
        }
    }
}

#[test]
fn sm_4_mint_creates_account() {
    let start: BalancesB = BalancesB::default();
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 100,
        },
    );
    let expected = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_mint_creates_second_account() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Mint {
            minter: User::Bob,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100), (User::Bob, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_mint_increases_balance() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 150)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_empty_mint() {
    let start: BalancesB = BalancesB::default();

    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Mint {
            minter: User::Alice,
            amount: 0,
        },
    );
    let expected = HashMap::new();

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_simple_burn() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Alice,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_burn_no_existential_deposit_left() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Bob,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_non_registered_burner() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Bob,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_burn_more_than_balance() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end2 = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Bob,
            amount: 100,
        },
    );
    let expected2 = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end2.balances, expected2);
}

#[test]
fn sm_4_empty_burn() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Alice,
            amount: 0,
        },
    );
    let expected = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_burner_does_not_exist() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Burn {
            burner: User::Bob,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_simple_transfer() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Alice,
            receiver: User::Bob,
            amount: 10,
        },
    );
    let expected = HashMap::from([(User::Alice, 90), (User::Bob, 60)]);

    assert_eq!(end.balances, expected);

    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 90), (User::Bob, 60)]),
    };

    let end1 = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Bob,
            receiver: User::Alice,
            amount: 50,
        },
    );
    let expected1 = HashMap::from([(User::Alice, 140), (User::Bob, 10)]);

    assert_eq!(end1.balances, expected1);
}

#[test]
fn sm_4_send_to_same_user() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Bob,
            receiver: User::Bob,
            amount: 10,
        },
    );
    let expected = HashMap::from([(User::Alice, 100), (User::Bob, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_insufficient_balance_transfer() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Bob,
            receiver: User::Alice,
            amount: 60,
        },
    );
    let expected = HashMap::from([(User::Alice, 100), (User::Bob, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_sender_not_registered() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Charlie,
            receiver: User::Alice,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100), (User::Bob, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_receiver_not_registered() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Alice,
            receiver: User::Charlie,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 50), (User::Bob, 50), (User::Charlie, 50)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_sender_to_empty_balance() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Bob,
            receiver: User::Alice,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 150)]);

    assert_eq!(end.balances, expected);
}

#[test]
fn sm_4_transfer() {
    let start = BalancesB {
        balances: HashMap::from([(User::Alice, 100), (User::Bob, 50)]),
    };
    let end = AccountedCurrency::next_state(
        &start,
        &AccountingTransaction::Transfer {
            sender: User::Bob,
            receiver: User::Charlie,
            amount: 50,
        },
    );
    let expected = HashMap::from([(User::Alice, 100), (User::Charlie, 50)]);

    assert_eq!(end.balances, expected);
}
