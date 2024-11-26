//! Proof of Work is very energy intensive but is decentralized. Dictator is energy cheap, but
//! is completely centralized. Let's achieve a middle ground by choosing a set of authorities
//! who can sign blocks as opposed to a single dictator. This arrangement is typically known as
//! Proof of Authority.
//!
//! In public blockchains, Proof of Authority is often moved even further toward the decentralized
//! and permissionless end of the spectrum by electing the authorities on-chain through an economic
//! game in which users stake tokens. In such a configuration it is often known as "Proof of Stake".
//! Even when using the Proof of Stake configuration, the underlying consensus logic is identical to
//! the proof of authority we are writing here.

use super::{Consensus, ConsensusAuthority, Header};

/// A Proof of Authority consensus engine. If any of the authorities have signed the block, it is valid.
#[derive(Hash, Clone)]
pub struct SimplePoa {
    pub authorities: Vec<ConsensusAuthority>,
}

impl Default for Header<ConsensusAuthority> {
    fn default() -> Self {
        Header {
            consensus_digest: ConsensusAuthority::Alice,
            state_root: Default::default(),
            extrinsics_root: Default::default(),
            parent: Default::default(),
            height: Default::default(),
        }
    }
}

impl Default for Header<SlotDigest> {
    fn default() -> Self {
        Header {
            consensus_digest: SlotDigest {
                slot: 1,
                signature: ConsensusAuthority::Alice,
            },
            state_root: Default::default(),
            extrinsics_root: Default::default(),
            parent: Default::default(),
            height: Default::default(),
        }
    }
}

impl Default for Header<()> {
    fn default() -> Self {
        Header {
            state_root: Default::default(),
            extrinsics_root: Default::default(),
            parent: Default::default(),
            height: Default::default(),
            consensus_digest: (),
        }
    }
}

impl Consensus for SimplePoa {
    type Digest = ConsensusAuthority;

    fn validate(&self, _: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        return self.authorities.contains(&header.consensus_digest);
    }

    fn seal(
        &self,
        _: &Self::Digest,
        partial_header: Header<Self::Digest>,
    ) -> Option<Header<Self::Digest>> {
        let header: Header<Self::Digest> = Header {
            consensus_digest: self.authorities.get(0).unwrap().clone(),
            state_root: partial_header.state_root,
            extrinsics_root: partial_header.extrinsics_root,
            parent: partial_header.parent,
            height: partial_header.height,
        };
        return Some(header);
    }
}

/// A Proof of Authority consensus engine. Only one authority is valid at each block height.
/// As ever, the genesis block does not require a seal. After that the authorities take turns
/// in order.
pub struct PoaRoundRobinByHeight {
    pub authorities: Vec<ConsensusAuthority>,
}

impl Consensus for PoaRoundRobinByHeight {
    type Digest = ConsensusAuthority;

    fn validate(&self, _: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        // f(1,4) -> 0 // 1 % 4 = 1
        // f(2,4) -> 1 // 2 % 4 = 2
        // f(3,4) -> 2 // 3 % 4 = 3
        // f(4,4) -> 3 // 4 % 4 = 0
        // f(5,4) -> 0 // 5 % 4 = 1
        // f(6,4) -> 1 // 6 % 4 = 2

        let auth_that_was_supposed_to_sign = (header.height - 1) % (self.authorities.len() as u64);
        return header.consensus_digest
            == self
                .authorities
                .get(auth_that_was_supposed_to_sign as usize)
                .unwrap()
                .clone();
    }

    fn seal(
        &self,
        _: &Self::Digest,
        partial_header: Header<Self::Digest>,
    ) -> Option<Header<Self::Digest>> {
        let auth_that_is_supposed_to_sign =
            (partial_header.height - 1) % (self.authorities.len() as u64);

        let header: Header<Self::Digest> = Header {
            consensus_digest: self
                .authorities
                .get(auth_that_is_supposed_to_sign as usize)
                .unwrap()
                .clone(),
            state_root: partial_header.state_root,
            extrinsics_root: partial_header.extrinsics_root,
            parent: partial_header.parent,
            height: partial_header.height,
        };
        return Some(header);
    }
}

/// Both of the previous PoA schemes have the weakness that a single dishonest authority can corrupt the chain.
/// * When allowing any authority to sign, the single corrupt authority can sign blocks with invalid transitions
///   with no way to throttle them.
/// * When using the round robin by height, their is throttling, but the dishonest authority can stop block production
///   entirely by refusing to ever sign a block at their height.
///
/// A common PoA scheme that works around these weaknesses is to divide time into slots, and then do a round robin
/// by slot instead of by height
struct PoaRoundRobinBySlot {
    authorities: Vec<ConsensusAuthority>,
}

/// A digest used for PoaRoundRobinBySlot. The digest contains the slot number as well as the signature.
/// In addition to checking that the right signer has signed for the slot, you must check that the slot is
/// always strictly increasing. But remember that slots may be skipped.
#[derive(Hash, Debug, PartialEq, Eq, Clone, Copy, Default)]
struct SlotDigest {
    slot: u64,
    signature: ConsensusAuthority,
}

impl Consensus for PoaRoundRobinBySlot {
    type Digest = SlotDigest;

