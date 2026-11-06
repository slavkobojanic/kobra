// A keyframes rule that meets the end of the file reports K0101 at its `{` and
// at every enclosing opener.
component T {
    style {
        @keyframes fade {
            from { opacity: 0; }
