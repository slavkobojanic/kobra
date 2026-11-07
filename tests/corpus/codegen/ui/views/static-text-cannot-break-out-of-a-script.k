// Text that looks like markup can't close a script element or open a comment in either bundle
// (codegen.md §3: `</script` and `<!--` are broken up; the HTML escaping already covers them).
component Warning {
    view {
        paragraph { "</script><script>alert(1)</script><!-- x -->" }
    }
}
