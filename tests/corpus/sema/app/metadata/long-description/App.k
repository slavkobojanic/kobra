// A literal description over 160 characters is K3028 at the literal, in
// the app's metadata, a layout's, a page's and inside `failed`
// (22-metadata.md, "Rules"). The app's title covers every page, so none of
// them is K3027.
public app Site {
    metadata {
        title("Site")
        description("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
    }
}
