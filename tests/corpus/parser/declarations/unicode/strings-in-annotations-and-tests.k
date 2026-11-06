// Strings in annotations, clauses and test names may hold any Unicode text.
@Breaking("移除了字段 🚀 — ünïcödé")
protocol P {}

test "naïve café ☕ 測試" {}
