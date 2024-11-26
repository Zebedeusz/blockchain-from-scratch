//! In the previous chapter, we considered a hypothetical scenario where blocks must contain an even state root
//! in order to be valid. Now we will express that logic here as a higher-order consensus engine. It is higher-
//! order because it will wrap an inner consensus engine, such as PoW or PoA and work in either case.

use std::marker::PhantomData;

use crate::hash;

use super::{p1_pow::moderate_difficulty_pow, Consensus, Header, Pow};

use super::super::c2_blockchain::Header as HeaderPow;

/// A Consensus engine that requires the state root to be even for the header to be valid.
/// Wraps an inner consensus engine whose rules will also be enforced.
pub struct EvenOnly<Inner: Consensus> {
    /// The inner consensus engine that will be used in addition to the even-only requirement.
    pub inner: Inner,
}

impl<Inner: Consensus> Consensus for EvenOnly<Inner> {
    type Digest = Inner::Digest;

    fn validate(&self, parent_digest: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        return header.state_root % 2 == 0 && self.inner.validate(parent_digest, header);
    }

    fn seal(
        &self,
        parent_digest: &Self::Digest,
        partial_header: Header<Self::Digest>,
    ) -> Option<Header<Self::Digest>> {
        if partial_header.state_root % 2 == 0 {
            return self.inner.seal(parent_digest, partial_header);
        } else {
            return None;
        }
    }
}

/// Using the moderate difficulty PoW algorithm you created in section 1 of this chapter as the inner engine,
/// create a PoW chain that is valid according to the inner consensus engine, but is not valid according to
/// this engine because the state roots are not all even.
fn almost_valid_but_not_all_even() -> Vec<Header<u64>> {
    let mut chain = Vec::<HeaderPow>::new();
    let g: HeaderPow = HeaderPow::genesis(2);
    chain.push(g.clone());
    for i in 0..10 {
        chain.push(g.child(hash(&vec![i]), hash(&vec![i])));
    }

    let mut result_chain = Vec::<Header<u64>>::new();
    for e in chain {
        result_chain.push(Header {
            parent: e.parent,
            height: e.height,
            state_root: e.state_root,
            extrinsics_root: e.extrinsics_root,
            consensus_digest: e.consensus_digest,
        });
    }
    return result_chain;
}

// --- TESTS ---

#[test]
fn cs4_even_only_valid() {
    let pow = moderate_difficulty_pow();
    let even_only = EvenOnly { inner: pow };

    let parent_digest = 0;
    let header = Header {
        parent: 0,
        height: 1,
        state_root: 2,
        extrinsics_root: 0,
        consensus_digest: 0,
    };

    assert!(even_only.validate(&parent_digest, &header));
}

#[test]
fn cs4_even_only_invalid() {
    let pow = moderate_difficulty_pow();
    let even_only = EvenOnly { inner: pow };

    let parent_digest = 0;
    let header = Header {
        parent: 0,
        height: 1,
        state_root: 3,
        extrinsics_root: 0,
        consensus_digest: 0,
    };

    assert!(!even_only.validate(&parent_digest, &header));
}

#[test]
fn cs4_even_only_seal_valid() {
    let pow = moderate_difficulty_pow();
    let even_only = EvenOnly { inner: pow };

    let parent_digest = 0;
    let partial_header = Header::<u64> {
        parent: 0,
        height: 1,
        state_root: 2,
        extrinsics_root: 0,
        consensus_digest: 0,
    };

    assert!(even_only.seal(&parent_digest, partial_header).is_some());
}

#[test]
fn cs4_even_only_seal_invalid() {
    let pow = moderate_difficulty_pow();
    let even_only = EvenOnly { inner: pow };

    let parent_digest = 0;
    let partial_header = Header::<u64> {
        parent: 0,
        height: 1,
        state_root: 3,
        extrinsics_root: 0,
        consensus_digest: 0,
    };

    assert!(even_only.seal(&parent_digest, partial_header).is_none());
}

#[test]
fn cs4_almost_valid_but_not_all_even() {
    let chain = almost_valid_but_not_all_even();
    let pow = moderate_difficulty_pow();
    let even_only = EvenOnly { inner: pow };

    for i in 0..chain.len() {
        let parent_digest = if i == 0 {
            0
        } else {
            chain[i - 1].consensus_digest
        };
        let header = &chain[i];
        if header.state_root % 2 == 0 {
            assert!(even_only.validate(&parent_digest, header));
        } else {
            assert!(!even_only.validate(&parent_digest, header));
        }
    }
}
