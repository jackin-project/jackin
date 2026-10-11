use super::*;

#[test]
fn parse_git_identity_config_reads_name_and_email_from_one_capture() {
    let identity = parse_git_identity_config("user.name Neo\nuser.email neo@example.test\n");

    assert_eq!(identity.user_name, "Neo");
    assert_eq!(identity.user_email, "neo@example.test");
}

#[test]
fn parse_git_identity_config_tolerates_missing_keys() {
    let identity = parse_git_identity_config("user.email trinity@example.test\n");

    assert_eq!(identity.user_name, "");
    assert_eq!(identity.user_email, "trinity@example.test");
}
