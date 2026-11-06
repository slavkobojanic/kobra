// Media feature names are camel case and come from the feature table; an unknown
// one is K3211 with the closest feature as a fix.
component Card {
    view {
        box { "x" }
            .class(.card)
    }

    style {
        .card {
            @media (maxWidth: 600px) { padding: 8px; }
            @media (maxWidht: 600px) { padding: 4px; }
        }
    }
}
