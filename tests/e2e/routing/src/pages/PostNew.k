/// A static named URL that also matches the shape of `PostDetail`'s
/// template: the static segment wins.
public page PostNew at "/posts/new" {
    view {
        paragraph { "new post form" }
    }
}
