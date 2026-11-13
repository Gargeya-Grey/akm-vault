//! A limit on the key derivation cost that a vault file asks for (FR-28).
//!
//! The outer header of a KDBX file says how expensive its key derivation is: Argon2 memory,
//! passes and lanes, or the AES-KDF round count. A crafted file can ask for terabytes or for
//! years of work, and the program that opens it pays before it knows whether the password is
//! right. This module reads only the header, before any key derivation, and refuses a file
//! above a fixed limit.
//!
//! The guard fails closed (core review 1, finding 3). It follows the parse of the pinned
//! `keepass` reader (0.17.0) step by step. A file passes without a cost only where that
//! reader stops before any derivation: fewer than 12 bytes, no KDBX identifier, the KeePass 2
//! pre-release format, an unknown version, or a KeePass 1 file too short for its header. For
//! every other file the guard must read the cost exactly, or it refuses the file. "Exactly"
//! means that each fixed-width number has exactly its width: the reader takes the leading
//! bytes of a longer field and panics on a shorter one, so a padded field could carry a cost
//! past a guard that reads it differently, and a short one would crash AKM.

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

// The bounds on the work of the guard (CORE-R2-06). The header is not authenticated until
// the key is derived, so every count and every length that it states is bounded here, and the
// guard does a fixed amount of work for each field and each entry. KeePassXC, kdbxweb and the
// `keepass` crate write about ten header fields, about eight KDF entries and keys of one to
// five bytes; each bound is far above that. A file beyond a bound is refused before any key
// derivation, with the bound in the message.

/// The most fields of an outer header.
pub const MAX_HEADER_FIELDS: usize = 64;
/// The outer header ends within this many bytes of the start of the file.
pub const MAX_HEADER_BYTES: usize = 1 << 20;
/// The most entries of one variant dictionary (the KDF parameters, the public custom data).
pub const MAX_DICTIONARY_ENTRIES: usize = 64;
/// The longest key of a variant dictionary, in bytes.
pub const MAX_DICTIONARY_KEY_BYTES: usize = 256;
/// The names that the guard reads from the KDF parameters.
const KDF_NAMES: [&[u8]; 7] = [b"$UUID", b"S", b"R", b"V", b"M", b"I", b"P"];

#[cfg(test)]
const SIGNATURE: [u8; 8] = [0x03, 0xD9, 0xA2, 0x9A, 0x67, 0xFB, 0x4B, 0xB5];
const IDENTIFIER: [u8; 4] = [0x03, 0xD9, 0xA2, 0x9A];
const KEEPASS_1_ID: u32 = 0xb54b_fb65;
const KEEPASS_LATEST_ID: u32 = 0xb54b_fb67;
/// The fixed header of a KeePass 1 file; its AES round count is the `u32` at byte 120.
const KDB_HEADER_SIZE: usize = 124;

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

/// Refuses a vault file whose header asks for more key derivation than the limits allow, or
/// whose header the guard cannot read exactly. `Ok(())` for a file within the limits and for a
/// file that the reader refuses before any derivation.
pub fn check(bytes: &[u8]) -> Result<()> {
    match read(bytes) {
        Ok(Header::NoDerivation) => Ok(()),
        Ok(Header::Derives(cost)) => cost.within_limits(),
        Err(Unreadable(why)) => Err(Error::Open(format!(
            "AKM cannot read the key derivation settings in this file's header exactly ({why}), so it stops before it derives a key"
        ))),
    }
}

/// What the reader does with a file before it decrypts anything.
#[derive(Debug, PartialEq, Eq)]
enum Header {
    /// The reader refuses the file before any key derivation.
    NoDerivation,
    /// The reader derives the key at this cost.
    Derives(Cost),
}

/// Why the guard could not read a header that the reader would go on to derive from.
#[derive(Debug, PartialEq, Eq)]
struct Unreadable(&'static str);

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

type Read<T> = std::result::Result<T, Unreadable>;

#[cfg(test)]
thread_local! {
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts the steps of the guard (a field, an entry, a key comparison), so that a test can
/// prove a bound on its work by a count and not by a clock.
fn work(steps: usize) {
    #[cfg(test)]
    WORK.with(|work| work.set(work.get().saturating_add(steps)));
    let _ = steps;
}

const CUT_SHORT: Unreadable = Unreadable("the header is cut short");

/// A cursor over a byte slice that never reads past the end and never overflows.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn take(&mut self, count: usize) -> Read<&'a [u8]> {
        let end = self.at.checked_add(count).ok_or(CUT_SHORT)?;
        let part = self.bytes.get(self.at..end).ok_or(CUT_SHORT)?;
        self.at = end;
        Ok(part)
    }

    fn u8(&mut self) -> Read<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Read<u16> {
        Ok(u16::from_le_bytes(exact(self.take(2)?)?))
    }

    fn u32(&mut self) -> Read<u32> {
        u32_field(self.take(4)?)
    }

    fn length32(&mut self) -> Read<usize> {
        usize::try_from(self.u32()?).map_err(|_| CUT_SHORT)
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }
}

