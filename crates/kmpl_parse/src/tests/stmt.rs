use kmpl_ast::{Item, Span};

use super::{body, body_error, parse_program};

#[test]
fn type_arguments_may_end_in_one_token() {
    assert_eq!(body("let x: A<int> = 1;"), "[(let x A<int> 1)]");
    assert_eq!(body("let x: A<B<int>> = 1;"), "[(let x A<B<int>> 1)]");
    assert_eq!(body("let x: A<int>= 1;"), "[(let x A<int> 1)]");
    assert_eq!(body("let x: A<B<int>>= 1;"), "[(let x A<B<int>> 1)]");
    assert_eq!(
        body("let x: [A<int, real?>]? = 1;"),
        "[(let x [A<int, real?>]? 1)]"
    );
    assert_eq!(body_error("let x: A<int; "), "expected `>`, found `;`");
}

#[test]
fn if_let_is_a_match() {
    assert_eq!(
        body("if let some(x) = y { a; } else { b; }"),
        r#"[(match y ("some[\"x\"]" [a]) ("_[]" [b]))]"#
    );
    assert_eq!(
        body("if let A::B(c) = y { }"),
        r#"[(match y ("A::B[\"c\"]" []) ("_[]" []))]"#
    );
    assert_eq!(
        body("if let none = y { } else if x { a; }"),
        r#"[(match y ("none[]" []) ("_[]" [(if x [a])]))]"#
    );
    assert_eq!(
        body_error("if let some(x) y { }"),
        "expected `=`, found identifier `y`"
    );
}

#[test]
fn while_let_is_a_loop_around_a_match() {
    assert_eq!(
        body("while let some(x) = y { a; }"),
        r#"[(while true [(match y ("some[\"x\"]" [a]) ("_[]" [break]))])]"#
    );
}

#[test]
fn unknown_type_names_parse_as_params() {
    // Could be a generic parameter; the checker reports unknown ones.
    assert_eq!(body("let x: T;"), "[(let x T _)]");
}

#[test]
fn let_forms() {
    assert_eq!(body("let x: real = 1.0;"), "[(let x real 1.0)]");
    assert_eq!(body("let x = 1;"), "[(let x _ 1)]");
    assert_eq!(body("let x: vec3;"), "[(let x vec3 _)]");
    assert_eq!(body("let x;"), "[(let x _ _)]");
}

#[test]
fn assign_vs_expr_stmt() {
    assert_eq!(body("x = x + 1;"), "[(= x (+ x 1))]");
    assert_eq!(body("x + 1;"), "[(+ x 1)]");
    assert_eq!(body("print(x);"), "[(call print [x])]");
    assert_eq!(body("x == 1;"), "[(== x 1)]");
}

#[test]
fn if_else() {
    assert_eq!(body("if a { b; }"), "[(if a [b])]");
    assert_eq!(body("if a { b; } else { c; }"), "[(if a [b] [c])]");
}

#[test]
fn else_if_is_nested_if() {
    assert_eq!(
        body("if a { x; } else if b { y; } else { z; }"),
        "[(if a [x] [(if b [y] [z])])]"
    );
}

#[test]
fn while_break_continue() {
    assert_eq!(
        body("while i < 10 { if i == 5 { break; } continue; }"),
        "[(while (< i 10) [(if (== i 5) [break]) continue])]"
    );
}

#[test]
fn return_forms() {
    assert_eq!(body("return;"), "[(return)]");
    assert_eq!(body("return a * 2;"), "[(return (* a 2))]");
}

#[test]
fn empty_and_nested_blocks() {
    assert_eq!(body(""), "[]");
    assert_eq!(
        body("while true { while false { } }"),
        "[(while true [(while false [])])]"
    );
}

#[test]
fn stmt_spans() {
    let program = parse_program("fn f() { let x = 1; if x { } else { } }").unwrap();
    let Item::Function(f) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(f.body[0].span, Span::new(9, 19));
    assert_eq!(f.body[1].span, Span::new(20, 37));
}

