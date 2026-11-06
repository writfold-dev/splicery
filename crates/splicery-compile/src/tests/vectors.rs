use super::common::*;
use super::*;

#[test]
fn all_eleven_recipe_vectors() {
    let cases = [
        (
            "01-await",
            include_str!("../../../../examples/01-await.spl"),
            &b"let reply = transport.send(request)?;\r\n"[..],
            &b"let reply = transport.send(request).await?;\r\n"[..],
            Profile::Text,
        ),
        (
            "02-wrap",
            include_str!("../../../../examples/02-wrap.spl"),
            &b"let x = send(\r\n    req, // caf\xc3\xa9\n)?;"[..],
            &b"let x = trace_send(send(\r\n    req, // caf\xc3\xa9\n))?;"[..],
            Profile::Text,
        ),
        (
            "03-policy",
            include_str!("../../../../examples/03-policy.spl"),
            &b"keep\n<!-- policy:begin -->\nobsolete\n<!-- policy:end -->\nend"[..],
            &b"keep\n<!-- policy:begin -->\nAttempts: 3\nBackoff: exponential\n<!-- policy:end -->\nend"[..],
            Profile::Text,
        ),
        (
            "04-routes",
            include_str!("../../../../examples/04-routes.spl"),
            &b"# routes\r\nroute  GET /health\r\nroute POST /jobs\nroute GET /v2/jobs"[..],
            &b"# routes\r\nGET /health\r\nPOST /jobs\nGET /v2/jobs"[..],
            Profile::Text,
        ),
        (
            "05-swap",
            include_str!("../../../../examples/05-swap.spl"),
            &b"alpha beta"[..],
            &b"beta alpha"[..],
            Profile::Text,
        ),
        (
            "06-raw",
            include_str!("../../../../examples/06-raw.spl"),
            &[0xaa, 0x00, 0xff, 0xbb][..],
            &[0xaa, 0x80, 0xfe, 0xbb][..],
            Profile::Raw,
        ),
        (
            "07-lines",
            include_str!("../../../../examples/07-lines.spl"),
            &b"[capture]\r\nobsolete"[..],
            &b"[capture]\r\nA\r\nB\r\n"[..],
            Profile::Text,
        ),
        (
            "08-indent",
            include_str!("../../../../examples/08-indent.spl"),
            &b"a\r\n\nbc"[..],
            &b"  a\r\n\n  bc"[..],
            Profile::Text,
        ),
        (
            "09-import",
            include_str!("../../../../examples/09-import.spl"),
            &b"// imports begin\nuse lib::Config;\n// imports end\nuse lib::Config;\n"[..],
            &b"// imports begin\nuse lib::Policy;\nuse lib::Config;\n// imports end\nuse lib::Config;\n"[..],
            Profile::Text,
        ),
        (
            "10-move",
            include_str!("../../../../examples/10-move.spl"),
            &b"# pinned\noption=old\nfoo=1\nTAIL"[..],
            &b"foo=1\n# pinned\noption=old\nTAIL"[..],
            Profile::Text,
        ),
    ];
    for (name, source, input, expected, profile) in cases {
        assert_eq!(
            run_recipe(source, input, profile).as_deref(),
            Ok(expected),
            "{name}"
        );
    }

    let absent = run_recipe(
        include_str!("../../../../examples/11-absence.spl"),
        b"y",
        Profile::Text,
    );
    assert!(
        matches!(&absent, Err(error) if error.contains("AbsentGroup")),
        "{absent:?}"
    );
}
