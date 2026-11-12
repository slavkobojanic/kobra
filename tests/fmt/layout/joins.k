func f(ready: Bool) -> Int {
    if ready
    {
        return 1
    }
    else {
        return 2
    }
}

func g() throws E {
    do {
        try h()
    }
    catch .failed {
        return
    }
}

struct S
{
    let a: Int
}
