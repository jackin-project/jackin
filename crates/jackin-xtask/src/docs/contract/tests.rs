use super::*;

#[test]
fn prose_does_not_change_link_surface() {
    let first = "# Heading\nplain prose\n[docs](guide) beside words\n";
    let second = "# Heading\ndifferent prose\n[guide](guide) and other words\n";
    assert_eq!(extract_link_surface(first), extract_link_surface(second));
}

#[test]
fn component_copy_does_not_change_link_attributes() {
    let first = "Read <Link href=\"/guide\">the guide</Link>.\n";
    let second = "Open <Link href=\"/guide\">documentation</Link> now.\n";
    assert_eq!(extract_link_surface(first), extract_link_surface(second));
}

#[test]
fn multiline_link_targets_are_part_of_surface() {
    let surface = String::from_utf8(extract_link_surface("[docs](\n/guide\n)\nother\n")).unwrap();
    assert!(surface.contains("/guide"));
    assert!(surface.contains(')'));
    assert!(!surface.contains("other"));
}

#[test]
fn tool_section_matches_sed_inclusive_end_address() {
    let lock = b"[[tools.a]]\nx = 1\n[[tools.b]]\ny = 2\n";
    assert_eq!(
        selected_tool_sections(lock, &["a"]),
        b"[[tools.a]]\nx = 1\n[[tools.b]]\n"
    );
}
