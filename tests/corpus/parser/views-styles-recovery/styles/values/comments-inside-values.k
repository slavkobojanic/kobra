// Block comments between value components and line comments after declarations are trivia.
component T {
    style {
        .a {
            padding: .md /* inner */ .lg; // trailing
            margin: /* before */ 0;
        }
    }
}