/// A fixed-width number. Any other length is refused: the reader would read a different
/// number (a longer field) or panic (a shorter one).
fn exact<const N: usize>(bytes: &[u8]) -> Read<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| Unreadable("a number field does not have the width of its number"))
}

fn u32_field(bytes: &[u8]) -> Read<u32> {
    Ok(u32::from_le_bytes(exact(bytes)?))
}

fn u64_field(bytes: &[u8]) -> Read<u64> {
    Ok(u64::from_le_bytes(exact(bytes)?))
}

/// `DatabaseVersion::parse` and `Database::parse` of the reader, then the header of the format
/// that they select.
fn read(bytes: &[u8]) -> Read<Header> {
    let Some(version) = bytes.get(..12) else {
        return Ok(Header::NoDerivation);
    };
    if version[..4] != IDENTIFIER {
        return Ok(Header::NoDerivation);
    }
    let id = u32::from_le_bytes([version[4], version[5], version[6], version[7]]);
    let major = u16::from_le_bytes([version[10], version[11]]);
    match id {
        KEEPASS_1_ID => Ok(read_kdb(bytes)),
        KEEPASS_LATEST_ID if major == 3 || major == 4 => {
            read_kdbx(bytes, major).map(Header::Derives)
        }
        // The pre-release format is unsupported, and other versions are invalid: the reader
        // stops at the version.
        _ => Ok(Header::NoDerivation),
    }
}

/// KeePass 1: a fixed header. The reader refuses a shorter file, and otherwise derives with
/// the 32-bit round count at byte 120.
fn read_kdb(bytes: &[u8]) -> Header {
    match bytes.get(120..KDB_HEADER_SIZE) {
        Some(rounds) => Header::Derives(Cost::Aes {
            rounds: u64::from(u32::from_le_bytes([
                rounds[0], rounds[1], rounds[2], rounds[3],
            ])),
        }),
        None => Header::NoDerivation,
    }
}

/// The outer header of KDBX 3 (field lengths in 16 bits) or KDBX 4 (32 bits), field by field
/// as the reader parses it. A field that appears twice counts the last time, as in the reader.
/// At most [`MAX_HEADER_FIELDS`] fields, ending within [`MAX_HEADER_BYTES`].
fn read_kdbx(bytes: &[u8], major: u16) -> Read<Cost> {
    let mut cursor = Cursor::new(bytes, 12);
    let mut cost = None;
    for _ in 0..MAX_HEADER_FIELDS {
        work(1);
        let id = cursor.u8()?;
        let length = if major == 3 {
            usize::from(cursor.u16()?)
        } else {
            cursor.length32()?
        };
        if cursor.at.saturating_add(length) > MAX_HEADER_BYTES {
            return Err(Unreadable(
                "the header is longer than the 1 MiB that AKM reads",
            ));
        }
        let data = cursor.take(length)?;
        match (major, id) {
            (_, 0) => return cost.ok_or(Unreadable("the header names no key derivation")),
            // Comment, cipher, master seed, IV; KDBX 3 also transform seed, stream key and
            // stream start bytes. Byte strings of any length.
            (_, 1 | 2 | 4 | 7) | (3, 5 | 8 | 9) => {}
            // Compression flags; KDBX 3 inner random stream.
            (_, 3) | (3, 10) => {
                u32_field(data)?;
            }
            (3, 6) => {
                cost = Some(Cost::Aes {
                    rounds: u64_field(data)?,
                });
            }
            (4, 11) => cost = Some(read_kdf_parameters(data)?),
            (4, 12) => {
                read_dictionary(data)?;
            }
            _ => return Err(Unreadable("the header has a field that the reader refuses")),
        }
    }
    Err(Unreadable(
        "the header has more than the 64 fields that AKM reads",
    ))
}

/// One value of a variant dictionary: its type byte and its bytes.
#[derive(Clone, Copy)]
struct Value<'a> {
    kind: u8,
    bytes: &'a [u8],
}

