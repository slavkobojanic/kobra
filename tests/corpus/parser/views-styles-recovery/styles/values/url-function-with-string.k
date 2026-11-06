// `url(...)` takes a string, because `//` starts a comment in style blocks.
component T {
    style {
        .hero { backgroundImage: url("https://cdn.example.com/hero.png"); }
    }
}
