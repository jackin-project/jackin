use super::DB_PATH;

#[test]
fn opencode_token_reader_db_path_is_correct() {
    assert!(DB_PATH.contains("opencode.db"));
}