const U32: u8 = 0x04;
const U64: u8 = 0x05;
const BOOL: u8 = 0x08;
const I32: u8 = 0x0c;
const I64: u8 = 0x0d;
const STRING: u8 = 0x18;
const BYTES: u8 = 0x42;

/// The last value of each name of [`KDF_NAMES`] that a dictionary holds.
type Named<'a> = [Option<Value<'a>>; KDF_NAMES.len()];

/// A KDBX 4 variant dictionary (version 1.0), parsed as `VariantDictionary::parse` does it:
/// entries while at least ten bytes remain, then a zero type byte. Every number must have its
/// width. At most [`MAX_DICTIONARY_ENTRIES`] entries, each key at most
/// [`MAX_DICTIONARY_KEY_BYTES`].
///
/// Of a key that appears twice, the last one counts, as in the reader. The guard reads only
/// the names of [`KDF_NAMES`], so it keeps the last value of each of them in its own slot and
/// compares each key with those names only: the work for one entry is fixed, and never grows
/// with the number of entries before it (CORE-R2-06). The reader keys the entries by the text
/// of the key (lossy UTF-8); no other byte key gives one of these ASCII names, so comparing
/// bytes is the same.
fn read_dictionary(data: &[u8]) -> Read<Named<'_>> {
    let mut cursor = Cursor::new(data, 0);
    if cursor.u16()? != 0x0100 {
        return Err(Unreadable("the KDF dictionary has an unknown version"));
    }
    let mut named: Named<'_> = [None; KDF_NAMES.len()];
    let mut entries = 0usize;
    while cursor.remaining() > 9 {
        entries += 1;
        if entries > MAX_DICTIONARY_ENTRIES {
            return Err(Unreadable(
                "a variant dictionary has more than the 64 entries that AKM reads",
            ));
        }
        work(1);
        let kind = cursor.u8()?;
        let key_length = cursor.length32()?;
        if key_length > MAX_DICTIONARY_KEY_BYTES {
            return Err(Unreadable(
                "a variant dictionary has a key longer than the 256 bytes that AKM reads",
            ));
        }
        let key = cursor.take(key_length)?;
        let value_length = cursor.length32()?;
        let bytes = cursor.take(value_length)?;
        match kind {
            U32 | I32 => {
                u32_field(bytes)?;
            }
            U64 | I64 => {
                u64_field(bytes)?;
            }
            BOOL | STRING | BYTES => {}
            _ => return Err(Unreadable("the KDF dictionary has an unknown value type")),
        }
        let slot = KDF_NAMES.iter().position(|name| {
            work(1);
            *name == key
        });
        if let Some(slot) = slot {
            named[slot] = Some(Value { kind, bytes });
        }
    }
    if cursor.u8().ok() != Some(0) {
        return Err(Unreadable("the KDF dictionary has no end"));
    }
    Ok(named)
}

