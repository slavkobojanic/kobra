// An unknown prop type is K1202 once; reads of the prop and of its members, and
// the call that passes it, cause no follow-on errors.
component Row {
    @Prop let item: Itme

    view {
        text { item.name }
        text { "{item.price.formatted()}" }
    }
}

component List1 {
    view {
        Row(item: 42)
    }
}
