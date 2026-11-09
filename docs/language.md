# The kmpl language

A short tour. The exact syntax is in [`grammar.ebnf`](../grammar.ebnf).

A program is one or more `.kmpl` files. It starts at `fn main()`. Comments start
with `//`.

```kmpl
fn main() {
    print("Hello!");  // Hello!
}
```

## Values and types

| Type | Example | |
|---|---|---|
| `int` | `42` | 64-bit integer |
| `real` | `1.5`, `.5`, `1e6` | 64-bit float |
| `bool` | `true` | |
| `complex` | `2.0i`, `1.0 + 2.0i` | |
| `vec2`, `vec3`, `vec4` | `vec3(1, 2, 3)` | |
| `string` | `"text\n"` | UTF-8, cannot be changed in place |
| `bytes` | `bytes("text")` | Raw bytes; elements are `int` from 0 to 255 |
| `[T]` | `[1, 2, 3]` | array of any length |
| `(A, B)` | `(1, 2.5)` | tuple of two or more values |

Struct names are types too. Types are inferred, and you can always write them:

```kmpl
let a = 1;
let b: real = 1;
let c;
c = "assigned later";
print(a / 2);  // 0
print(b / 2);  // 0.5
print(c);  // assigned later
```

A number literal takes the type its context needs: `1` can be an `int`, `real`
or `complex`, and `1.0` a `real` or `complex`. A value whose type is already
known never converts by itself, so convert it explicitly:

```kmpl
let n = 3;
let x = real(n) / 2.0;
let z = complex(x, 0.0) + 1.0i;
print(x);  // 1.5
print(z);  // 1.5+1.0i
print(int(2.9));  // 2
print(string(n) + "!");  // 3!
```

Constants are declared outside functions and can use earlier constants. `PI`,
`TAU` and `E` are always available.

```kmpl
const RADIUS: real = 2.0;
const AREA: real = PI * RADIUS * RADIUS;

fn main() {
    print(AREA > 12.5);  // true
}
```

A constant can call functions. It is an error if a call reaches a constant declared
later, or something that depends on how the program is run, such as `args`.

`bytes` holds raw data. `b[i]` and `for x in b` give `int`s, and slices, `length`,
`push`, `concat` and `==` work as on arrays. `string(b)` decodes UTF-8 and stops the
program if the data is not valid UTF-8, while `print(b)` shows the numbers:

```kmpl
let b = bytes("AB");
b[0] = 97;
print(string(b));  // aB
print(b);  // [97, 66]
```

## Operators

From weakest to strongest: `||`, `&&`, `== !=`, `< <= > >=`, `|`, `^`, `&`, `<< >>`,
`+ -`, `* / %`, unary `! - ~`, then `[index]` and `.field`, then `:swizzle`.

`int` overflow and `int` division or remainder by zero stop the program with an
error. `real` follows IEEE 754, so `1.0 / 0` is `inf`.

A swizzle picks vector components:

```kmpl
let v = vec3(1, 2, 3);
print(v:zy);  // (3.0, 2.0)
print(v * 2.0 + v);  // (3.0, 6.0, 9.0)
```

`%` is the remainder, with the sign of the left side. `& | ^ ~ << >>` work on
`int`; `>>` keeps the sign and `shr_logical(x, n)` from `std:bits` fills with
zeros. Shifting by less than 0 or more than 63 stops the program with an error.

Assignments can be combined with `+ - * / % & | ^ << >>`:

```kmpl
let x = 2;
x += 3;
x *= 2;
print(x);  // 10
```

## Control flow

```kmpl
let n = 7;
if n > 5 {
    print("big");  // big
} else if n > 2 {
    print("medium");
} else {
    print("small");
}

let i = 0;
while i < 3 {
    i += 1;
    if i == 2 {
        continue;
    }
}
print(i);  // 3
```

`break` leaves a loop and `continue` goes to its next round. A `for` loop goes
over a range or an array. `a..b` does not include `b`, `a..=b` does, and a
negative `step` counts down:

