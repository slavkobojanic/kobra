// `:not`, `:is`, `:where` and `:has` take selector lists.
component T {
    style {
        .a:not(.b, .c) {}
        .d:is(.e:hover, .f) {}
        .g:where(.h) {}
        .i:has(.icon) {}
    }
}
