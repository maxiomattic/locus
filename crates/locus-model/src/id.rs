//! Stable identifiers.
//!
//! An [`Id`] is a ULID: 48 bits of milliseconds and 80 random bits, written in
//! 26 characters of Crockford base32. Anything can mint one offline, which is
//! what lets a client create an object before its home server has heard of it.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id([u8; 16]);

/// The ID of an object. Never changes, whatever happens to its fields.
pub type ObjectId = Id;
/// The ID of an operation, unique across every machine.
pub type OpId = Id;

impl Id {
    /// The actor that wrote the genesis entries of a store.
    pub const SYSTEM: Id = Id([0; 16]);

    pub fn new() -> Self {
        Self::at(crate::now_ms())
    }

    pub fn at(ms: i64) -> Self {
        let mut b = [0u8; 16];
        b[..6].copy_from_slice(&(ms as u64).to_be_bytes()[2..]);
        getrandom::fill(&mut b[6..]).expect("the OS random source failed");
        Id(b)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn from_bytes(b: [u8; 16]) -> Self {
        Id(b)
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = u128::from_be_bytes(self.0);
        let mut out = [0u8; 26];
        for (i, c) in out.iter_mut().enumerate() {
            *c = ALPHABET[((n >> (5 * (25 - i))) & 31) as usize];
        }
        f.write_str(std::str::from_utf8(&out).expect("the alphabet is ASCII"))
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("not a Locus ID: {0}")]
pub struct BadId(pub String);

impl FromStr for Id {
    type Err = BadId;

    fn from_str(s: &str) -> Result<Self, BadId> {
        let bad = || BadId(s.to_string());
        if s.len() != 26 || s.as_bytes()[0] > b'7' {
            return Err(bad());
        }
        let mut n: u128 = 0;
        for c in s.bytes() {
            let c = c.to_ascii_uppercase();
            let v = ALPHABET.iter().position(|a| *a == c).ok_or_else(bad)?;
            n = (n << 5) | v as u128;
        }
        Ok(Id(n.to_be_bytes()))
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A blob's address: the BLAKE3 hash of its bytes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobHash(pub [u8; 32]);

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for BlobHash {
    type Err = BadId;

    fn from_str(s: &str) -> Result<Self, BadId> {
        let bad = || BadId(s.to_string());
        if s.len() != 64 || !s.is_ascii() {
            return Err(bad());
        }
        let mut out = [0u8; 32];
        for (i, o) in out.iter_mut().enumerate() {
            *o = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
        }
        Ok(BlobHash(out))
    }
}

impl Serialize for BlobHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BlobHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// The name of a home server. A plain string for now, so that certificates can
/// later bind it to a key without changing what objects store.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerId(pub String);

impl fmt::Display for ServerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ServerId {
    fn from(s: &str) -> Self {
        ServerId(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_sort_by_time() {
        let a = Id::at(1_000);
        let b = Id::at(2_000);
        assert!(a < b);
        assert_eq!(a.to_string().parse::<Id>().unwrap(), a);
        assert_eq!(a.to_string().len(), 26);
        assert!("not-an-id".parse::<Id>().is_err());
        assert!("80000000000000000000000000".parse::<Id>().is_err());
    }

    #[test]
    fn blob_hash_round_trips() {
        let h = BlobHash([7; 32]);
        assert_eq!(h.to_string().parse::<BlobHash>().unwrap(), h);
    }
}
