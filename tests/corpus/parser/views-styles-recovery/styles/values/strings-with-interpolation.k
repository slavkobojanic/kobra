// Strings in style values may interpolate K expressions and contain Unicode.
component T {
    style {
        .badge::after { content: "{count} items →"; }
        .jp { fontFamily: "Noto Sans 日本"; }
    }
}
