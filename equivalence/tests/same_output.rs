//! This tree must render exactly what the published base release renders:
//! the same compile result for every template, and byte-for-byte the same
//! output for every view.
//!
//! The one exception is the random-piece property, for the places 0.9.3
//! reads the way a browser does where 0.9.2 did not: URL attributes, and
//! scripts across line breaks and HTML-like comments. There a difference is
//! allowed only when the template has such a place, and outputs that both
//! compile must still agree outside URL attribute values, with no URL
//! attribute holding a `javascript:` URL.

use proptest::prelude::*;

#[derive(Debug)]
struct Item {
    name: String,
    tags: Vec<String>,
    n: i64,
}

#[derive(Debug)]
struct Page {
    title: String,
    body: String,
    url: String,
    count: i64,
    ratio: f64,
    on: bool,
    missing: Option<String>,
    items: Vec<Item>,
}

luminess::view!(Item as "item" { name: text, tags: list(text), n: int });
luminess::view!(Page as "page" {
    title: text, body: html, url: text, count: int, ratio: float, on: bool,
    missing: text, items: list(object(Item)),
});
luminess_base::view!(Item as "item" { name: text, tags: list(text), n: int });
luminess_base::view!(Page as "page" {
    title: text, body: html, url: text, count: int, ratio: float, on: bool,
    missing: text, items: list(object(Item)),
});

/// Template pieces: every context, statement, and filter the engine has.
const PIECES: &[&str] = &[
    "{{ page.title }}",
    "{{ page.body }}",
    "{{ page.url }}",
    "{{ page.count }}",
    "{{ page.ratio }}",
    "{{ page.on }}",
    "{{ page.missing }}",
    "{{ page.missing | default(\"none\") }}",
    "{{ page.title | upper }}",
    "{{ page.title | lower | trim }}",
    "{{ page.title | truncate(5) }}",
    "{{ page.title | replace(\"a\", \"<b>\") }}",
    "{{ page.title | length }}",
    "{{ page.count | number }}",
    "{{ page.count | date(\"%Y-%m-%d %H:%M\") }}",
    "{{ page.title | raw }}",
    "{% for i in page.items %}",
    "{% else %}",
    "{% endfor %}",
    "{{ i.name }}",
    "{{ i.n }}",
    "{{ loop.index }}",
    "{{ loop.index0 }}{{ loop.revindex }}",
    "{% if loop.first %}F{% endif %}",
    "{% if loop.last %}L{% endif %}",
    "{% for t in i.tags %}[{{ t }}]{% endfor %}",
    "{{ i.tags | join(\", \") }}",
    "{{ i.tags | first }}{{ i.tags | last }}",
    "{% if page.on %}",
    "{% if not page.on and page.count > 3 %}",
    "{% if page.title == \"x\" or page.ratio >= 1.5 %}",
    "{% elif page.missing %}",
    "{% endif %}",
    "{% set s = page.title | upper %}{{ s }}",
    "{% set k = page.count %}{{ k }}",
    "{% try %}",
    "{% fallback %}",
    "{% endtry %}",
    "<p>",
    "</p>",
    "<a href=\"",
    "<a href=\"/x?q=",
    "\">",
    "<p title='",
    "'>",
    "<script>var a = ",
    "<script>var a = '",
    "';",
    "</script>",
    "<style>a { color: ",
    "; }</style>",
    "<!-- c -->",
    "{# c #}",
    "{{- page.count -}}",
    " \n ",
    "text & more",
];

/// Values that lean on escaping.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("<".to_owned()),
            Just(">".to_owned()),
            Just("&".to_owned()),
            Just("\"".to_owned()),
            Just("'".to_owned()),
            Just("javascript:".to_owned()),
            Just("</script>".to_owned()),
            Just("${".to_owned()),
            Just("%".to_owned()),
            Just("/".to_owned()),
            Just(" ".to_owned()),
            Just("\n".to_owned()),
            Just("rgb(1, 2, 3)".to_owned()),
            any::<char>().prop_map(String::from),
            "[a-zA-Z0-9]{1,12}",
        ],
        0..20,
    )
    .prop_map(|v| v.concat())
}

fn page() -> impl Strategy<Value = Page> {
    let item = (text(), prop::collection::vec(text(), 0..3), any::<i64>())
        .prop_map(|(name, tags, n)| Item { name, tags, n });
    (
        text(),
        text(),
        text(),
        any::<i64>(),
        any::<f64>(),
        any::<bool>(),
        prop::option::of(text()),
        prop::collection::vec(item, 0..4),
    )
        .prop_map(
            |(title, body, url, count, ratio, on, missing, items)| Page {
                title,
                body,
                url,
                count,
                ratio,
                on,
                missing,
                items,
            },
        )
}

static COMPILED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The template has a place where 0.9.3 changed on purpose.
fn changed_on_purpose(src: &str) -> bool {
    src.contains("href=")
        || (src.contains("<script") && (src.contains('\n') || src.contains("<!--")))
}

