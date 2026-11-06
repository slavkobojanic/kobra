// There is no implicit `it`: a lambda without parameters can't stand for `(T) -> U`, and `it` is an unknown name.
func doubled(numbers: List<Int>) -> List<Int> {
    return numbers.map { it * 2 }
}
