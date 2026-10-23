use super::assert_error;
use crate::catalog_revision::CatalogAuthorityError;
use crate::catalog_revision::state::{
    FileIdentity, KEY_FILE_BYTES, MARKER_BYTES, MODE_FIRST_USE, MODE_LEGACY_ADOPTION,
    STATE_RECORD_BYTES, StateRecord, decode_key_file, decode_marker, encode_key_file,
    encode_marker, initial_record_for_test, key_commitment,
};

#[test]
fn marker_layout_preserves_epoch_and_lock_identity_ranges() {
    let epoch = std::array::from_fn(|index| {
        u8::try_from(0x20 + index).expect("marker epoch fixture byte fits")
    });
    let lock_identity = FileIdentity {
        device: 0x0102_0304_0506_0708,
        inode: 0x1112_1314_1516_1718,
    };
    let marker = encode_marker(&epoch, lock_identity);
    let mut expected = [0_u8; MARKER_BYTES];
    expected[0..8].copy_from_slice(b"JCMARK01");
    expected[8..40].copy_from_slice(&epoch);
    expected[40..48].copy_from_slice(&lock_identity.device.to_be_bytes());
    expected[48..56].copy_from_slice(&lock_identity.inode.to_be_bytes());
    assert_eq!(marker, expected);
    let (decoded_epoch, decoded_lock) = decode_marker(&marker).expect("valid marker layout");
    assert_eq!(decoded_epoch, epoch);
    assert_eq!(decoded_lock, lock_identity);

    let mut changed_epoch = marker;
    changed_epoch[8] ^= 0x80;
    assert_ne!(
        decode_marker(&changed_epoch)
            .expect("marker remains structurally valid")
            .0,
        epoch
    );
    assert_error(
        decode_marker(&marker[..MARKER_BYTES - 1]),
        CatalogAuthorityError::RecoveryRequired,
    );
    let mut oversized = marker.to_vec();
    oversized.push(0);
    assert_error(
        decode_marker(&oversized),
        CatalogAuthorityError::RecoveryRequired,
    );
}

#[test]
fn key_layout_is_exact_and_marker_key_epochs_are_distinct_ranges() {
    let key =
        std::array::from_fn(|index| u8::try_from(0x40 + index).expect("key fixture byte fits"));
    let epoch = std::array::from_fn(|index| {
        u8::try_from(0x80 + index).expect("key epoch fixture byte fits")
    });
    let encoded = encode_key_file(&key, &epoch);
    assert_eq!(encoded.len(), KEY_FILE_BYTES);
    assert_eq!(&encoded[0..8], b"JCKEY001");
    assert_eq!(&encoded[8..40], &key);
    assert_eq!(&encoded[40..72], &epoch);
    let (decoded_key, decoded_epoch) = decode_key_file(&encoded[..]).expect("valid key file");
    assert_eq!(*decoded_key, key);
    assert_eq!(decoded_epoch, epoch);
    assert_error(
        decode_key_file(&encoded[..KEY_FILE_BYTES - 1]),
        CatalogAuthorityError::RecoveryRequired,
    );
}

#[test]
fn state_record_round_trips_initializing_and_adopted_modes_strictly() {
    let lock = FileIdentity {
        device: 3,
        inode: 5,
    };
    let initial = initial_record_for_test(MODE_FIRST_USE, [7; 16], [9; 32], lock, [0; 32], [0; 32]);
    let encoded = initial.encode();
    assert_eq!(encoded.len(), STATE_RECORD_BYTES);
    assert_eq!(
        StateRecord::decode(&encoded).expect("valid init record"),
        initial
    );
    assert!(initial.is_initializing());

    let legacy = initial_record_for_test(
        MODE_LEGACY_ADOPTION,
        [8; 16],
        [10; 32],
        lock,
        [11; 32],
        [12; 32],
    );
    assert_eq!(
        StateRecord::decode(&legacy.encode()).expect("valid legacy intent"),
        legacy
    );
    assert!(!format!("{legacy:?}").contains("11"));

    let adopted = StateRecord::adopted(
        legacy,
        FileIdentity {
            device: 3,
            inode: 21,
        },
        FileIdentity {
            device: 3,
            inode: 22,
        },
        FileIdentity {
            device: 3,
            inode: 23,
        },
        key_commitment(&[0x55; 32]),
    )
    .expect("initial record transitions to complete adopted record");
    assert_eq!(
        StateRecord::decode(&adopted.encode()).expect("valid adopted record"),
        adopted
    );
    assert!(!adopted.is_initializing());
}

#[test]
fn state_record_rejects_unknown_version_status_reserved_bytes_and_trailing_data() {
    let record = initial_record_for_test(
        MODE_FIRST_USE,
        [7; 16],
        [9; 32],
        FileIdentity {
            device: 3,
            inode: 5,
        },
        [0; 32],
        [0; 32],
    );
    for (index, value) in [(8, 0xff), (10, 0xff), (12, 1)] {
        let mut invalid = record.encode();
        invalid[index] = value;
        assert_eq!(
            StateRecord::decode(&invalid),
            Err(CatalogAuthorityError::RecoveryRequired)
        );
    }
    let mut trailing = record.encode().to_vec();
    trailing.push(0);
    assert_eq!(
        StateRecord::decode(&trailing),
        Err(CatalogAuthorityError::RecoveryRequired)
    );
}

#[test]
fn state_record_rejects_missing_legacy_evidence_and_first_use_evidence() {
    let lock = FileIdentity {
        device: 3,
        inode: 5,
    };
    for (mode, evidence_id, evidence_digest) in [
        (MODE_FIRST_USE, [1; 32], [0; 32]),
        (MODE_FIRST_USE, [0; 32], [1; 32]),
        (MODE_LEGACY_ADOPTION, [0; 32], [1; 32]),
        (MODE_LEGACY_ADOPTION, [1; 32], [0; 32]),
    ] {
        let invalid =
            initial_record_for_test(mode, [7; 16], [9; 32], lock, evidence_id, evidence_digest);
        assert_eq!(
            StateRecord::decode(&invalid.encode()),
            Err(CatalogAuthorityError::RecoveryRequired)
        );
    }
}
