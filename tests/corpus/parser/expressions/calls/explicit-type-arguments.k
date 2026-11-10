// explicit type arguments on a name, a member, a type's static member and before a trailing lambda
let a = decode<User>(json)
let b = Jwt.verify<Claims>(token, key: key)
let c = Sealed<Note>.parse(text)
let d = names.map<Int> { name => name.count }
let e = parse<Map<String, List<Int?>>>(text)
