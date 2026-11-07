// `variant dark matches .prefersDark` also applies through `prefers-color-scheme` until a choice is
// made: `:root:not(.k-theme-set)` (07-themes-and-units.md, codegen.md §12).
theme Tokens {
    color surface: Color = #FFFFFF

    variant dark matches .prefersDark {
        surface = #111827
    }

    variant highContrast {
        surface = #000000
    }
}