```kmpl
let total = 0;
for i in 1..=9 step 2 {
    total += i;
}
print(total);  // 25

let digits = 0;
for i in 3..=1 step -1 {
    digits = digits * 10 + i;
}
print(digits);  // 321

let sum = 0.0;
for x in [1.5, 2.5] {
    sum += x;
}
print(sum);  // 4.0
```

## Functions

A function can be called before its definition. Parameters always have types.

```kmpl
fn square(x: real) -> real {
    return x * x;
}

fn main() {
    print(square(1.5));  // 2.25
}
```

Generic functions list the types they accept. `number` is `int | real | complex`,
`float` is `real | complex`, `vector` is `vec2 | vec3 | vec4`. Without a bound,
`T` is any type except an array, tuple or struct, but `[T]` and `(T, T)` work.

```kmpl
fn twice<T: real | vector>(x: T) -> T {
    return x + x;
}

fn main() {
    print(twice(1.5));  // 3.0
    print(twice(vec2(1, 2)));  // (2.0, 4.0)
}
```

## Arrays and matrices

Arrays are values: `let b = a;` makes an independent copy. Indices are `int`,
from zero, and a bad index stops the program with an error.

```kmpl
let a = [1.0, 2.0, 3.0];
let b = a;
b[0] = 9.0;
print(a[0]);  // 1.0
print(length(a));  // 3
print(a[1..]);  // [2.0, 3.0]
print(a[..=1]);  // [1.0, 2.0]
print([0; 2]);  // [0, 0]
print(push(a, 4.0));  // [1.0, 2.0, 3.0, 4.0]
print(concat([1], [2, 3]));  // [1, 2, 3]
```

`push` and `concat` return a new array and leave the old one alone, so write
`a = push(a, x);`.

`+`, `-` and unary `-` work element by element. `*` and `/` by a number apply to
every element. A matrix is just a `[[real]]`, and `*` on matrices is the matrix
product:

```kmpl
let a = [1.0, 2.0];
print(a + a);  // [2.0, 4.0]
print(a * 2.0);  // [2.0, 4.0]

let m = [[1.0, 2.0], [3.0, 4.0]];
print(m * m);  // [[7.0, 10.0], [15.0, 22.0]]
print(m * a);  // [5.0, 11.0]
```

Sizes are only known when the program runs, so mismatched sizes are runtime
errors. `[real] * [real]` is not allowed.

## Tuples

A tuple holds a few values of different types, and is read by unpacking it.
`_` skips an element.

```kmpl
fn div_rem(a: int, b: int) -> (int, int) {
    return (a / b, a % b);
}

fn main() {
    let (q, r) = div_rem(17, 5);
    print(q);  // 3
    print(r);  // 2
    let (_, only_r) = div_rem(9, 4);
    print(only_r);  // 1
}
```

## Strings

`+` joins strings, and `==` and `!=` compare them. `length` counts characters. The
escapes are `\n`, `\t`, `\r`, `\"` and `\\`.

```kmpl
let name = "мир";
print("hello, " + name);  // hello, мир
print(length(name));  // 3
print(name[0]);  // м
print(name[1..]);  // ир
print("a" < "b");  // true
```

A character is a string of one. Positions count characters, and a string cannot be
changed through them. `<`, `<=`, `>` and `>=` compare by character codes, so they
know nothing about alphabets. `for c in s` goes through a text once. `s[i]` starts
from the beginning every time, so to jump around in a long text use `chars(s)`,
which gives an array of its characters.

## Calling with a dot

`x.f(y)` is the same as `f(x, y)`, for any function that can be called here by
its plain name. It is handy in chains. `p.x` reads a field, `p.x()` calls `x`.

```kmpl
fn scaled(v: vec2, k: real) -> vec2 {
    return v * k;
}

fn main() {
    let v = vec2(3.0, 4.0);
    print(v.scaled(2.0).length());  // 10.0
    print(sqrt(v.length()));  // 2.23606797749979
    print([1, 2, 3].length());  // 3
}
```

