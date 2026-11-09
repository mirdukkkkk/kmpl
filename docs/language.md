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

Constants are declared outside functions. They can use earlier constants and
built-in functions. `PI`, `TAU` and `E` are always available.

```kmpl
const RADIUS: real = 2.0;
const AREA: real = PI * RADIUS * RADIUS;

fn main() {
    print(AREA > 12.5);  // true
}
```

A constant can call functions. It is an error if a call reaches a constant declared
later, or something that depends on how the program is run, such as `args`.

`bytes` holds raw data. `b[i]` and `for x in b` give `int`s, and slices, `len`,
`push`, `concat` and `==` work as on arrays. `string(b)` decodes UTF-8 and stops the
program if the data is not valid UTF-8, while `print(b)` shows the numbers:

```kmpl
let b = bytes("AB");
b[0] = 97;
print(string(b));  // aB
print(b);  // [97, 66]
```

## Operators

From weakest to strongest: `||`, `&&`, `== !=`, `< <= > >=`, `|`, `^`, `&`, `<< >>`, `+ -`, `* / %`, unary
`! -`, then `[index]` and `.field`, then `:swizzle`.

`int` overflow and `int` division by zero stop the program with an error. `real`
follows IEEE 754, so `1.0 / 0` is `inf`.

A swizzle picks vector components:

```kmpl
let v = vec3(1, 2, 3);
print(v:zy);  // (3.0, 2.0)
print(v * 2.0 + v);  // (3.0, 6.0, 9.0)
```

`%` is the remainder, with the sign of the left side. `& | ^ ~ << >>` work on
`int`; `>>` keeps the sign and `shr_logical(x, n)` from `std:bits` fills with zeros. Shifting by
less than 0 or more than 63 stops the program with an error.

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
print(len(a));  // 3
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
    return (a / b, a - a / b * b);
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

`+` joins strings, and `==` and `!=` compare them. `len` counts characters. The
escapes are `\n`, `\t`, `\"` and `\\`.

```kmpl
let name = "мир";
print("hello, " + name);  // hello, мир
print(len(name));  // 3
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
array: `children: [Node]`.

## Modules

`import` goes at the top of a file. `from` says where the module is, relative to
that file: `"./lib/stats"` is `lib/stats.kmpl` or `lib/stats/mod.kmpl`, and there
must be exactly one of them. With `.kmpl` at the end it is exactly that file.

```kmpl
// lib/stats.kmpl
fn mean(a: [real]) -> real {
    let sum = 0.0;
    for x in a {
        sum += x;
    }
    return sum / real(len(a));
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

Everything in a module is public. A file must import a module itself to use it,
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
added. `kmpl.lock`, next to `kmpl.toml`, records the commit behind every GitHub link
and the sha256 of every downloaded file. Commit it to your repository: a moved tag
or a new push to a branch changes nothing until you delete its line, and a file
that differs from its hash stops the build. `--locked` refuses to add anything new.

## Libraries

Libraries that come with `kmpl` are imported from `std:`:

| Library | Functions |
|---|---|
| `std:fs` | `read read_text write write_text` |
| `std:io` | `args read_line` |
| `std:text` | `parse_int parse_real` |
| `std:bits` | `shr_logical` |

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
`read_line` returns a line and `false` at the end of input. `parse_int` and
`parse_real` return a value and whether the text was a number.

```kmpl
import (parse_int, parse_real) from "std:text";

fn main() {
    let (n, ok) = parse_int("42");
    print(n + 1);  // 43
    print(ok);  // true
    print(parse_real("abc"));  // (0.0, false)
}
```

```text
import (args, read_line) from "std:io";

fn main() {
    for arg in args() { print(arg); }
    let (line, more) = read_line();
}
```

## Built-in functions

| | |
|---|---|
| Math on `real` or `complex` | `sqrt exp log sin cos tan asin acos atan sinh cosh tanh` |
| Other numbers | `abs pow cbrt floor ceil round atan2 hypot min max clamp is_nan is_inf lerp smoothstep` |
| Complex | `re im arg conj` |
| Conversion | `real int complex string` |
| Vectors | `vec2 vec3 vec4 dot length distance normalize cross` |
| Arrays and strings | `len push concat` |
| Output | `print` |

These functions work on single values. `sqrt` of an array is an error, so use a
loop.
