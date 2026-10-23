use super::{
    classify_first_entry_result, classify_getter_failure, classify_native_error,
    classify_validity_result,
};
use crate::catalog_revision::CatalogAuthorityError;
use rustix::io::Errno;

#[test]
fn getter_enoent_is_the_only_no_acl_error() {
    assert_eq!(classify_getter_failure(Errno::NOENT), Ok(()));
    assert_eq!(
        classify_getter_failure(Errno::INVAL),
        Err(CatalogAuthorityError::StorageFailure)
    );
    assert_eq!(
        classify_getter_failure(Errno::BADF),
        Err(CatalogAuthorityError::StorageFailure)
    );
    assert_eq!(
        classify_getter_failure(Errno::OPNOTSUPP),
        Err(CatalogAuthorityError::Unsupported)
    );
}

#[test]
fn validated_darwin_first_entry_zero_means_present_and_einval_means_empty() {
    assert_eq!(
        classify_first_entry_result(0, None, true),
        Err(CatalogAuthorityError::UnsafeStore)
    );
    assert_eq!(
        classify_first_entry_result(0, None, false),
        Err(CatalogAuthorityError::StorageFailure)
    );
    assert_eq!(
        classify_first_entry_result(-1, Some(Errno::INVAL), false),
        Ok(())
    );
    assert_eq!(
        classify_first_entry_result(-1, Some(Errno::BADF), false),
        Err(CatalogAuthorityError::StorageFailure)
    );
    assert_eq!(
        classify_first_entry_result(-1, Some(Errno::NOTSUP), false),
        Err(CatalogAuthorityError::Unsupported)
    );
    assert_eq!(
        classify_first_entry_result(1, Some(Errno::INVAL), false),
        Err(CatalogAuthorityError::StorageFailure)
    );
}

#[test]
fn validity_einval_is_failure_not_an_empty_acl() {
    assert_eq!(
        classify_validity_result(-1, Some(Errno::INVAL)),
        Err(CatalogAuthorityError::StorageFailure)
    );
    assert_eq!(
        classify_validity_result(-1, Some(Errno::OPNOTSUPP)),
        Err(CatalogAuthorityError::Unsupported)
    );
    assert_eq!(classify_validity_result(0, None), Ok(()));
    assert_eq!(
        classify_native_error(Errno::NOENT),
        CatalogAuthorityError::StorageFailure
    );
}