#[test]
fn errors() {
    assert_eq!(body_error("let x = 1"), "expected `;`, found `}`");
    assert_eq!(
        body_error("let 1 = x;"),
        "expected identifier, found number `1`"
    );
    assert_eq!(body_error("if a b;"), "expected `{`, found identifier `b`");
    assert_eq!(body_error("break"), "expected `;`, found `}`");
    assert_eq!(body_error("x = ;"), "expected expression, found `;`");
}

#[test]
fn unclosed_block() {
    let err = parse_program("fn f() { let x = 1;").unwrap_err();
    assert_eq!(err.message, "expected `}`, found end of file");
    assert_eq!(err.span, Span::new(19, 19));
}

#[test]
fn for_loop() {
    assert_eq!(body("for i in 0..n { x; }"), "[(for i 0 n [x])]");
    assert_eq!(body("for i in 0..n { }"), "[(for i 0 n [])]");
}

#[test]
fn for_bounds_are_full_expressions() {
    assert_eq!(
        body("for i in a + 1..f(b) * 2 { }"),
        "[(for i (+ a 1) (* (call f [b]) 2) [])]"
    );
}

#[test]
fn for_errors() {
    assert_eq!(
        body_error("for 1 in 0..3 { }"),
        "expected identifier, found number `1`"
    );
    assert_eq!(
        body_error("for i 0..3 { }"),
        "expected `in`, found number `0`"
    );
    assert_eq!(
        body_error("for i in 0..3 x;"),
        "expected `{`, found identifier `x`"
    );
}

#[test]
fn array_types() {
    assert_eq!(body("let a: [real];"), "[(let a [real] _)]");
    assert_eq!(body("let m: [[int]];"), "[(let m [[int]] _)]");
    assert_eq!(body("let a: [T];"), "[(let a [T] _)]");
}

#[test]
fn array_literals() {
    assert_eq!(body("[];"), "[(array )]");
    assert_eq!(body("[1, 2];"), "[(array 1 2)]");
    assert_eq!(body("[1, 2,];"), "[(array 1 2)]");
    assert_eq!(body("[0.0; n + 1];"), "[(fill 0.0 (+ n 1))]");
    assert_eq!(body("[[1], [2]];"), "[(array (array 1) (array 2))]");
}

#[test]
fn indexing() {
    assert_eq!(body("a[0];"), "[(index a 0)]");
    assert_eq!(body("m[i][j + 1];"), "[(index (index m i) (+ j 1))]");
    assert_eq!(body("f(x)[0];"), "[(index (call f [x]) 0)]");
    assert_eq!(body("[1, 2][0];"), "[(index (array 1 2) 0)]");
    // Indexing binds tighter than a swizzle and than unary minus.
    assert_eq!(body("a[0]:xy;"), "[(: (index a 0) xy)]");
    assert_eq!(body("-a[0];"), "[(- (index a 0))]");
}

#[test]
fn index_assignment() {
    assert_eq!(body("a[0] = 1;"), "[(index= a [0] 1)]");
    assert_eq!(body("m[i][j] = x + 1;"), "[(index= m [i j] (+ x 1))]");
    assert_eq!(body("a[0] == 1;"), "[(== (index a 0) 1)]");
}

#[test]
fn for_each_loop() {
    assert_eq!(body("for x in a { x; }"), "[(foreach x a [x])]");
    assert_eq!(body("for x in [1, 2] { }"), "[(foreach x (array 1 2) [])]");
}

#[test]
fn array_errors() {
    assert_eq!(body_error("[1, 2;"), "expected `]`, found `;`");
    assert_eq!(body_error("a[0;"), "expected `]`, found `;`");
    assert_eq!(body_error("a[] ;"), "expected expression, found `]`");
    assert_eq!(body_error("let a: [real;"), "expected `]`, found `;`");
    assert_eq!(body_error("f()[0] = 1;"), "expected `;`, found `=`");
    assert_eq!(body_error("[1][0] = 1;"), "expected `;`, found `=`");
}