/// `s` with the value of every `href` attribute removed.
fn without_urls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("href=\"") {
        out.push_str(&rest[..i + 6]);
        rest = &rest[i + 6..];
        let end = rest.find('"').unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// No URL attribute value is a `javascript:` URL, as a browser reads it.
fn no_script_urls(s: &str) -> bool {
    s.split("href=\"").skip(1).all(|v| {
        let v = &v[..v.find('"').unwrap_or(v.len())];
        let v: String = v
            .trim_start_matches(|c: char| c <= ' ')
            .chars()
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .collect();
        !v.to_ascii_lowercase().starts_with("javascript:")
    })
}

fn same(src: &str, p: &Page) -> Result<(), TestCaseError> {
    compare(src, p, true)
}

fn compare(src: &str, p: &Page, exact: bool) -> Result<(), TestCaseError> {
    let now = luminess::Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .compile();
    let base = luminess_base::Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .compile();
    match (now, base) {
        (Ok(now), Ok(base)) => {
            let _ = COMPILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let a = now.render_to_string("t", p).unwrap();
            let b = base.render_to_string("t", p).unwrap();
            if exact || a == b {
                prop_assert_eq!(a, b, "template: {:?}", src);
            } else {
                prop_assert!(changed_on_purpose(src), "template: {:?}", src);
                prop_assert_eq!(without_urls(&a), without_urls(&b), "template: {:?}", src);
                prop_assert!(no_script_urls(&a), "template: {:?}: {}", src, a);
            }
        }
        (Err(now), Err(base)) => {
            if exact || !changed_on_purpose(src) {
                prop_assert_eq!(now.to_string(), base.to_string(), "template: {:?}", src);
            }
        }
        (Ok(now), Err(_)) if !exact && changed_on_purpose(src) => {
            let a = now.render_to_string("t", p).unwrap();
            prop_assert!(no_script_urls(&a), "template: {:?}: {}", src, a);
        }
        (Err(_), Ok(_)) if !exact && changed_on_purpose(src) => {}
        (now, base) => {
            prop_assert!(
                false,
                "compile results differ for {:?}: now ok={} base ok={}",
                src,
                now.is_ok(),
                base.is_ok()
            );
        }
    }
    Ok(())
}

/// Whole units that are valid wherever HTML text is.
const UNITS: &[&str] = &[
    "<h1>{{ page.title }}</h1>",
    "<p title=\"{{ page.title }}\">{{ page.body }}</p>",
    "<a href=\"{{ page.url }}\">u</a>",
    "<a href=\"/x/{{ page.title }}?q={{ page.url }}&n={{ page.count }}\">q</a>",
    "<script>var a = {{ page.title }}, b = \"{{ page.url }}\", c = {{ page.ratio }}, d = {{ page.missing }};</script>",
    "<style>p { color: {{ page.title }}; font: '{{ page.url }}'; }</style>",
    "{{ page.count }} {{ page.ratio }} {{ page.on }} {{ page.missing }}",
    "{{ page.title | upper | truncate(7) }}{{ page.missing | default(\"-\") }}",
    "{{ page.count | number }} {{ page.count | date(\"%d %b %Y\") }}",
    "{% set s = page.title | lower %}[{{ s }}]",
    "text & more ",
];

fn template() -> impl Strategy<Value = String> {
    let leaf = prop::sample::select(UNITS).prop_map(str::to_owned);
    leaf.prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 1..5).prop_map(|v| v.concat()),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("{{% if page.on %}}{a}{{% else %}}{b}{{% endif %}}")),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!(
                "{{% for i in page.items %}}<li class=\"{{% if loop.first %}}f{{% endif %}}\">{{{{ loop.index }}}} {{{{ i.name }}}} {{{{ i.n }}}}{a}{{% for t in i.tags %}}<a href=\"/t/{{{{ t }}}}\">{{{{ t }}}}</a>{{% endfor %}}</li>{{% else %}}{b}{{% endfor %}}"
            )),
            (inner.clone(), inner).prop_map(|(a, b)| format!("{{% try %}}{a}{{% fallback %}}{b}{{% endtry %}}")),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4000))]

    #[test]
    fn structured_templates_render_the_same(src in template(), p in page()) {
        let compiles = luminess::Engine::<Page>::builder().template("t", src.as_str()).compile();
        prop_assert!(compiles.is_ok(), "{:?}: {}", src, compiles.err().map(|e| e.to_string()).unwrap_or_default());
        same(&src, &p)?;
    }

    #[test]
    fn templates_render_the_same(
        parts in prop::collection::vec(prop::sample::select(PIECES), 0..24),
        p in page(),
    ) {
        compare(&parts.concat(), &p, false)?;
    }

    #[test]
    fn well_formed_pages_render_the_same(p in page()) {
        // A page that compiles for certain, so every value path is checked.
        let src = "<h1 title=\"{{ page.title }}\">{{ page.title }}</h1>{{ page.body }}\
            <a href=\"{{ page.url }}\">x</a><a href=\"/s?q={{ page.title }}\">y</a>\
            {% for i in page.items %}<li class=\"{% if loop.first %}first{% endif %}\">{{ loop.index }}. {{ i.name }} {{ i.n }}\
            {% for t in i.tags %}<a href=\"/tags/{{ t }}\">{{ t }}</a>{% endfor %}</li>{% else %}<li>none</li>{% endfor %}\
            {% if page.on %}{{ page.count }}{% else %}{{ page.ratio }}{% endif %}{{ page.missing | default(\"-\") }}\
            <script>var a = {{ page.title }}, b = '{{ page.title }}', n = {{ page.count }};</script>\
            <style>a { color: {{ page.title }}; }</style>";
        same(src, &p)?;
    }
}

#[test]
#[ignore = "diagnostic"]
fn compile_rate() {
    use proptest::{strategy::ValueTree, test_runner::TestRunner};
    let mut runner = TestRunner::default();
    let strat = prop::collection::vec(prop::sample::select(PIECES), 0..24);
    let mut ok = 0;
    for _ in 0..4000 {
        let parts = strat.new_tree(&mut runner).unwrap().current();
        if luminess::Engine::<Page>::builder()
            .template("t", parts.concat())
            .template("part", "")
            .compile()
            .is_ok()
        {
            ok += 1;
        }
    }
    println!("compile rate: {ok}/4000");
}
