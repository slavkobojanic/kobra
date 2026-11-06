// A component is closed to outside styling unless it opts in: `.class(…)` on a
// component call is not an attribute of components (K3103). The parent's class is
// still counted as referenced, so it gets no K3204.
component Badge {
    view {
        text { "new" }
    }
}

component Home {
    view {
        Badge()
            .class(.accent)
    }

    style {
        .accent { color: #FF0000; }
    }
}