#[test]
fn compound_assignment() {
    assert_eq!(body("x += 1;"), "[(+= x 1)]");
    assert_eq!(body("x -= 1;"), "[(-= x 1)]");
    assert_eq!(body("x *= 2 + 3;"), "[(*= x (+ 2 3))]");
    assert_eq!(body("x /= 2;"), "[(/= x 2)]");
    assert_eq!(body("a[i][j] += 1;"), "[(index+= a [i j] 1)]");
    assert_eq!(body_error("f()[0] += 1;"), "expected `;`, found `+=`");
    assert_eq!(body_error("1 += 1;"), "expected `;`, found `+=`");
}

#[test]
fn inclusive_ranges_and_steps() {
    assert_eq!(body("for i in 0..=3 { }"), "[(for= i 0 3 [])]");
    assert_eq!(body("for i in 0..9 step 2 { }"), "[(for i 0 9 step 2 [])]");
    assert_eq!(
        body("for i in 9..=0 step -1 { }"),
        "[(for= i 9 0 step (- 1) [])]"
    );
    // `step` is only special after a range.
    assert_eq!(
        body("let step = 1; for i in 0..step step step { }"),
        "[(let step _ 1) (for i 0 step step step [])]"
    );
    assert_eq!(
        body_error("for i in 0..1 step { }"),
        "expected expression, found `{`"
    );
    assert_eq!(
        body_error("for x in a step 2 { }"),
        "expected `{`, found identifier `step`"
    );
}

#[test]
fn slices() {
    assert_eq!(body("a[1..2];"), "[(slice a 1 2)]");
    assert_eq!(body("a[..2];"), "[(slice a _ 2)]");
    assert_eq!(body("a[1..];"), "[(slice a 1 _)]");
    assert_eq!(body("a[..];"), "[(slice a _ _)]");
    assert_eq!(body("a[1..=2];"), "[(slice= a 1 2)]");
    assert_eq!(body("a[..=2];"), "[(slice= a _ 2)]");
    assert_eq!(body("a[1..][0];"), "[(index (slice a 1 _) 0)]");
    assert_eq!(
        body("for x in a[1..] { }"),
        "[(foreach x (slice a 1 _) [])]"
    );
    assert_eq!(body_error("a[1..=];"), "expected expression, found `]`");
    assert_eq!(body_error("a[..=];"), "expected expression, found `]`");
    assert_eq!(body_error("a[1..2] = 1;"), "expected `;`, found `=`");
}

#[test]
fn tuples() {
    assert_eq!(body("(1, 2);"), "[(tuple 1 2)]");
    assert_eq!(body("(1, 2,);"), "[(tuple 1 2)]");
    assert_eq!(body("(1, (2, 3));"), "[(tuple 1 (tuple 2 3))]");
    assert_eq!(body("(1);"), "[1]");
    assert_eq!(
        body("let x: (int, [real]) = t;"),
        "[(let x (int, [real]) t)]"
    );
    assert_eq!(body("return (a, b);"), "[(return (tuple a b))]");
    assert_eq!(body("f((1, 2));"), "[(call f [(tuple 1 2)])]");
}

#[test]
fn unpacking() {
    assert_eq!(body("let (a, b) = f();"), "[(let-tuple [a b] (call f []))]");
    assert_eq!(body("let (a, _, c,) = t;"), "[(let-tuple [a _ c] t)]");
}

#[test]
fn tuple_errors() {
    assert_eq!(body_error("(1,);"), "expected expression, found `)`");
    assert_eq!(body_error("(1, 2;"), "expected `)`, found `;`");
    assert_eq!(body_error("let (a) = t;"), "expected `,`, found `)`");
    assert_eq!(body_error("let (a, b);"), "expected `=`, found `;`");
    assert_eq!(body_error("let x: (int) = 1;"), "expected `,`, found `)`");
    assert_eq!(body_error("(a, b) = (b, a);"), "expected `;`, found `=`");
}

