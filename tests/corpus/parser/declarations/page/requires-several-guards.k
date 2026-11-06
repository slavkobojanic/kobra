// `requires` takes a comma-separated list of guards, with or without arguments.
page Admin requires Authenticated, Role(.admin), RateLimit(100.perMinute, burst: 10) {}
