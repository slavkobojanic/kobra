// A role that a semantic element expresses is K3110 with the element as help:
// `section { }.role(.navigation)` → use `nav` (05-views.md).
component Menu {
    view {
        box { "menu" }
            .role(.navigation)
    }
}