#[test]
fn strings() {
    assert_eq!(body(r#"print("hi");"#), r#"[(call print ["hi"])]"#);
    assert_eq!(
        body(r#"let s: string = "a" + "b";"#),
        r#"[(let s str (+ "a" "b"))]"#
    );
    assert_eq!(
        body_error(r#""a" "b";"#),
        r#"expected `;`, found string "b""#
    );
}

#[test]
fn dots() {
    assert_eq!(body("stats.G;"), "[(. stats G)]");
    assert_eq!(body("stats.mean(1);"), "[(call stats.mean [1])]");
    assert_eq!(body("m.a[0];"), "[(index (. m a) 0)]");
    assert_eq!(body("m.v:xy;"), "[(: (. m v) xy)]");
    assert_eq!(body("a.b.c;"), "[(. (. a b) c)]");
    assert_eq!(body("f().x;"), "[(. (call f []) x)]");
    assert_eq!(body("a[0].x[1];"), "[(index (. (index a 0) x) 1)]");
    assert_eq!(body_error("a.;"), "expected identifier, found `;`");
    assert_eq!(body("a.b.c(1);"), "[(call c [(. a b) 1])]");
    assert_eq!(body("f().g();"), "[(call g [(call f [])])]");
    assert_eq!(body("a[0].g();"), "[(call g [(index a 0)])]");
}

#[test]
fn field_assignment() {
    assert_eq!(body("p.x = 1;"), "[(index= p [.x] 1)]");
    assert_eq!(body("a[0].x.y += 1;"), "[(index+= a [0 .x .y] 1)]");
    assert_eq!(body_error("f().x = 1;"), "expected `;`, found `=`");
}

#[test]
fn struct_literals() {
    assert_eq!(body("P { x: 1, y: 2 };"), "[(new P [(x 1) (y 2)])]");
    assert_eq!(body("P { x: 1, };"), "[(new P [(x 1)])]");
    assert_eq!(body("P { };"), "[(new P [])]");
    assert_eq!(body("m.P { x: 1 };"), "[(new m.P [(x 1)])]");
    assert_eq!(
        body("P { a: Q { b: 1 } };"),
        "[(new P [(a (new Q [(b 1)]))])]"
    );
    assert_eq!(body("let q: m.P = p;"), "[(let q m.P p)]");
    assert_eq!(body_error("P { x };"), "expected `:`, found `}`");
    assert_eq!(
        body_error("P { x: 1 y: 2 };"),
        "expected `}`, found identifier `y`"
    );
}

#[test]
fn no_struct_literal_directly_in_headers() {
    assert_eq!(body("if x { }"), "[(if x [])]");
    assert_eq!(body("if p.x { }"), "[(if (. p x) [])]");
    assert_eq!(body("while x { }"), "[(while x [])]");
    assert_eq!(body("for i in a { }"), "[(foreach i a [])]");
    assert_eq!(body("for i in 0..n step k { }"), "[(for i 0 n step k [])]");
    // Inside brackets a literal is not in doubt.
    assert_eq!(body("if a == (P { }) { }"), "[(if (== a (new P [])) [])]");
    assert_eq!(
        body("while f(P { }) { }"),
        "[(while (call f [(new P [])]) [])]"
    );
    assert_eq!(
        body("for x in [P { }] { }"),
        "[(foreach x (array (new P [])) [])]"
    );
    // The body of `if` still parses as a block after a bracketed literal.
    assert_eq!(
        body("if (P { }) == a { x; }"),
        "[(if (== (new P []) a) [x])]"
    );
    assert_eq!(
        body_error("if P { x: 1 } { }"),
        "expected identifier, found number `1`"
    );
}
