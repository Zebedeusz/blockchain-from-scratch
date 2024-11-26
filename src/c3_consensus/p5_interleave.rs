//! PoW and PoA each have their own set of strengths and weaknesses. Many chains are happy to choose
//! one of them. But other chains would like consensus properties that fall in between. To achieve this
//! we could consider interleaving PoW blocks with PoA blocks. Some very early designs of Ethereum considered
//! this approach as a way to transition away from PoW.

use std::u64;

use super::{Consensus, ConsensusAuthority, Header, Pow, SimplePoa};

/// A Consensus engine that alternates back and forth between PoW and PoA sealed blocks.
///
/// Odd blocks are PoW
/// Even blocks are PoA
struct AlternatingPowPoa {
    pub pow: Pow,
    pub poa: SimplePoa,
}

/// In order to implement a consensus that can be sealed with either work or a signature,
/// we will need an enum that wraps the two individual digest types.
#[derive(Hash, Debug, PartialEq, Eq, Clone, Copy)]
pub enum PowOrPoaDigest {
    Pow(u64),
    Poa(ConsensusAuthority),
}

impl Default for PowOrPoaDigest {
    fn default() -> Self {
        PowOrPoaDigest::Pow(0)
    }
}

// Provides an implementation of convertion from u64 to PowOrPoaDigest.
impl From<u64> for PowOrPoaDigest {
    fn from(v: u64) -> Self {
        PowOrPoaDigest::Pow(v)
    }
}

// Provides an implementation of convertion from PowOrPoaDigest to u64. May fail though.
impl TryFrom<PowOrPoaDigest> for u64 {
    type Error = ();

    fn try_from(d: PowOrPoaDigest) -> Result<Self, Self::Error> {
        match d {
            PowOrPoaDigest::Pow(v) => Ok(v),
            PowOrPoaDigest::Poa(_) => Err(()),
        }
    }
}

// Provides an implementation of convertion from ConsensusAuthority to PowOrPoaDigest.
impl From<ConsensusAuthority> for PowOrPoaDigest {
    fn from(c: ConsensusAuthority) -> Self {
        PowOrPoaDigest::Poa(c)
    }
}

// Provides an implementation of convertion from PowOrPoaDigest to ConsensusAuthority. May fail though.
impl TryFrom<PowOrPoaDigest> for ConsensusAuthority {
    type Error = ();

    fn try_from(d: PowOrPoaDigest) -> Result<Self, Self::Error> {
        match d {
            PowOrPoaDigest::Pow(_) => Err(()),
            PowOrPoaDigest::Poa(c) => Ok(c),
        }
    }
}

impl From<Header<PowOrPoaDigest>> for Header<ConsensusAuthority> {
    fn from(h: Header<PowOrPoaDigest>) -> Self {
        Header {
            parent: h.parent,
            height: h.height,
            state_root: h.state_root,
            extrinsics_root: h.extrinsics_root,
            consensus_digest: ConsensusAuthority::try_from(h.consensus_digest).unwrap(),
        }
    }
}

impl From<Header<PowOrPoaDigest>> for Header<u64> {
    fn from(h: Header<PowOrPoaDigest>) -> Self {
        Header {
            parent: h.parent,
            height: h.height,
            state_root: h.state_root,
            extrinsics_root: h.extrinsics_root,
            consensus_digest: u64::try_from(h.consensus_digest).unwrap(),
        }
    }
}

impl Consensus for AlternatingPowPoa {
    type Digest = PowOrPoaDigest;

    fn validate(&self, parent_digest: &Self::Digest, header: &Header<Self::Digest>) -> bool {
        return match parent_digest {
            PowOrPoaDigest::Pow(_) => {
                let res = ConsensusAuthority::try_from(header.consensus_digest);
                res.is_ok()
                    && self.poa.validate(
                        &ConsensusAuthority::Alice,
                        &Header::<ConsensusAuthority>::from(header.clone()),
                    )
            }
            PowOrPoaDigest::Poa(_) => {
                let res = u64::try_from(header.consensus_digest);
                res.is_ok()
                    && self
                        .pow
                        .validate(&u64::MIN, &Header::<u64>::from(header.clone()))
            }
        };
    }

