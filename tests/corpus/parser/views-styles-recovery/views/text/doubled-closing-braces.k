// `}}` in a text node is two literal closing braces, and `{{` before an interpolation escapes only the first brace.
component T {
    view {
        text { "}} {{{name}" }
    }
}
