// Several different style errors in one block are each reported exactly once.
component T {
    style {
        gap: 0;
        .a {
            color: red
            margin: 0 !important;
        }
        .b p { padding: 3pxx; }
    }
}
