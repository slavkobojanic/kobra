// CSS keywords are written in camel case (`lineThrough`, `spaceBetween`) and
// checked against each property's grammar in that spelling.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            display: inlineFlex;
            textDecoration: lineThrough;
            justifyContent: spaceBetween;
            alignItems: flexStart;
            whiteSpace: preWrap;
            fontFamily: "Inter", sansSerif;
            transition: backgroundColor 200ms easeInOut;
            gridTemplateColumns: repeat(autoFill, minmax(14rem, 1fr));
            width: fitContent;
            color: currentColor;
            @media (prefersReducedMotion: noPreference) { padding: 4px; }
        }
    }
}
