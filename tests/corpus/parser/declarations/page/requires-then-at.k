// Clause order is `requires` then `at`.
page Admin requires Authenticated, Role(.admin) at "/admin" {}
