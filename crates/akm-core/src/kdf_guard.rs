//! A limit on the key derivation cost that a vault file asks for (FR-28).
//!
//! The outer header of a KDBX file says how expensive its key derivation is: Argon2 memory,
//! passes and lanes, or the AES-KDF round count. A crafted file can ask for terabytes or for
//! years of work, and the program that opens it pays before it knows whether the password is
//! right. This module reads only those numbers, before any key derivation, and refuses a file
//! above a fixed limit. It reads no more of the file than the header fields it needs.
//!
//! A file that is not a KDBX 3 or 4 file, or whose header is cut short or malformed, is not
//! refused here. It goes on to the library, which reports it with its own error.

use crate::error::{Error, Result};

const GIB: u64 = 1 << 30;

/// Argon2 memory, in bytes.
pub const MAX_ARGON2_MEMORY: u64 = 4 * GIB;
/// Argon2 passes.
pub const MAX_ARGON2_ITERATIONS: u64 = 100_000;
/// Argon2 lanes.
pub const MAX_ARGON2_PARALLELISM: u64 = 256;
/// AES-KDF rounds.
pub const MAX_AES_ROUNDS: u64 = 2_000_000_000;

const SIGNATURE: [u8; 8] = [0x03, 0xD9, 0xA2, 0x9A, 0x67, 0xFB, 0x4B, 0xB5];

const KDF_AES_KDBX3: [u8; 16] = [
    0xc9, 0xd9, 0xf3, 0x9a, 0x62, 0x8a, 0x44, 0x60, 0xbf, 0x74, 0x0d, 0x08, 0xc1, 0x8a, 0x4f, 0xea,
];
const KDF_AES_KDBX4: [u8; 16] = [
    0x7c, 0x02, 0xbb, 0x82, 0x79, 0xa7, 0x4a, 0xc0, 0x92, 0x7d, 0x11, 0x4a, 0x00, 0x64, 0x82, 0x38,
];
const KDF_ARGON2D: [u8; 16] = [
    0xef, 0x63, 0x6d, 0xdf, 0x8c, 0x29, 0x44, 0x4b, 0x91, 0xf7, 0xa9, 0xa4, 0x03, 0xe3, 0x0a, 0x0c,
];
const KDF_ARGON2ID: [u8; 16] = [
    0x9e, 0x29, 0x8b, 0x19, 0x56, 0xdb, 0x47, 0x73, 0xb2, 0x3d, 0xfc, 0x3e, 0xc6, 0xf0, 0xa1, 0xe6,
];

/// Outer header field numbers.
const FIELD_END: u8 = 0;
const FIELD_TRANSFORM_ROUNDS: u8 = 6; // KDBX 3
const FIELD_KDF_PARAMETERS: u8 = 11; // KDBX 4

/// Refuses a vault file whose header asks for more key derivation than the limits allow.
/// `Ok(())` for a file within the limits and for a file that cannot be read as a header.
pub fn check(bytes: &[u8]) -> Result<()> {
    match read_cost(bytes) {
        Some(cost) => cost.within_limits(),
        None => Ok(()),
    }
}

/// What a header asks for.
#[derive(Debug, PartialEq, Eq)]
enum Cost {
    Aes {
        rounds: u64,
    },
    Argon2 {
        memory: u64,
        iterations: u64,
        parallelism: u64,
    },
}

impl Cost {
    fn within_limits(&self) -> Result<()> {
        let refuse = |what: String, limit: String| {
            Err(Error::Invalid(format!(
                "this vault's key derivation asks for {what}, above the limit of {limit} that AKM opens. AKM stops before it derives the key. If the vault is yours, lower its key derivation settings in the program that made it"
            )))
        };
        match *self {
            Self::Aes { rounds } if rounds > MAX_AES_ROUNDS => refuse(
                format!("{rounds} AES-KDF rounds"),
                format!("{MAX_AES_ROUNDS} AES-KDF rounds"),
            ),
            Self::Argon2 { memory, .. } if memory > MAX_ARGON2_MEMORY => refuse(
                format!("{} of Argon2 memory", bytes_text(memory)),
                format!("{} of Argon2 memory", bytes_text(MAX_ARGON2_MEMORY)),
            ),
            Self::Argon2 { iterations, .. } if iterations > MAX_ARGON2_ITERATIONS => refuse(
                format!("{iterations} Argon2 iterations"),
                format!("{MAX_ARGON2_ITERATIONS} Argon2 iterations"),
            ),
            Self::Argon2 { parallelism, .. } if parallelism > MAX_ARGON2_PARALLELISM => refuse(
                format!("{parallelism} Argon2 lanes"),
                format!("{MAX_ARGON2_PARALLELISM} Argon2 lanes"),
            ),
            _ => Ok(()),
        }
    }
}

