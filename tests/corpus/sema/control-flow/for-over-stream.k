func consume(stream: Stream<Int>) async {
    for value in stream {
    }
    for await value in stream {
    }
}