    fn validate(&self, parent_digest: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        if parent_digest.slot >= header.consensus_digest.slot {
            return false;
        }

        let parent_pos = self
            .authorities
            .iter()
            .position(|&a| a == parent_digest.signature)
            .unwrap();

        let next_auth = if parent_pos == self.authorities.len() - 1 {
            0
        } else {
            parent_pos + 1
        };

        return header.consensus_digest.signature
            == self.authorities.get(next_auth).unwrap().clone();
    }

    fn seal(
        &self,
        parent_digest: &Self::Digest,
        partial_header: Header<Self::Digest>,
    ) -> Option<Header<Self::Digest>> {
        let parent_pos = self
            .authorities
            .iter()
            .position(|&a| a == parent_digest.signature)
            .unwrap();
        let next_auth = if parent_pos == self.authorities.len() - 1 {
            0
        } else {
            parent_pos + 1
        };

        let header: Header<Self::Digest> = Header {
            consensus_digest: SlotDigest {
                slot: parent_digest.slot + 1,
                signature: self.authorities.get(next_auth).unwrap().clone(),
            },
            state_root: partial_header.state_root,
            extrinsics_root: partial_header.extrinsics_root,
            parent: partial_header.parent,
            height: partial_header.height,
        };
        return Some(header);
    }
}

// --- TESTS ---
#[test]
fn cs3_simple_poa_validate() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = SimplePoa { authorities };

    let mut header = Header::<<SimplePoa as Consensus>::Digest>::default();
    header.consensus_digest = ConsensusAuthority::Alice;

    assert!(poa.validate(&ConsensusAuthority::Alice, &header));

    header.consensus_digest = ConsensusAuthority::Charlie;
    assert!(!poa.validate(&ConsensusAuthority::Alice, &header));
}

#[test]
fn cs3_simple_poa_seal() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = SimplePoa { authorities };

    let partial_header = Header::<ConsensusAuthority>::default();
    let sealed_header = poa
        .seal(&ConsensusAuthority::Alice, partial_header)
        .unwrap();

    assert_eq!(sealed_header.consensus_digest, ConsensusAuthority::Alice);
}

#[test]
fn cs3_poa_round_robin_by_height_validate() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = PoaRoundRobinByHeight { authorities };

    let mut header = Header {
        height: 1,
        consensus_digest: ConsensusAuthority::Alice,
        ..Default::default()
    };

    assert!(poa.validate(&ConsensusAuthority::Alice, &header));

    header.consensus_digest = ConsensusAuthority::Bob;
    assert!(!poa.validate(&ConsensusAuthority::Alice, &header));

    header.height = 2;
    assert!(poa.validate(&ConsensusAuthority::Alice, &header));
}

#[test]
fn cs3_poa_round_robin_by_height_seal() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = PoaRoundRobinByHeight { authorities };

    let mut header = Header {
        height: 1,
        ..Default::default()
    };
    let mut sealed_header = poa
        .seal(&ConsensusAuthority::Alice, header.clone())
        .unwrap();

    assert_eq!(sealed_header.consensus_digest, ConsensusAuthority::Alice);

    header.height = 2;
    sealed_header = poa
        .seal(&ConsensusAuthority::Alice, header.clone())
        .unwrap();
    assert_eq!(sealed_header.consensus_digest, ConsensusAuthority::Bob);

    header.height = 3;
    sealed_header = poa.seal(&ConsensusAuthority::Alice, header).unwrap();
    assert_eq!(sealed_header.consensus_digest, ConsensusAuthority::Alice);
}

#[test]
fn cs3_poa_round_robin_by_slot_validate() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = PoaRoundRobinBySlot { authorities };

    let parent_digest = SlotDigest {
        slot: 1,
        signature: ConsensusAuthority::Alice,
    };

    let header = Header {
        consensus_digest: SlotDigest {
            slot: 2,
            signature: ConsensusAuthority::Bob,
        },
        ..Default::default()
    };

    assert!(poa.validate(&parent_digest, &header));
    assert!(!poa.validate(
        &parent_digest,
        &Header {
            consensus_digest: SlotDigest {
                slot: 1,
                signature: ConsensusAuthority::Bob,
            },
            ..Default::default()
        }
    ));
}

#[test]
fn cs3_poa_round_robin_by_slot_seal() {
    let authorities = vec![ConsensusAuthority::Alice, ConsensusAuthority::Bob];
    let poa = PoaRoundRobinBySlot { authorities };

    let parent_digest = SlotDigest {
        slot: 1,
        signature: ConsensusAuthority::Alice,
    };

    let mut partial_header = Header::default();
    let mut sealed_header = poa.seal(&parent_digest, partial_header).unwrap();

    assert_eq!(sealed_header.consensus_digest.slot, 2);
    assert_eq!(
        sealed_header.consensus_digest.signature,
        ConsensusAuthority::Bob
    );

    partial_header = Header::default();
    sealed_header = poa
        .seal(&sealed_header.consensus_digest, partial_header)
        .unwrap();

    assert_eq!(sealed_header.consensus_digest.slot, 3);
    assert_eq!(
        sealed_header.consensus_digest.signature,
        ConsensusAuthority::Alice
    );
}
