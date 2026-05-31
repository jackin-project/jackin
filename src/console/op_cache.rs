pub type OpCache = jackin_console::op_cache::OpCache<
    crate::operator_env::OpAccount,
    crate::operator_env::OpVault,
    crate::operator_env::OpItem,
    crate::operator_env::OpField,
>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Trust-model guard: if `OpField` ever grows a value-bearing field,
    /// this exhaustive destructure breaks and forces a cache review.
    #[test]
    fn root_op_cache_does_not_store_field_values() {
        let mut cache = OpCache::default();
        cache.put_fields(
            None,
            "v1",
            "i1",
            vec![crate::operator_env::OpField {
                id: "f1".into(),
                label: "password".into(),
                field_type: "STRING".into(),
                concealed: true,
                reference: "op://v/i/f".into(),
            }],
        );
        for field in cache.get_fields(None, "v1", "i1").unwrap() {
            let crate::operator_env::OpField {
                id: _,
                label: _,
                field_type: _,
                concealed: _,
                reference: _,
            } = field;
        }
    }
}
