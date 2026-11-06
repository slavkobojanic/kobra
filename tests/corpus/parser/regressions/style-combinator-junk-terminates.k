// Regression: a style rule made only of combinators and stray symbols once looped forever.
component T { style { ::: &&& >>> +++ ~~~ } }
