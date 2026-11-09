//! Any text as a value: it never leaves the place the template put it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use luminess::{Engine, escape};

struct Page {
    v: String,
}
luminess::view!(Page as "page" { v: text });

fuzz_target!(|data: &[u8]| {
    let Ok(v) = std::str::from_utf8(data) else { return };
    let engine = Engine::<Page>::builder()
        .template("text", "<p>{{ page.v }}</p>")
        .template("dq", "<a title=\"{{ page.v }}\">")
        .template("sq", "<a title='{{ page.v }}'>")
        .template("url", "<a href=\"{{ page.v }}\">")
        .template("query", "<a href=\"/s?q={{ page.v }}\">")
        .build()
        .expect("compiles");
    let page = Page { v: v.to_owned() };
    let r = |n: &str| engine.render_to_string(n, &page).expect("renders");

    let text = r("text");
    assert!(!text[3..text.len() - 4].contains('<'));
    let dq = r("dq");
    assert!(!dq[10..dq.len() - 2].contains('"'));
    let sq = r("sq");
    assert!(!sq[10..sq.len() - 2].contains('\''));
    let url = r("url");
    let inner = &url[9..url.len() - 2];
    assert!(!inner.contains(['"', '\'', '<', '>', ' ']));
    if !escape::url_allowed(v) {
        assert_eq!(inner, escape::BLOCKED_URL);
    }
    let q = r("query");
    let inner = &q[14..q.len() - 2];
    assert!(inner.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._~%".contains(&c)), "{inner}");
});