/// The reader's `TryFrom<VariantDictionary>` for the KDF: the KDF UUID selects the numbers,
/// and each must be present with its type.
fn read_kdf_parameters(data: &[u8]) -> Read<Cost> {
    let named = read_dictionary(data)?;
    let get = |name: &[u8], kind: u8| {
        KDF_NAMES
            .iter()
            .position(|known| *known == name)
            .and_then(|slot| named[slot])
            .filter(|value| value.kind == kind)
            .map(|value| value.bytes)
            .ok_or(Unreadable(
                "the KDF dictionary lacks a value, or has it with another type",
            ))
    };
    let uuid = get(b"$UUID", BYTES)?;
    if uuid == KDF_AES_KDBX3 || uuid == KDF_AES_KDBX4 {
        get(b"S", BYTES)?;
        return Ok(Cost::Aes {
            rounds: u64_field(get(b"R", U64)?)?,
        });
    }
    if uuid == KDF_ARGON2D || uuid == KDF_ARGON2ID {
        get(b"S", BYTES)?;
        let version = u32_field(get(b"V", U32)?)?;
        if version != 0x10 && version != 0x13 {
            return Err(Unreadable("the Argon2 version is unknown"));
        }
        return Ok(Cost::Argon2 {
            memory: u64_field(get(b"M", U64)?)?,
            iterations: u64_field(get(b"I", U64)?)?,
            parallelism: u64::from(u32_field(get(b"P", U32)?)?),
        });
    }
    Err(Unreadable("the key derivation function is unknown"))
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
        aes3_raw(&rounds.to_le_bytes())
    }

    /// A KDBX 3 header whose transform round field holds `rounds` as given.
    pub fn aes3_raw(rounds: &[u8]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        out.extend_from_slice(&[0x01, 0x00, 0x03, 0x00]); // 3.1
        out.push(6);
        out.extend_from_slice(&(rounds.len() as u16).to_le_bytes());
        out.extend_from_slice(rounds);
        out.push(0);
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&[0x0d, 0x0a, 0x0d, 0x0a]);
        out
    }

    /// A KDBX 4 header whose KDF dictionary holds these (type, key, value) entries.
    pub fn kdbx4_entries(entries: &[(u8, &str, Vec<u8>)]) -> Vec<u8> {
        let mut dictionary = vec![0x00, 0x01];
        for (kind, key, value) in entries {
            entry(&mut dictionary, *kind, key, value);
        }
        dictionary.push(0);
        kdbx4(&dictionary)
    }

    /// A usual Argon2id header in which the entry `key` holds `value` as given.
    pub fn argon2_entries(key: &str, value: Vec<u8>) -> Vec<u8> {
        let mut entries = vec![
            (0x42, "$UUID", KDF_ARGON2ID.to_vec()),
            (0x05, "M", (64u64 << 20).to_le_bytes().to_vec()),
            (0x05, "I", 3u64.to_le_bytes().to_vec()),
            (0x04, "P", 4u32.to_le_bytes().to_vec()),
            (0x04, "V", 0x13u32.to_le_bytes().to_vec()),
            (0x42, "S", vec![7u8; 32]),
        ];
        for (_, name, stored) in &mut entries {
            if *name == key {
                *stored = value.clone();
            }
        }
        kdbx4_entries(&entries)
    }

    /// The 124-byte header of a KeePass 1 (KDB) file with its AES round count.
    pub fn kdb(rounds: u32) -> Vec<u8> {
        let mut out = vec![0x03, 0xd9, 0xa2, 0x9a, 0x65, 0xfb, 0x4b, 0xb5];
        out.resize(120, 0);
        out.extend_from_slice(&rounds.to_le_bytes());
        out
    }
}

/// The fields of a real KDBX 4 or KDBX 3 file, rewritten. Only the header changes: a KDBX 4
/// file gets a new header hash, so the library reads it, derives the key, and then fails
/// at the header HMAC with "Incorrect key". That error is proof that it derived.
#[cfg(test)]
pub(crate) mod rewrite {
    use sha2::{Digest, Sha256};

    pub struct Header {
        pub version: Vec<u8>,
        pub major: u16,
        pub fields: Vec<(u8, Vec<u8>)>,
        pub tail: Vec<u8>,
    }

    pub fn read(bytes: &[u8]) -> Header {
        let major = u16::from_le_bytes([bytes[10], bytes[11]]);
        let wide = major == 4;
        let mut at = 12;
        let mut fields = Vec::new();
        loop {
            let id = bytes[at];
            let (length, start) = if wide {
                (
                    u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize,
                    at + 5,
                )
            } else {
                (
                    usize::from(u16::from_le_bytes([bytes[at + 1], bytes[at + 2]])),
                    at + 3,
                )
            };
            fields.push((id, bytes[start..start + length].to_vec()));
            at = start + length;
            if id == 0 {
                break;
            }
        }
        // KDBX 4: the header hash (32 bytes) follows; it is recomputed on write.
        let tail = if wide {
            bytes[at + 32..].to_vec()
        } else {
            bytes[at..].to_vec()
        };
        Header {
            version: bytes[..12].to_vec(),
            major,
            fields,
            tail,
        }
    }

    pub fn write(header: &Header) -> Vec<u8> {
        let mut out = header.version.clone();
        for (id, data) in &header.fields {
            out.push(*id);
            if header.major == 4 {
                out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            } else {
                out.extend_from_slice(&(data.len() as u16).to_le_bytes());
            }
            out.extend_from_slice(data);
        }
        if header.major == 4 {
            let hash = Sha256::digest(&out);
            out.extend_from_slice(&hash);
        }
        out.extend_from_slice(&header.tail);
        out
    }

    /// A variant dictionary as (type, key, value) entries.
    pub fn entries(data: &[u8]) -> Vec<(u8, Vec<u8>, Vec<u8>)> {
        let mut at = 2;
        let mut out = Vec::new();
        while data[at] != 0 {
            let kind = data[at];
            let key_length = u32::from_le_bytes(data[at + 1..at + 5].try_into().unwrap()) as usize;
            let key = data[at + 5..at + 5 + key_length].to_vec();
            at += 5 + key_length;
            let value_length = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
            let value = data[at + 4..at + 4 + value_length].to_vec();
            at += 4 + value_length;
            out.push((kind, key, value));
        }
        out
    }

