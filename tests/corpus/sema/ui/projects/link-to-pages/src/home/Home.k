// `link(to:)` takes a `Page`: `.about` is the generated case of the `About` page.
// An unknown page is K2006; `to:` and `url:` together are K2004 at the second
// label, because the forms are alternatives.
public page Home {
    view {
        link(to: .about) { "About" }
        //        ^^^^^ resolves: member Page.about
        link(to: .abuot) { "About" }
        link(to: .about, url: Url("https://k.dev")) { "Both" }
    }
}
