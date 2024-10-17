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
pub struct SimplePoa {
    pub authorities: Vec<ConsensusAuthority>,
}

impl Consensus for SimplePoa {
    type Digest = ConsensusAuthority;

    fn validate(&self, _: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        return self.authorities.contains(&header.consensus_digest);
    }

    fn seal(&self, _: &Self::Digest, partial_header: Header<()>) -> Option<Header<Self::Digest>> {
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
struct PoaRoundRobinByHeight {
    authorities: Vec<ConsensusAuthority>,
}

impl Consensus for PoaRoundRobinByHeight {
    type Digest = ConsensusAuthority;

    // TODO TESTS
    fn validate(&self, _: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        let auth_that_was_supposed_to_sign = header.height % (self.authorities.len() as u64) - 1;
        return header.consensus_digest
            == self
                .authorities
                .get(auth_that_was_supposed_to_sign as usize)
                .unwrap()
                .clone();
    }

    // TODO TESTS
    fn seal(&self, _: &Self::Digest, partial_header: Header<()>) -> Option<Header<Self::Digest>> {
        let auth_that_is_supposed_to_sign =
            partial_header.height % (self.authorities.len() as u64) - 1;

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
#[derive(Hash, Debug, PartialEq, Eq, Clone, Copy)]
struct SlotDigest {
    slot: u64,
    signature: ConsensusAuthority,
}

impl Consensus for PoaRoundRobinBySlot {
    type Digest = SlotDigest;

    // TODO TESTS
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
        partial_header: Header<()>,
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