`length` is the length of a vector as a `real`, and the number of elements of an
array or `bytes` or of characters of a string as an `int`.

`format(x, digits)` gives the text of an `int`, `real`, `complex` or vector with
this many digits after the point, from 0 to 100. A number is rounded by its exact
value in memory, so `format(2.675, 2)` is `2.67`.

```kmpl
print(format(3.14159, 2));  // 3.14
print(format(vec2(1.0, 2.5), 1));  // (1.0, 2.5)
```

## Structs

Structs are values too. A literal has to give every field, in any order.

```kmpl
struct Point {
    x: real,
    y: real,
}

fn moved(p: Point, dx: real) -> Point {
    p.x += dx;
    return p;
}

fn main() {
    let a = Point { x: 1.0, y: 2.0 };
    let b = moved(a, 1.0);
    print(a.x);  // 1.0
    print(b);  // Point { x: 2.0, y: 2.0 }
    if a == (Point { x: 1.0, y: 2.0 }) {
        print("equal");  // equal
    }
}
```

In the header of `if`, `while` and `for` a literal needs brackets, as above,
because `{` would start the body. A struct can hold itself only through an
array or an optional: `children: [Node]`, `next: Node?`.

## Enums and optionals

An enum holds one of its variants, and a variant can carry values. `match` runs
the arm of the variant it finds and has to handle all of them; `_` takes the
rest, and a name `_` skips a value.

`T?` is a value of type `T` or nothing: `some(value)` or `none`.

`panic("message")` stops the program with an error, and the checker knows that
nothing runs after it.

```kmpl
enum Shape {
    Circle(real),
    Rect(real, real),
    Empty,
}

fn width(s: Shape) -> real {
    match s {
        Shape::Circle(r) => { return 2.0 * r; }
        Shape::Rect(w, _) => { return w; }
        Shape::Empty => { return 0.0; }
    }
}

fn first(a: [int]) -> int? {
    if length(a) == 0 {
        return none;
    }
    return some(a[0]);
}

fn main() {
    print(width(Shape::Rect(2.0, 3.0)));  // 2.0
    print(Shape::Circle(1.0));  // Shape::Circle(1.0)
    match first([7, 8]) {
        some(x) => { print(x); }  // 7
        none => { panic("empty"); }
    }
    print(first([]));  // none
}
```

When only one case matters, `if let` and `while let` are shorter. `unwrap_or` gives
the value or a replacement, which is computed either way, and `unwrap` gives the
value or stops the program:

```kmpl
fn half(x: int) -> int? {
    if x % 2 == 1 {
        return none;
    }
    return some(x / 2);
}

fn main() {
    if let some(v) = half(8) {
        print(v);  // 4
    }
    let i = 16;
    while let some(v) = half(i) {
        i = v;
    }
    print(i);  // 1
    print(half(3).unwrap_or(0));  // 0
    print(half(4).unwrap());  // 2
}
```

`match` is a statement, and it works on enums and optionals only. In another
module an enum is written `geometry.Shape::Circle(1.0)`.

## Functions as values

A function can be kept in a variable, a field or an array, passed along and
returned. Its type is written `fn(real) -> real`. A function without a name has
the types of its parameters and result written out. It reads the variables around
it as they were when it was created, and cannot change them.

```kmpl
fn square(x: real) -> real {
    return x * x;
}

fn apply(f: fn(real) -> real, x: real) -> real {
    return f(x);
}

fn adder(n: int) -> fn(int) -> int {
    return fn(x: int) -> int { return x + n; };
}

fn main() {
    print(apply(square, 3.0));  // 9.0
    let add2 = adder(2);
    print(add2(1));  // 3
    print(square);  // <fn>
}
```

`std:array` has `map`, `filter`, `fold`, `any` and `all`. Functions cannot be
compared. A generic function or a built-in has to be wrapped in a function without
a name to become a value, and a function without a name cannot call itself. A
constant cannot call a function held in a variable. A call through a variable is
slower than a direct one.