    pub fn dictionary(entries: &[(u8, Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0x00, 0x01];
        for (kind, key, value) in entries {
            out.push(*kind);
            out.extend_from_slice(&(key.len() as u32).to_le_bytes());
            out.extend_from_slice(key);
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(value);
        }
        out.push(0);
        out
    }

    /// The file with the KDF parameter `key` stored in `length` bytes. The value keeps its
    /// low bytes, so a padded value means what it meant before to a reader that takes the
    /// leading bytes.
    pub fn kdf_field(bytes: &[u8], key: &str, length: usize) -> Vec<u8> {
        let mut header = read(bytes);
        let (_, data) = header
            .fields
            .iter_mut()
            .find(|(id, _)| *id == 11)
            .expect("KDF parameters");
        let mut entries = entries(data);
        let (_, _, value) = entries
            .iter_mut()
            .find(|(_, name, _)| name == key.as_bytes())
            .unwrap_or_else(|| panic!("no {key}"));
        value.resize(length, 0);
        *data = dictionary(&entries);
        write(&header)
    }

    /// A KDBX 3 file with the transform rounds stored in `length` bytes.
    pub fn kdbx3_rounds(bytes: &[u8], length: usize) -> Vec<u8> {
        let mut header = read(bytes);
        let (_, data) = header
            .fields
            .iter_mut()
            .find(|(id, _)| *id == 6)
            .expect("transform rounds");
        data.resize(length, 0);
        write(&header)
    }

    /// The KDF parameter `key` set to `value` in its usual width.
    pub fn kdf_value(bytes: &[u8], key: &str, value: u64) -> Vec<u8> {
        let mut header = read(bytes);
        let (_, data) = header.fields.iter_mut().find(|(id, _)| *id == 11).unwrap();
        let mut entries = entries(data);
        let (_, _, stored) = entries
            .iter_mut()
            .find(|(_, name, _)| name == key.as_bytes())
            .unwrap();
        let width = stored.len();
        *stored = value.to_le_bytes()[..width].to_vec();
        *data = dictionary(&entries);
        write(&header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted<T>(run: impl FnOnce() -> T) -> (T, usize) {
        WORK.with(|work| work.set(0));
        let result = run();
        (result, WORK.with(std::cell::Cell::get))
    }

    /// A usual AES header whose KDF dictionary also holds `extra` distinct keys that the
    /// reader ignores.
    fn many_entries(extra: usize) -> Vec<u8> {
        let mut entries = vec![
            (0x42, "$UUID".to_string(), KDF_AES_KDBX4.to_vec()),
            (0x05, "R".to_string(), 1000u64.to_le_bytes().to_vec()),
            (0x42, "S".to_string(), vec![7u8; 32]),
        ];
        for index in 0..extra {
            entries.push((0x42, format!("u{index:07}"), Vec::new()));
        }
        let borrowed: Vec<(u8, &str, Vec<u8>)> = entries
            .iter()
            .map(|(kind, key, value)| (*kind, key.as_str(), value.clone()))
            .collect();
        crafted::kdbx4_entries(&borrowed)
    }

    /// CORE-R2-06, the reviewer's closing test: a header whose KDF dictionary holds 60,000
    /// entries (about 1 MB) gets a controlled refusal, and the guard's work stays within a
    /// fixed bound. The bound is a count of the steps of the guard, never a clock.
    #[test]
    fn a_kdf_dictionary_with_very_many_entries_is_refused_with_bounded_work() {
        let header = many_entries(60_000);
        // The size of the reviewer's header, under the 1 MiB bound: the entry limit refuses it.
        assert!(header.len() > 1_000_000, "{}", header.len());
        assert!(header.len() < MAX_HEADER_BYTES, "{}", header.len());
        let (result, steps) = counted(|| check(&header));
        let error = result.expect_err("a dictionary of 60,000 entries is refused");
        assert!(error.to_string().contains("stops before"), "{error}");
        assert!(error.to_string().contains("64 entries"), "{error}");
        let bound = (MAX_HEADER_FIELDS + MAX_DICTIONARY_ENTRIES) * (2 + KDF_NAMES.len());
        assert!(steps <= bound, "{steps} steps, bound {bound}");
    }

    /// CORE-R2-06: under the limits, the work grows with the number of entries, not with its
    /// square: each entry costs at most one step and one comparison per name that is read.
    #[test]
    fn the_guard_does_linear_work_on_a_dictionary_within_the_limits() {
        for extra in [0usize, 10, 30, MAX_DICTIONARY_ENTRIES - 3] {
            let header = many_entries(extra);
            let (result, steps) = counted(|| check(&header));
            assert!(result.is_ok(), "{extra}: {result:?}");
            let entries = extra + 3;
            let bound = MAX_HEADER_FIELDS + entries * (1 + KDF_NAMES.len());
            assert!(
                steps <= bound,
                "{extra} extra entries: {steps} steps, bound {bound}"
            );
        }
        let over = many_entries(MAX_DICTIONARY_ENTRIES - 2);
        let text = check(&over).unwrap_err().to_string();
        assert!(text.contains(&MAX_DICTIONARY_ENTRIES.to_string()), "{text}");
    }

    /// A usual AES header with `count` comment fields of `length` bytes before its KDF field.
    fn with_comments(count: usize, length: usize) -> Vec<u8> {
        let mut bytes = crafted::aes4(1000);
        let mut fields = Vec::new();
        for _ in 0..count {
            fields.push(1u8);
            fields.extend_from_slice(&(length as u32).to_le_bytes());
            fields.resize(fields.len() + length, b'c');
        }
        bytes.splice(12..12, fields);
        bytes
    }

    /// CORE-R2-06: each other bound of the header holds at its value and refuses one past it,
    /// and the refusal names the bound: the number of fields, the length of the header and
    /// the length of a dictionary key.
    #[test]
    fn each_bound_of_the_header_holds_at_its_value_and_refuses_one_past_it() {
        let refused_with = |bytes: &[u8], bound: &str| {
            let text = check(bytes).unwrap_err().to_string();
            assert!(
                text.contains("stops before") && text.contains(bound),
                "{text}"
            );
        };
        // The KDF field and the end field are two of the fields.
        assert!(check(&with_comments(MAX_HEADER_FIELDS - 2, 0)).is_ok());
        refused_with(
            &with_comments(MAX_HEADER_FIELDS - 1, 0),
            &MAX_HEADER_FIELDS.to_string(),
        );

        assert_eq!(MAX_HEADER_BYTES, 1 << 20, "the message says 1 MiB");
        let usual = crafted::aes4(1000).len();
        let room = MAX_HEADER_BYTES - usual - 5;
        assert!(check(&with_comments(1, room)).is_ok());
        refused_with(&with_comments(1, room + 1), "1 MiB");

        let keyed = |length: usize| {
            crafted::kdbx4_entries(&[
                (0x42, "$UUID", KDF_AES_KDBX4.to_vec()),
                (0x05, "R", 1000u64.to_le_bytes().to_vec()),
                (0x42, "S", vec![7; 32]),
                (0x42, &"k".repeat(length), Vec::new()),
            ])
        };
        assert!(check(&keyed(MAX_DICTIONARY_KEY_BYTES)).is_ok());
        refused_with(
            &keyed(MAX_DICTIONARY_KEY_BYTES + 1),
            &MAX_DICTIONARY_KEY_BYTES.to_string(),
        );
    }

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
            assert!(check(&bytes).is_ok(), "{:?}", read(&bytes));
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
        let cost = read(fixture);
        assert!(
            matches!(cost, Ok(Header::Derives(Cost::Aes { rounds: 10 }))),
            "the KeePass fixture has readable KDF parameters: {cost:?}"
        );
        assert!(check(fixture).is_ok(), "{cost:?}");
        let kdbx3 = include_bytes!("../tests/fixtures/kdbx3.kdbx");
        assert_eq!(read(kdbx3), Ok(Header::Derives(Cost::Aes { rounds: 10 })));

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.kdbx");
        crate::Vault::create(&path, "correct horse battery staple", None).unwrap();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(
            read(&written),
            Ok(Header::Derives(Cost::Argon2 {
                memory: 64 * 1024 * 1024,
                iterations: 3,
                parallelism: 4
            }))
        );
        assert!(check(&written).is_ok());
    }

    /// The value of a little-endian number in `length` bytes: cut short, or padded with zeros.
    fn sized(value: u64, length: usize) -> Vec<u8> {
        let mut bytes = value.to_le_bytes().to_vec();
        bytes.resize(length, 0);
        bytes
    }

    /// What the pinned library does with a file: `Derived` when it reached the key derivation
    /// (it then fails at the header HMAC that the rewrite broke), `Panicked`, or `Refused`
    /// with another error before any derivation.
    #[derive(Debug, PartialEq, Eq)]
    enum Library {
        Derived,
        Panicked,
        Refused(String),
    }

    fn library(bytes: &[u8]) -> Library {
        let key = keepass::DatabaseKey::new().with_password("correct horse battery staple");
        let bytes = bytes.to_vec();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(move || keepass::Database::parse(&bytes, key));
        std::panic::set_hook(previous);
        match outcome {
            Err(_) => Library::Panicked,
            Ok(Ok(_)) => Library::Derived,
            Ok(Err(error)) if error.to_string() == "Incorrect key" => Library::Derived,
            Ok(Err(error)) => Library::Refused(error.to_string()),
        }
    }

    /// A small Argon2 vault written by the library: its derivation is cheap.
    fn cheap_argon2_vault() -> Vec<u8> {
        let mut db = keepass::Database::new();
        db.config.version = keepass::config::DatabaseVersion::KDB4(1);
        db.config.kdf_config = keepass::config::KdfConfig::Argon2id {
            iterations: 1,
            memory: 1 << 20,
            parallelism: 1,
            version: argon2::Version::Version13,
        };
        let mut bytes = Vec::new();
        db.save(
            &mut bytes,
            keepass::DatabaseKey::new().with_password("correct horse battery staple"),
        )
        .unwrap();
        bytes
    }

    /// FR-28, core review 1 finding 3: the library reads a numeric KDF field from its leading
    /// bytes when the field is longer than the number, and panics when it is shorter. The
    /// guard reads only the exact width and refuses every other length, so a padded field
    /// cannot carry a cost past it, and a short one cannot crash AKM.
    #[test]
    fn a_numeric_kdf_field_of_any_length_but_its_width_is_refused() {
        let at_limit = [
            ("KDBX 4 AES rounds", 8usize),
            ("Argon2 memory", 8),
            ("Argon2 iterations", 8),
            ("Argon2 lanes", 4),
            ("KDBX 3 rounds", 8),
        ];
        for (name, width) in at_limit {
            for length in 0..=16usize {
                let bytes = match name {
                    "KDBX 4 AES rounds" => crafted::kdbx4_entries(&[
                        (0x42, "$UUID", KDF_AES_KDBX4.to_vec()),
                        (0x05, "R", sized(1000, length)),
                        (0x42, "S", vec![7; 32]),
                    ]),
                    "Argon2 memory" => crafted::argon2_entries("M", sized(64 << 20, length)),
                    "Argon2 iterations" => crafted::argon2_entries("I", sized(3, length)),
                    "Argon2 lanes" => crafted::argon2_entries("P", sized(4, length)),
                    _ => crafted::aes3_raw(&sized(1000, length)),
                };
                let result = check(&bytes);
                if length == width {
                    assert!(result.is_ok(), "{name}, {length} bytes: {result:?}");
                } else {
                    let error = result.expect_err(&format!("{name}, {length} bytes passed"));
                    assert!(!error.to_string().contains("panicked"), "{error}");
                }
            }
        }
    }

    /// FR-28: a differential test against the pinned `keepass` reader on real files. For each
    /// numeric KDF field and each length from 0 to 16 bytes, either the guard refuses the file,
    /// or the library reads exactly the number that the guard checked. A file the library
    /// would derive from is never passed with a number the guard did not read.
    #[test]
    fn the_guard_passes_a_field_only_at_the_length_where_it_reads_what_the_library_reads() {
        let aes4 = include_bytes!("../tests/fixtures/kdbx40.kdbx").to_vec();
        let aes3 = include_bytes!("../tests/fixtures/kdbx3.kdbx").to_vec();
        let argon2 = cheap_argon2_vault();
        let cases: [(&str, &[u8], usize); 5] = [
            ("R", &aes4, 8),
            ("M", &argon2, 8),
            ("I", &argon2, 8),
            ("P", &argon2, 4),
            ("KDBX 3 rounds", &aes3, 8),
        ];
        let mut derived_with_other_length = 0;
        for (field, source, width) in cases {
            for length in 0..=16usize {
                let bytes = if field == "KDBX 3 rounds" {
                    rewrite::kdbx3_rounds(source, length)
                } else {
                    rewrite::kdf_field(source, field, length)
                };
                let library = library(&bytes);
                let guard = check(&bytes);
                if length == width {
                    assert_eq!(library, Library::Derived, "{field} at its width");
                    assert!(guard.is_ok(), "{field} at its width: {guard:?}");
                    continue;
                }
                if library == Library::Derived {
                    derived_with_other_length += 1;
                }
                assert!(
                    guard.is_err(),
                    "{field} in {length} bytes passed the guard; the library gave {library:?}"
                );
            }
        }
        assert!(
            derived_with_other_length > 0,
            "the library must accept a padded field, or this test proves nothing"
        );
    }

    /// FR-28: the cost in a padded field is refused on open and on reload, before any key
    /// derivation (2000000001 AES rounds would take far longer than the deadline).
    #[test]
    fn a_padded_cost_is_refused_quickly_on_open_and_on_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("padded.kdbx");
        let fixture = include_bytes!("../tests/fixtures/kdbx40.kdbx");
        let over = rewrite::kdf_value(fixture, "R", MAX_AES_ROUNDS + 1);
        let padded = rewrite::kdf_field(&over, "R", 9);
        let started = std::time::Instant::now();
        std::fs::write(&path, &padded).unwrap();
        assert!(crate::Vault::open(&path, "correct horse battery staple").is_err());
        std::fs::write(&path, fixture).unwrap();
        let mut vault = crate::Vault::open(&path, "correct horse battery staple").unwrap();
        std::fs::write(&path, &padded).unwrap();
        assert!(vault.reload().is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "refused only after {:?}",
            started.elapsed()
        );
    }

    /// FR-28: a KeePass 1 file names its AES rounds in a 32-bit field, which can hold more
    /// than the limit. The library derives from it, so the guard holds it to the same limit.
    #[test]
    fn a_keepass_1_file_is_held_to_the_aes_round_limit() {
        let over = crafted::kdb(u32::MAX);
        let text = check(&over).unwrap_err().to_string();
        assert!(text.contains("2000000000"), "{text}");
        assert!(check(&crafted::kdb(6000)).is_ok());
        assert!(
            check(&crafted::kdb(6000)[..123]).is_ok(),
            "too short to derive"
        );
    }

    /// Fail closed: a file passes without a cost only where the reader stops at the version
    /// (too short, not KDBX, the pre-release format, an unknown version). A KDBX 3 or 4 header
    /// that is cut short, malformed, or names an unknown KDF is refused before any derivation.
    #[test]
    fn a_header_the_guard_cannot_read_is_refused_and_one_the_reader_stops_at_passes() {
        let good = crafted::argon2(false, 64 << 20, 3, 4);
        assert!(check(&[]).is_ok());
        assert!(check(b"not a vault at all").is_ok());
        assert!(check(&good[..11]).is_ok(), "shorter than the version");
        let mut wrong_major = good.clone();
        wrong_major[10] = 9;
        assert!(
            check(&wrong_major).is_ok(),
            "the reader stops at the version"
        );
        let mut prerelease = good.clone();
        prerelease[4] = 0x66;
        assert!(check(&prerelease).is_ok(), "the reader refuses the format");

        let refused = |bytes: &[u8], what: &str| {
            let error = check(bytes).expect_err(what);
            assert!(
                matches!(error, Error::Open(_)) && error.to_string().contains("stops before"),
                "{what}: {error}"
            );
        };
        for cut in 12..good.len() - 4 {
            refused(&good[..cut], &format!("cut at {cut}"));
        }
        let mut unknown_kdf = crafted::argon2(false, u64::MAX, 3, 4);
        let at = unknown_kdf
            .windows(16)
            .position(|window| window == KDF_ARGON2D)
            .unwrap();
        unknown_kdf[at] ^= 0xff;
        refused(&unknown_kdf, "an unknown KDF");
        let mut wrong_version = crafted::aes4(u64::MAX);
        wrong_version[8 + 4 + 1 + 4] = 0x02; // the dictionary version
        refused(&wrong_version, "an unknown dictionary version");
        refused(
            &crafted::kdbx4_entries(&[
                (0x42, "$UUID", KDF_AES_KDBX4.to_vec()),
                (0x09, "R", u64::MAX.to_le_bytes().to_vec()),
            ]),
            "an unknown value type",
        );
        refused(
            &crafted::kdbx4_entries(&[
                (0x42, "$UUID", KDF_AES_KDBX4.to_vec()),
                (0x04, "R", 1u32.to_le_bytes().to_vec()),
                (0x42, "S", vec![7; 32]),
            ]),
            "rounds with another type",
        );
        refused(
            &crafted::kdbx4_entries(&[(0x42, "$UUID", KDF_AES_KDBX4.to_vec())]),
            "no rounds",
        );
        // The last of two entries counts, as in the reader.
        let twice = crafted::kdbx4_entries(&[
            (0x42, "$UUID", KDF_AES_KDBX4.to_vec()),
            (0x05, "R", 1u64.to_le_bytes().to_vec()),
            (0x42, "S", vec![7; 32]),
            (0x05, "R", u64::MAX.to_le_bytes().to_vec()),
        ]);
        assert!(check(&twice)
            .unwrap_err()
            .to_string()
            .contains("2000000000"));
    }
}
