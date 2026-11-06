// The environment list may continue after a comma on the next line.
app A {
    use Fake for Api in .development,
        .test
    use Real for Api in .production
}