## Generic structs and enums

A struct or an enum can take types as arguments, written `<T>`. They are usually
found from the values. Where a value says too little, as `Res::Good(1)` says nothing
about the error, the type comes from a `return`, an annotation or an argument.

```kmpl
struct Pair<T> {
    first: T,
    second: T,
}

enum Res<T, F> {
    Good(T),
    Bad(F),
}

fn half(n: int) -> Res<int, string> {
    if n % 2 == 1 {
        return Res::Bad("odd");
    }
    return Res::Good(n / 2);
}

fn swap<T>(p: Pair<T>) -> Pair<T> {
    return Pair { first: p.second, second: p.first };
}

fn main() {
    print(half(3));  // Res::Bad(odd)
    print(swap(Pair { first: 1, second: 2 }));  // Pair { first: 2, second: 1 }
    let q: Pair<[real]> = Pair { first: [], second: [1.0] };
    print(q.second);  // [1.0]
}
```

A `T` with no bound can be any type, an array or a struct too. A struct or an enum
cannot limit its arguments the way a function can with `T: int | real`.

## Modules

`import` goes at the top of a file. `from` says where the module is, relative to
that file: `"./lib/stats"` is `lib/stats.kmpl` or `lib/stats/mod.kmpl`, and there
must be exactly one of them. With `.kmpl` at the end it is exactly that file.

```kmpl
// lib/stats.kmpl
pub fn mean(a: [real]) -> real {
    let sum = 0.0;
    for x in a {
        sum += x;
    }
    return sum / real(length(a));
}
```

```kmpl
// main.kmpl
import stats from "./lib/stats";

fn main() {
    print(stats.mean([1.0, 2.0, 6.0]));  // 3.0
}
```

To use items without the module's name, list them in brackets. `as` gives one
another name, and a name that is already taken is an error:

```kmpl
import (mean, mean as average) from "./lib/stats";
```

Only items marked `pub` can be used from other files; everything else is private
to its file. A public struct has public fields and a public enum public variants.
A file must import a module itself to use it,
even if a module it imports already does. Only the `main` of the file you run is
used.

## Dependencies

A plain name after `from` is looked up in `kmpl.toml`, which is searched for from
the file you run upward. The version says which format the file uses:

```toml
version = 1

[dependencies]
helpers = "../helpers"
```

```text
import helpers from "helpers";
```

A dependency, or an import itself, can also be a link. On GitHub it names a tag,
a branch or a commit, and a path inside the repository can follow it:

```toml
[dependencies]
pb = "https://github.com/user/pb/v1.2.0"
```

Any other `https://` link ending in `.kmpl` is that file; otherwise `/mod.kmpl` is
added. `kmpl.lock`, next to `kmpl.toml` or else next to the program, records the commit behind every GitHub link
and the sha256 of every downloaded file. Commit it to your repository: a moved tag
or a new push to a branch changes nothing until you run `kmpl update`, and a file
that differs from its hash stops the build. `--locked` refuses to add anything new.

`kmpl add` checks that a module and everything it imports exist, then writes both
files for you, and creates `kmpl.toml` if there is none. A GitHub link takes the
name of its repository unless you give one; anything else needs a name:

```text
kmpl add https://github.com/user/pb/v1.2.0
kmpl add https://github.com/user/pb/v1.2.0 protobuf
kmpl add ../helpers helpers
```

`kmpl update pb` finds again what the link of `pb` points to; `kmpl update` does
this for every link in `kmpl.lock`. A commit in a link never moves.
`kmpl update pb v2.0.0` also moves the GitHub link in `kmpl.toml` to that tag,
branch or commit. `--dry-run` shows what would change and writes nothing.

`kmpl vendor` copies every file of `kmpl.lock` into `vendor/` next to it, named by
its hash. Commit that directory, and `--locked` builds need no network, as in a
Nix sandbox. A link written directly in an `import` is in `kmpl.lock` only after
the program has been run once. Run `kmpl vendor` again after `kmpl update`.

