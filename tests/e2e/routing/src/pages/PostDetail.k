/// A named URL with a `String` parameter.
public page PostDetail at "/posts/{slug}" {
    @Param let slug: String

    view {
        paragraph { "post {slug}" }
    }
}
