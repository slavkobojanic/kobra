// String literals are printed so the output is safe to inline in HTML: a closing script tag and a comment
// opener are broken up, and line separators are escaped
// (docs/compiler/codegen.md §3).
func markup() -> List<String> {
    return ["</script>", "<!-- note -->", "line\u{2028}separator"]
}