## Libraries

Libraries that come with `kmpl` are imported from `std:`:

| Library | Functions |
|---|---|
| `std:fs` | `read read_text write write_text` |
| `std:io` | `args read_line` |
| `std:text` | `parse_int parse_real split join trim starts_with ends_with contains find replace repeat to_upper to_lower is_digit is_letter is_space code from_code` |
| `std:bits` | `shr_logical` |
| `std:array` | `map filter fold any all` |
| `std:linalg` | `solve det inverse transpose identity trace norm` |
| `std:stats` | `mean median variance stddev quantile correlation histogram linear_fit` |
| `std:random` | `seed real int normal shuffle` |
| `std:time` | `clock now sleep` |

`std:stats` works on `[real]`. Data it cannot use, such as an empty list, stops the
program. `variance` and `stddev` are those of a sample:

```kmpl
import stats from "std:stats";

fn main() {
    let a = [1.0, 2.0, 3.0];
    print(stats.mean(a));  // 2.0
    print(stats.stddev(a));  // 1.0
    print(stats.quantile(a, 0.5));  // 2.0
    print(stats.linear_fit(a, [3.0, 5.0, 7.0]));  // (2.0, 1.0)
}
```

`std:linalg` works on matrices written as `[[real]]`:

```kmpl
import linalg from "std:linalg";

fn main() {
    let a = [[2.0, 0.0], [0.0, 4.0]];
    print(linalg.solve(a, [2.0, 8.0]));  // some([1.0, 2.0])
    print(linalg.det(a));  // 8.0
    print(linalg.inverse([[1.0, 2.0], [2.0, 4.0]]));  // none
}
```

`solve` and `inverse` return `none` for a singular matrix, and a matrix of the wrong
shape stops the program. Its numbers are copied, so a function of this library is
as fast as Rust, while a loop over `a[i][j]` in your own code is slower.

`random.real()` is from 0.0 up to 1.0, and `random.int(1, 7)` from 1 to 6, like
`1..7`. An empty range stops the program. After `random.seed(n)` the numbers are
the same on every run; `normal` may differ in the last digits between systems.
The generator is not for secrets.

`time.clock()` counts seconds from the start of the program and never goes back,
so it is the one to measure with. `time.now()` is the system clock, in seconds
since 1970.

Functions in `std:fs` also return the text of an error, empty if all went well:

```text
import fs from "std:fs";

fn main() {
    let error = fs.write_text("note.txt", "hello");
    let (text, read_error) = fs.read_text("note.txt");
    print(text);  // hello
}
```

`kmpl program.kmpl a b` gives the program its arguments through `args()`.
A program compiled with `kmpl build program.kmpl` gets its own: `./program a b`.
`read_line` returns a line, or `none` at the end of input. `parse_int` and
`parse_real` return a number, or `none` if the text is not one.

```kmpl
import (parse_int, parse_real) from "std:text";

fn main() {
    print(parse_int("42"));  // some(42)
    print(parse_real("abc"));  // none
}
```

```text
import (args, read_line) from "std:io";

fn main() {
    for arg in args() { print(arg); }
    match read_line() {
        some(line) => { print(line); }
        none => { print("no input"); }
    }
}
```

## Built-in functions

| | |
|---|---|
| Math on `real` or `complex` | `sqrt exp log sin cos tan asin acos atan sinh cosh tanh` |
| Other numbers | `abs pow cbrt floor ceil round atan2 hypot min max clamp is_nan is_inf lerp smoothstep` |
| Complex | `re im arg conj` |
| Conversion | `real int complex string bytes format` |
| Vectors | `vec2 vec3 vec4 dot length distance normalize cross` |
| Arrays and strings | `length push concat chars` |
| Optionals | `unwrap unwrap_or` |
| Stopping | `panic exit` |
| Output | `print eprint` |

These functions work on single values. `sqrt` of an array is an error, so use a
loop.
