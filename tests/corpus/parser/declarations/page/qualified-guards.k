// Guard names may be qualified with dots.
page Billing requires auth.Authenticated, Roles.Admin(level: 2) {}