fn bytes_text(bytes: u64) -> String {
    if bytes >= GIB && bytes.is_multiple_of(GIB) {
        format!("{} GiB", bytes / GIB)
    } else if bytes >= 1 << 20 && bytes.is_multiple_of(1 << 20) {
        format!("{} MiB", bytes >> 20)
    } else {
        format!("{bytes} bytes")
    }
}

/// A cursor over a byte slice that never reads past the end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let part = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(part)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|part| part[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2)
            .map(|part| u16::from_le_bytes([part[0], part[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|part| u32::from_le_bytes([part[0], part[1], part[2], part[3]]))
    }
}

fn le_number(bytes: &[u8]) -> Option<u64> {
    let array: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}

fn read_cost(bytes: &[u8]) -> Option<Cost> {
    let mut cursor = Cursor { bytes, at: 0 };
    if cursor.take(8)? != SIGNATURE {
        return None;
    }
    let _minor = cursor.u16()?;
    let major = cursor.u16()?;
    if major != 3 && major != 4 {
        return None;
    }
    let mut found = None;
    loop {
        let id = cursor.u8()?;
        let length = if major == 3 {
            usize::from(cursor.u16()?)
        } else {
            usize::try_from(cursor.u32()?).ok()?
        };
        let data = cursor.take(length)?;
        match id {
            FIELD_END => break,
            FIELD_TRANSFORM_ROUNDS if major == 3 => {
                found = Some(Cost::Aes {
                    rounds: le_number(data)?,
                });
            }
            FIELD_KDF_PARAMETERS if major == 4 => found = read_parameters(data),
            _ => {}
        }
    }
    found
}

/// The KDF parameters of a KDBX 4 header: a variant dictionary (version 1.0). Entries are a
/// type byte, a key (length and bytes) and a value (length and bytes), up to a zero type byte.
/// Of a name that appears twice, the last one counts, as in the library.
fn read_parameters(data: &[u8]) -> Option<Cost> {
    let mut cursor = Cursor { bytes: data, at: 0 };
    if cursor.u16()? != 0x0100 {
        return None;
    }
    let (mut uuid, mut memory, mut iterations, mut parallelism, mut rounds) =
        (None, None, None, None, None);
    loop {
        let kind = cursor.u8()?;
        if kind == 0 {
            break;
        }
        let key_length = usize::try_from(cursor.u32()?).ok()?;
        let key = cursor.take(key_length)?;
        let value_length = usize::try_from(cursor.u32()?).ok()?;
        let value = cursor.take(value_length)?;
        match (key, kind) {
            (b"$UUID", 0x42) => uuid = Some(value),
            (b"M", 0x05) => memory = le_number(value),
            (b"I", 0x05) => iterations = le_number(value),
            (b"P", 0x04) => {
                let array: [u8; 4] = value.try_into().ok()?;
                parallelism = Some(u64::from(u32::from_le_bytes(array)));
            }
            (b"R", 0x05) => rounds = le_number(value),
            _ => {}
        }
    }
    let uuid = uuid?;
    if uuid == KDF_AES_KDBX3 || uuid == KDF_AES_KDBX4 {
        Some(Cost::Aes { rounds: rounds? })
    } else if uuid == KDF_ARGON2D || uuid == KDF_ARGON2ID {
        Some(Cost::Argon2 {
            memory: memory?,
            iterations: iterations?,
            parallelism: parallelism?,
        })
    } else {
        None
    }
}

/// Builds the first bytes of a KDBX file that carry only the key derivation fields. Tests
/// use it: no memory is allocated for the cost that a header names.
#[cfg(test)]
pub(crate) mod crafted {
    use super::{KDF_AES_KDBX4, KDF_ARGON2D, KDF_ARGON2ID, SIGNATURE};

    fn entry(out: &mut Vec<u8>, kind: u8, key: &str, value: &[u8]) {
        out.push(kind);
        out.extend_from_slice(&(key.len() as u32).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        out.extend_from_slice(value);
    }

    fn kdbx4(parameters: &[u8]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        out.extend_from_slice(&[0x01, 0x00, 0x04, 0x00]); // 4.1
        out.push(11);
        out.extend_from_slice(&(parameters.len() as u32).to_le_bytes());
        out.extend_from_slice(parameters);
        out.push(0);
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&[0x0d, 0x0a, 0x0d, 0x0a]);
        out
    }

    /// A KDBX 4 header with Argon2 (`id` selects Argon2id).
    pub fn argon2(id: bool, memory: u64, iterations: u64, parallelism: u32) -> Vec<u8> {
        let mut dictionary = vec![0x00, 0x01];
        entry(
            &mut dictionary,
            0x42,
            "$UUID",
            if id { &KDF_ARGON2ID } else { &KDF_ARGON2D },
        );
        entry(&mut dictionary, 0x05, "M", &memory.to_le_bytes());
        entry(&mut dictionary, 0x05, "I", &iterations.to_le_bytes());
        entry(&mut dictionary, 0x04, "P", &parallelism.to_le_bytes());
        entry(&mut dictionary, 0x04, "V", &0x13u32.to_le_bytes());
        entry(&mut dictionary, 0x42, "S", &[7u8; 32]);
        dictionary.push(0);
        kdbx4(&dictionary)
    }

    /// A KDBX 4 header with the AES key derivation.
    pub fn aes4(rounds: u64) -> Vec<u8> {
        let mut dictionary = vec![0x00, 0x01];
        entry(&mut dictionary, 0x42, "$UUID", &KDF_AES_KDBX4);
        entry(&mut dictionary, 0x05, "R", &rounds.to_le_bytes());
        entry(&mut dictionary, 0x42, "S", &[7u8; 32]);
        dictionary.push(0);
        kdbx4(&dictionary)
    }

    /// A KDBX 3 header with a transform round count.
    pub fn aes3(rounds: u64) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        out.extend_from_slice(&[0x01, 0x00, 0x03, 0x00]); // 3.1
        out.push(6);
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&rounds.to_le_bytes());
        out.push(0);
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&[0x0d, 0x0a, 0x0d, 0x0a]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_at_each_limit_is_within_it_and_one_over_is_refused() {
        let at = [
            crafted::argon2(false, MAX_ARGON2_MEMORY, 3, 4),
            crafted::argon2(true, 64 << 20, MAX_ARGON2_ITERATIONS, 4),
            crafted::argon2(true, 64 << 20, 3, 256),
            crafted::aes4(MAX_AES_ROUNDS),
            crafted::aes3(MAX_AES_ROUNDS),
        ];
        for bytes in at {
            assert!(check(&bytes).is_ok(), "{:?}", read_cost(&bytes));
        }
        let over = [
            (crafted::argon2(false, MAX_ARGON2_MEMORY + 1, 3, 4), "4 GiB"),
            (crafted::argon2(true, u64::MAX, 3, 4), "4 GiB"),
            (crafted::argon2(true, 64 << 20, 100_001, 4), "100000"),
            (crafted::argon2(true, 64 << 20, 3, 257), "256"),
            (crafted::aes4(MAX_AES_ROUNDS + 1), "2000000000"),
            (crafted::aes3(u64::MAX), "2000000000"),
        ];
        for (bytes, limit) in over {
            let text = check(&bytes).unwrap_err().to_string();
            assert!(text.contains(limit), "{limit}: {text}");
            assert!(text.contains("limit"), "{text}");
        }
    }

    /// The guard must read real files, not only the ones that the builder makes: a parser that
    /// finds nothing would pass every other test and protect nobody.
    #[test]
    fn real_vault_headers_are_read() {
        let fixture = include_bytes!("../tests/fixtures/kdbx40.kdbx");
        let cost = read_cost(fixture).expect("the KeePass fixture has readable KDF parameters");
        assert!(check(fixture).is_ok(), "{cost:?}");

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        crate::Vault::create(&path, "correct horse battery staple", None).unwrap();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(
            read_cost(&written),
            Some(Cost::Argon2 {
                memory: 64 * 1024 * 1024,
                iterations: 3,
                parallelism: 4
            })
        );
        assert!(check(&written).is_ok());
    }

    #[test]
    fn a_header_that_cannot_be_read_is_left_to_the_library() {
        let good = crafted::argon2(false, 64 << 20, 3, 4);
        assert!(check(&[]).is_ok());
        assert!(check(b"not a vault at all").is_ok());
        for cut in 0..good.len() {
            assert!(check(&good[..cut]).is_ok(), "cut at {cut}");
        }
        let mut wrong_major = good.clone();
        wrong_major[10] = 9;
        assert!(check(&wrong_major).is_ok());
        let mut unknown_kdf = crafted::argon2(false, u64::MAX, 3, 4);
        let at = unknown_kdf
            .windows(16)
            .position(|window| window == KDF_ARGON2D)
            .unwrap();
        unknown_kdf[at] ^= 0xff;
        assert!(
            check(&unknown_kdf).is_ok(),
            "an unknown KDF is the library's to report"
        );
        let mut wrong_version = crafted::aes4(u64::MAX);
        wrong_version[8 + 4 + 1 + 4] = 0x02; // the dictionary version
        assert!(check(&wrong_version).is_ok());
    }
}
