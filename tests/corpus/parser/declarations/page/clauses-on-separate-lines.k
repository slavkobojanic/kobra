// `requires`, `at` and the body's `{` may each start a new line.
page Admin
    requires Authenticated,
        Role(.admin)
    at "/admin"
{
    view {
        "Admin"
    }
}
