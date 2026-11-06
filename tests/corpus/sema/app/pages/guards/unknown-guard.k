// An unresolved guard reports only K1201, not also K5006.
page Admin requires Authentcated {
    view {
        text { "admin" }
    }
}
