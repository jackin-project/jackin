use super::*;

#[test]
fn error_popup_returns_ok_when_no_sink_installed() {
    // No global sink installed in this test process → must be a
    // silent no-op success so non-UI call sites do not have to
    // branch on the install race.
    assert!(error_popup("title", "message").is_ok());
}

#[test]
fn exit_dialog_with_inspect_returns_max_when_no_sink_installed() {
    let options = vec![String::from("Retry"), String::from("Abort")];
    let ctx = [PromptContextLine::Plain(String::from("ctx"))];
    let result = exit_dialog_with_inspect("title", &ctx, options, &[]);
    assert_eq!(result.ok(), Some(usize::MAX));
}

#[test]
fn set_global_sink_accepts_object_safe_trait_object() {
    fn _accept(_: &'static dyn StandaloneDialogSink) {}
    // Compile-time check; nothing to assert at runtime.
}
