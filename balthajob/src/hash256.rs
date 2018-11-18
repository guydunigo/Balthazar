use sha3::{Digest, Sha3_256};

use std::fmt;
use std::ops::Deref;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash256 {
    inner: [u8; 32],
}

impl Deref for Hash256 {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl fmt::Display for Hash256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x")?;
        for b in self.inner.iter() {
            write!(f, "{:x}", b)?;
        }

        Ok(())
    }
}

impl From<&[u8]> for Hash256 {
    fn from(src: &[u8]) -> Self {
        if src.len() != 32 {
            panic!("Bad parameter to create Hash256 : source is not 32 bytes long.");
        } else {
            let mut inner = [0; 32];
            for i in 0..32 {
                inner[i] = src[i];
            }
            Hash256 { inner }
        }
    }
}

pub fn hash(input: &[u8]) -> Hash256 {
    let mut hasher = Sha3_256::default();
    hasher.input(input);

    let hash = hasher.result();

    Hash256::from(&hash[..])
}
