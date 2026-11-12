component C {
    view { "x" }

    style {
        .card {
            padding: .md .lg;
            fontFamily: "Inter", sansSerif;
            width: calc(100% - 2px);
            color: {accent};
        }
        .card:hover > .title, .card::before { color: .primary; }
        .a .b { margin: 0; }
        @media (maxWidth: 600px) {
            .card { padding: .sm; }
        }
        @keyframes fade {
            from { opacity: 0; }
            to { opacity: 1; }
        }
    }
}
