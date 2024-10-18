//! We saw in the previous chapter that blockchain communities sometimes opt to modify the
//! consensus rules from time to time. This process is knows as a fork. Here we implement
//! a higher-order consensus engine that allows such forks to be made.
//!
//! The consensus engine we implement here does not contain the specific consensus rules to
//! be enforced before or after the fork, but rather delegates to existing consensus engines
//! for that. Here we simply write the logic for detecting whether we are before or after the fork.

use std::marker::PhantomData;

use super::{
    Consensus, ConsensusAuthority, EvenOnly, Header, PoaRoundRobinByHeight, Pow, PowOrPoaDigest,
};

/// A Higher-order consensus engine that represents a change from one set of consensus rules (Before) to
/// another set (After) at a specific block height
struct Forked<D, Before, After> {
    /// The first block height at which the new consensus rules apply
    fork_height: u64,
    before: Before,
    after: After,
    phdata: PhantomData<D>,
}

impl<D, B, A> Consensus for Forked<D, B, A>
where
    D: Clone + core::fmt::Debug + Eq + PartialEq + std::hash::Hash,
    // + Into<B::Digest>
    // + Into<A::Digest>,
    B::Digest: TryFrom<D>, // Use TryFrom here for PoW digest (u64)
    A::Digest: TryFrom<D>, // Use TryFrom here for PoA digest (ConsensusAuthority)
    D: From<B::Digest> + From<A::Digest>, // Handle From in the other direction
    // + TryFrom<A::Digest>
    // + TryFrom<B::Digest>,
    B: Consensus,
    A: Consensus,
    // B::Digest: Into<D>,
    // A::Digest: Into<D>,
{
    type Digest = D;

    // if header.height < self.fork_height -> validate with Consensus B validate function
    // otherwise validate with consensus A validate function
    fn validate(&self, parent_digest: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        return if header.height < self.fork_height {
            if let Ok(parent_pow_digest) = B::Digest::try_from(parent_digest.clone()) {
                if let Ok(header_pow_digest) =
                    B::Digest::try_from((header.consensus_digest).clone())
                {
                    return B::validate(
                        &self.before,
                        &parent_pow_digest,
                        &Header {
                            height: header.height,
                            state_root: header.state_root.clone(),
                            extrinsics_root: header.extrinsics_root.clone(),
                            parent: header.parent,
                            consensus_digest: header_pow_digest,
                        },
                    );
                }
            }
            false
        } else {
            if let Ok(parent_poa_digest) = A::Digest::try_from(parent_digest.clone()) {
                if let Ok(header_pow_digest) =
                    A::Digest::try_from((header.consensus_digest).clone())
                {
                    A::validate(
                        &self.after,
                        &parent_poa_digest,
                        &Header {
                            height: header.height,
                            state_root: header.state_root.clone(),
                            extrinsics_root: header.extrinsics_root.clone(),
                            parent: header.parent,
                            consensus_digest: header_pow_digest,
                        },
                    );
                }
            }
            false
        };
    }

    fn seal(
        &self,
        parent_digest: &Self::Digest,
        partial_header: Header<()>,
    ) -> Option<Header<Self::Digest>> {
        if partial_header.height < self.fork_height {
            // Convert parent digest to PoW digest
            if let Ok(pow_digest) = B::Digest::try_from((*parent_digest).clone()) {
                return self
                    .before
                    .seal(&pow_digest, partial_header)
                    .map(|header| Header {
                        height: header.height,
                        state_root: header.state_root,
                        extrinsics_root: header.extrinsics_root,
                        parent: header.parent,
                        consensus_digest: header.consensus_digest.into(),
                    });
            }
            None
        } else {
            // Convert parent digest to PoA digest
            if let Ok(poa_digest) = A::Digest::try_from((*parent_digest).clone()) {
                return self
                    .after
                    .seal(&poa_digest, partial_header)
                    .map(|header| Header {
                        height: header.height,
                        state_root: header.state_root,
                        extrinsics_root: header.extrinsics_root,
                        parent: header.parent,
                        consensus_digest: header.consensus_digest.into(),
                    });
            }
            None
        }
    }
}

/// Create a PoA consensus engine that changes authorities part way through the chain's history.
/// Given the initial authorities, the authorities after the fork, and the height at which the fork occurs.
fn change_authorities(
    fork_height: u64,
    initial_authorities: Vec<ConsensusAuthority>,
    final_authorities: Vec<ConsensusAuthority>,
) -> impl Consensus {
    return Forked::<ConsensusAuthority, PoaRoundRobinByHeight, PoaRoundRobinByHeight> {
        fork_height,
        before: PoaRoundRobinByHeight {
            authorities: initial_authorities,
        },
        after: PoaRoundRobinByHeight {
            authorities: final_authorities,
        },
        phdata: PhantomData,
    };
}

/// Create a PoW consensus engine that changes the difficulty part way through the chain's history.
fn change_difficulty(
    fork_height: u64,
    initial_difficulty: u64,
    final_difficulty: u64,
) -> impl Consensus {
    return Forked::<u64, Pow, Pow> {
        fork_height,
        before: Pow {
            threshold: initial_difficulty,
        },
        after: Pow {
            threshold: final_difficulty,
        },
        phdata: PhantomData,
    };
}

/// Earlier in this chapter we implemented a consensus rule in which blocks are only considered valid if
/// they contain an even state root. Sometimes a chain will be launched with a more traditional consensus like
/// PoW or PoA and only introduce an additional requirement like even state root after a particular height.
///
/// Create a consensus engine that introduces the even-only logic only after the given fork height.
/// Other than the evenness requirement, the consensus rules should not change at the fork. This function
/// should work with either PoW, PoA, or anything else as the underlying consensus engine.
fn even_after_given_height<Original: Consensus + Clone>(
    fork_height: u64,
    consensus: Original,
) -> impl Consensus {
    return Forked::<Original::Digest, Original, EvenOnly<Original>> {
        fork_height,
        before: consensus.clone(),
        after: EvenOnly { inner: consensus },
        phdata: PhantomData,
    };
}

/// In the spirit of Ethereum's recent switch from PoW to PoA, let us model a similar
/// switch in our consensus framework. It should go without saying that the real-world ethereum
/// handoff was considerably more complex than it may appear in our simplified example, although
/// the fundamentals are the same.
///
/// For this task, you may use the PowOrPoaDigest type from the previous module if you like.
fn pow_to_poa(
    fork_height: u64,
    difficulty: u64,
    authorities: Vec<ConsensusAuthority>,
) -> impl Consensus<Digest = PowOrPoaDigest> {
    return Forked::<PowOrPoaDigest, Pow, PoaRoundRobinByHeight> {
        fork_height,
        before: Pow {
            threshold: difficulty,
        },
        after: PoaRoundRobinByHeight { authorities },
        phdata: PhantomData,
    };
}
