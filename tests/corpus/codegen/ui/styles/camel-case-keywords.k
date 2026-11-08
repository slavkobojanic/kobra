// path: src/corpus/Row.k
// CSS keywords are camel case in K and kebab case in the CSS: `lineThrough` is
// `line-through`, also inside functions and media features; `currentColor`
// keeps its spelling (docs/compiler/codegen.md §12).
public component Row {
    view {
        box { "content" }
            .class(.row)
    }

    style {
        .row {
            display: inlineFlex;
            justifyContent: spaceBetween;
            textDecoration: lineThrough;
            transition: backgroundColor 200ms easeInOut;
            gridTemplateColumns: repeat(autoFill, minmax(14rem, 1fr));
            fontFamily: "Inter", sansSerif;
            color: currentColor;

            @media (prefersReducedMotion: noPreference) { whiteSpace: preWrap; }
        }
    }
}