    fn seal(
        &self,
        parent_digest: &Self::Digest,
        partial_header: Header<Self::Digest>,
    ) -> Option<Header<Self::Digest>> {
        let digest = match parent_digest {
            PowOrPoaDigest::Pow(_) => {
                PowOrPoaDigest::Poa(
                    self.poa
                        .seal(
                            &ConsensusAuthority::Alice,
                            Header {
                                parent: partial_header.parent,
                                height: partial_header.height,
                                state_root: partial_header.state_root,
                                extrinsics_root: partial_header.extrinsics_root,
                                consensus_digest: ConsensusAuthority::default(),
                            },
                        )
                        .unwrap()
                        .clone()
                        .consensus_digest,
                )
                // PowOrPoaDigest(self.poa.seal(&ConsensusAuthority::Alice, partial_header))
            }
            PowOrPoaDigest::Poa(_) => PowOrPoaDigest::Pow(
                self.pow
                    .seal(
                        &u64::MIN,
                        Header {
                            parent: partial_header.parent,
                            height: partial_header.height,
                            state_root: partial_header.state_root,
                            extrinsics_root: partial_header.extrinsics_root,
                            consensus_digest: 0,
                        },
                    )
                    .unwrap()
                    .clone()
                    .consensus_digest,
            ),
        };

        return Some(Header {
            consensus_digest: digest,
            parent: partial_header.parent,
            height: partial_header.height,
            state_root: partial_header.state_root,
            extrinsics_root: partial_header.extrinsics_root,
        });
    }
}

// --- TESTS ---

#[test]
fn cs5_pow_or_poa_digest_from_u64() {
    let value: u64 = 42;
    let digest: PowOrPoaDigest = value.into();
    assert_eq!(digest, PowOrPoaDigest::Pow(42));
}

#[test]
fn cs5_pow_or_poa_digest_try_from_u64() {
    let digest = PowOrPoaDigest::Pow(42);
    let value: Result<u64, ()> = u64::try_from(digest);
    assert_eq!(value, Ok(42));
}

#[test]
fn cs5_pow_or_poa_digest_try_from_consensus_authority() {
    let authority = ConsensusAuthority::Alice;
    let digest = PowOrPoaDigest::Poa(authority);
    let value: Result<ConsensusAuthority, ()> = ConsensusAuthority::try_from(digest);
    assert_eq!(value, Ok(ConsensusAuthority::Alice));
}

#[test]
fn cs5_alternating_pow_poa_validate_pow() {
    let pow = Pow { threshold: 12 };
    let poa = SimplePoa {
        authorities: vec![ConsensusAuthority::Alice],
    };
    let consensus = AlternatingPowPoa { pow, poa };

    let parent_digest = PowOrPoaDigest::Pow(42);
    let header = Header {
        parent: 0,
        height: 1,
        state_root: 1,
        extrinsics_root: 1,
        consensus_digest: PowOrPoaDigest::Poa(ConsensusAuthority::Alice),
    };

    assert!(consensus.validate(&parent_digest, &header));
}

#[test]
fn cs5_alternating_pow_poa_validate_poa() {
    let pow = Pow { threshold: 20 };
    let poa = SimplePoa {
        authorities: vec![ConsensusAuthority::Alice],
    };
    let consensus = AlternatingPowPoa { pow, poa };

    let parent_digest = PowOrPoaDigest::Poa(ConsensusAuthority::Alice);
    let header = Header {
        parent: 0,
        height: 1,
        state_root: 1,
        extrinsics_root: 1,
        consensus_digest: PowOrPoaDigest::Pow(12),
    };

    assert!(consensus.validate(&parent_digest, &header));
}

#[test]
fn cs5_alternating_pow_poa_seal_pow() {
    let pow = Pow {
        threshold: u64::MAX,
    };
    let poa = SimplePoa {
        authorities: vec![ConsensusAuthority::Alice],
    };
    let consensus = AlternatingPowPoa { pow, poa };

    let parent_digest = PowOrPoaDigest::Poa(ConsensusAuthority::Alice);
    let partial_header = Header::<PowOrPoaDigest> {
        parent: 0,
        height: 1,
        state_root: 1,
        extrinsics_root: 1,
        consensus_digest: PowOrPoaDigest::Pow(0),
    };

    let sealed_header = consensus.seal(&parent_digest, partial_header).unwrap();
    assert!(matches!(
        sealed_header.consensus_digest,
        PowOrPoaDigest::Pow(_)
    ));
}

#[test]
fn cs5_alternating_pow_poa_seal_poa() {
    let pow = Pow { threshold: 12 };
    let poa = SimplePoa {
        authorities: vec![ConsensusAuthority::Alice],
    };
    let consensus = AlternatingPowPoa { pow, poa };

    let parent_digest = PowOrPoaDigest::Pow(42);
    let partial_header = Header::<PowOrPoaDigest> {
        parent: 0,
        height: 1,
        state_root: 1,
        extrinsics_root: 1,
        consensus_digest: PowOrPoaDigest::Pow(0),
    };

    let sealed_header = consensus.seal(&parent_digest, partial_header).unwrap();
    assert!(matches!(
        sealed_header.consensus_digest,
        PowOrPoaDigest::Poa(_)
    ));
}
