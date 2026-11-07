// A variant is a class on `<html>` that overrides only the tokens it assigns
// (07-themes-and-units.md, codegen.md §12, "Variants").
theme Tokens {
    color surface: Color = #FFFFFF
    color text: Color = #111827

    variant dark {
        surface = #111827
    }
}
