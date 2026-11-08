// Dropping a column no migration retired breaks the previous release during a
// rolling deploy: K6006, even with another `@Breaking` reason.
public app Demo {}
