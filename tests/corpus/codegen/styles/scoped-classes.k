// path: src/corpus/Panel.k
// Scoped class names, theme tokens as custom properties, nesting flattened, `@media` kept, property
// names in kebab case (docs/compiler/codegen.md §12). Under the Test profile class names are readable.
theme Tokens {
    color surface: Color = #FFFFFF
    color surfaceHover: Color = #F3F4F6
    space sm: Spacing = 8px
    space md: Spacing = 16px
}

public component Panel {
    view {
        box { "content" }
            .class(.panel)
    }

    style {
        .panel {
            padding: .md;
            backgroundColor: .surface;

            &:hover { backgroundColor: .surfaceHover; }

            @media (maxWidth: 600px) { padding: .sm; }
        }
    }
}
