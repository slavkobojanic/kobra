// A keyframe selector without `{` reports K0100 at the token after it and
// skips to the end of the statement; the next block parses normally.
component T {
    style {
        @keyframes fade {
            from opacity: 0;
            to { opacity: 1; }
        }
    }
}
