//! Versioned envelope around every persisted file (ADR-0006).

use serde::{Serialize, de::DeserializeOwned};

/// Which kind of file an envelope holds. The discriminant is the on-disk `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileKind {
    World = 1,
    Scene = 2,
    Chunk = 3,
}

impl FileKind {
    /// Newest format version this build writes and reads for the kind.
    pub fn current_version(self) -> u16 {
        match self {
            Self::World | Self::Scene | Self::Chunk => 1,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::World),
            2 => Some(Self::Scene),
            3 => Some(Self::Chunk),
            _ => None,
        }
    }
}

const MAGIC: &[u8; 4] = b"MOHO";
const HEADER_LEN: usize = 19;

/// Why an envelope could not be written or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PersistError {
    #[error("not a save file")]
    NotASaveFile,
    #[error("unsupported format: {kind:?} version {version}")]
    UnsupportedFormat { kind: FileKind, version: u16 },
    #[error("wrong file kind: expected {expected:?}, found {found:?}")]
    WrongKind { expected: FileKind, found: FileKind },
    #[error("corrupt file")]
    Corrupt,
    #[error("encode failed: {0}")]
    Encode(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Wrap `value` in an envelope of the given kind.
pub fn encode<T: Serialize>(kind: FileKind, value: &T) -> Result<Vec<u8>, PersistError> {
    let payload = postcard::to_allocvec(value).map_err(|e| PersistError::Encode(e.to_string()))?;
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(kind as u8);
    out.extend_from_slice(&kind.current_version().to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Validate the envelope and decode its payload as `expected`.
pub fn decode<T: DeserializeOwned>(expected: FileKind, bytes: &[u8]) -> Result<T, PersistError> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
        return Err(PersistError::NotASaveFile);
    }
    let kind = FileKind::from_byte(bytes[4]).ok_or(PersistError::NotASaveFile)?;
    let version = u16::from_le_bytes([bytes[5], bytes[6]]);
    if version == 0 || version > kind.current_version() {
        return Err(PersistError::UnsupportedFormat { kind, version });
    }
    if kind != expected {
        return Err(PersistError::WrongKind {
            expected,
            found: kind,
        });
    }
    let declared = u64::from_le_bytes(bytes[7..15].try_into().expect("8-byte slice"));
    let crc = u32::from_le_bytes(bytes[15..19].try_into().expect("4-byte slice"));
    let payload = &bytes[HEADER_LEN..];
    if declared != payload.len() as u64 || crc32fast::hash(payload) != crc {
        return Err(PersistError::Corrupt);
    }
    let (value, rest) = postcard::take_from_bytes(payload).map_err(|_| PersistError::Corrupt)?;
    if rest.is_empty() {
        Ok(value)
    } else {
        Err(PersistError::Corrupt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const HEADER_LEN: usize = 19;
    const PAYLOAD: [u8; 4] = [7, 2, b'h', b'i'];
    const KIND: FileKind = FileKind { tag: 1, version: 2 };

    fn fixture() -> (u32, String) {
        (7, "hi".to_string())
    }

    fn valid(kind: FileKind) -> Vec<u8> {
        encode(kind, &fixture()).expect("encode fixture")
    }

    fn envelope_around(tag: u8, version: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = b"MOHO".to_vec();
        out.push(tag);
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        out.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn reject(expected: FileKind, bytes: &[u8]) -> PersistError {
        decode::<(u32, String)>(expected, bytes).expect_err("must reject")
    }

    #[test]
    fn envelope_bytes_are_stable() {
        let bytes = valid(FileKind { tag: 1, version: 1 });

        assert_eq!(
            bytes,
            [
                b'M', b'O', b'H', b'O', // magic
                1,    // kind tag
                1, 0, // version 1
                4, 0, 0, 0, 0, 0, 0, 0, // payload length
                152, 11, 93, 38, // CRC32 of the payload (little-endian)
                7, 2, b'h', b'i', // postcard payload
            ]
        );
    }

    #[test]
    fn caller_defined_kind_round_trips() {
        let kind = FileKind {
            tag: 200,
            version: 3,
        };
        let bytes = valid(kind);

        let back: (u32, String) = decode(kind, &bytes).expect("decode");

        assert_eq!(back, fixture());
        assert_eq!(bytes[4], 200);
        assert_eq!(&bytes[5..7], &3u16.to_le_bytes());
    }

    #[test]
    fn older_version_still_decodes() {
        let bytes = envelope_around(KIND.tag, KIND.version - 1, &PAYLOAD);

        let back: (u32, String) = decode(KIND, &bytes).expect("decode");

        assert_eq!(back, fixture());
    }

    #[test]
    fn header_only_envelope_round_trips_an_empty_payload() {
        let bytes = encode(KIND, &()).expect("encode");

        assert_eq!(bytes.len(), HEADER_LEN);
        decode::<()>(KIND, &bytes).expect("decode");
    }

    #[test]
    fn wrong_magic_returns_not_a_save_file() {
        let mut bytes = envelope_around(KIND.tag, KIND.version, &PAYLOAD);
        bytes[0] = b'X';

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::NotASaveFile), "{err:?}");
    }

    #[test]
    fn version_above_expected_returns_unsupported_format() {
        let bytes = envelope_around(1, 3, &PAYLOAD);

        let err = reject(KIND, &bytes);

        assert!(
            matches!(err, PersistError::UnsupportedFormat { tag: 1, version: 3 }),
            "{err:?}"
        );
    }

    #[test]
    fn version_zero_returns_unsupported_format() {
        let bytes = envelope_around(1, 0, &PAYLOAD);

        let err = reject(KIND, &bytes);

        assert!(
            matches!(err, PersistError::UnsupportedFormat { tag: 1, version: 0 }),
            "{err:?}"
        );
    }

    #[test]
    fn other_tag_returns_wrong_kind() {
        let bytes = envelope_around(7, 1, &PAYLOAD);

        let err = reject(KIND, &bytes);

        assert!(
            matches!(
                err,
                PersistError::WrongKind {
                    expected: 1,
                    found: 7
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn version_zero_is_reported_before_wrong_kind() {
        let bytes = envelope_around(7, 0, &PAYLOAD);

        let err = reject(KIND, &bytes);

        assert!(
            matches!(err, PersistError::UnsupportedFormat { tag: 7, version: 0 }),
            "{err:?}"
        );
    }

    #[test]
    fn flipped_payload_byte_returns_corrupt() {
        let mut bytes = valid(KIND);
        bytes[HEADER_LEN] ^= 0xFF;

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn payload_change_that_still_decodes_returns_corrupt() {
        let mut bytes = valid(KIND);
        *bytes.last_mut().expect("payload byte") = b'j';

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn trailing_bytes_after_declared_length_return_corrupt() {
        let mut bytes = valid(KIND);
        bytes.push(0);

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn payload_with_trailing_bytes_inside_the_envelope_returns_corrupt() {
        let bytes = envelope_around(KIND.tag, KIND.version, &[7, 2, b'h', b'i', 0]);

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    #[test]
    fn payload_that_does_not_decode_as_the_type_returns_corrupt() {
        let bytes = envelope_around(KIND.tag, KIND.version, &[7]);

        let err = reject(KIND, &bytes);

        assert!(matches!(err, PersistError::Corrupt), "{err:?}");
    }

    proptest! {
        #[test]
        fn truncated_input_is_rejected(values in proptest::collection::vec(any::<u32>(), 0..16)) {
            let bytes = encode(KIND, &values).expect("encode");

            for len in 0..bytes.len() {
                let result = decode::<Vec<u32>>(KIND, &bytes[..len]);
                prop_assert!(result.is_err(), "prefix of {len} bytes decoded");
            }
            let full: Vec<u32> = decode(KIND, &bytes).expect("full file decodes");
            prop_assert_eq!(full, values);
        }
    }
}
