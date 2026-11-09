//! Methods with arguments, `set`, the built-in filters added in 0.6, and
//! application filters.

use luminess::{Computed, Engine, FieldId, Kind, Schema, Shaped, ToValue, Value, View};

/// A shop product's variations: a custom block type with its own methods.
struct Variations {
    sizes: Vec<String>,
    prices: Vec<i64>,
}

impl Shaped for Variations {
    fn schema() -> Schema {
        Schema::build("variations")
            .int("count")
            .method("cheapest", &[], Kind::Int)
            .method("options", &[Kind::Text], Kind::list(Kind::Text))
            .method("label", &[Kind::Text, Kind::Int], Kind::Text)
            .finish()
    }
}

impl View for Variations {
    fn field(&self, id: FieldId) -> Value<'_> {
        match id.index() {
            0 => Value::Int(self.sizes.len() as i64),
            _ => Value::Absent,
        }
    }

    fn call<'s>(&'s self, id: FieldId, args: &[Value<'_>]) -> Computed<'s> {
        match (id.index(), args) {
            (1, []) => Value::Int(self.prices.iter().copied().min().unwrap_or(0)).into(),
            (2, [Value::Text("size")]) => Value::List(&self.sizes).into(),
            (3, [Value::Text(prefix), Value::Int(n)]) => Computed::Text(format!("{prefix} #{n}")),
            _ => Value::Absent.into(),
        }
    }
}

impl ToValue for Variations {
    fn to_value(&self) -> Value<'_> {
        Value::View(self)
    }
}

struct Product {
    name: String,
    published: i64,
    sold: i64,
    tags: Vec<String>,
    variations: Variations,
}

luminess::view!(Product as "product" {
    name: text,
    published: int,
    sold: int,
    tags: list(text),
    variations: object(Variations),
});

fn product() -> Product {
    Product {
        name: "Trail <shoe> runner".into(),
        published: 1_762_560_000, // 2025-11-08 00:00:00 UTC
        sold: 1_234_567,
        tags: vec!["run".into(), "trail".into(), "shoe".into()],
        variations: Variations {
            sizes: vec!["S".into(), "M".into(), "L".into()],
            prices: vec![5900, 4900, 6900],
        },
    }
}

fn render(src: &str) -> String {
    let e = Engine::<Product>::builder()
        .template("t", src)
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    e.render_to_string("t", &product()).unwrap()
}

fn error(src: &str) -> String {
    match Engine::<Product>::builder().template("t", src).build() {
        Ok(_) => panic!("expected an error for {src}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn methods() {
    assert_eq!(
        render("from {{ product.variations.cheapest() }}"),
        "from 4900"
    );
    assert_eq!(
        render(
            r#"{% for s in product.variations.options("size") %}{{ s }}{% if not loop.last %}/{% endif %}{% endfor %}"#
        ),
        "S/M/L"
    );
    assert_eq!(
        render(r#"{{ product.variations.label("Variant", product.variations.count) }}"#),
        "Variant #3"
    );
    assert_eq!(
        render(r#"{{ product.variations.options("color") | length }}"#),
        "0"
    );
    assert_eq!(
        render("{% if product.variations.cheapest() < 5000 %}deal{% endif %}"),
        "deal"
    );
}

#[test]
fn method_errors() {
    for (src, want) in [
        ("{{ product.variations.cheapest }}", "is a method"),
        ("{{ product.variations.count() }}", "is a field"),
        (
            "{{ product.variations.cheapst() }}",
            "did you mean `cheapest`",
        ),
        ("{{ product.variations.cheapest(1) }}", "takes 0 arguments"),
        (
            "{{ product.variations.options(1) }}",
            "argument 1 of `options` should be text",
        ),
        (
            r#"{{ product.variations.label("a", "b") }}"#,
            "argument 2 of `label` should be an int",
        ),
        ("{{ upper(product.name) }}", "unknown function `upper`"),
    ] {
        let e = error(src);
        assert!(e.contains(want), "{src}: {e}");
    }
}

#[test]
fn set_values() {
    assert_eq!(
        render(r#"{% set n = product.name | upper %}<b>{{ n }}</b><i title="{{ n }}"></i>"#),
        r#"<b>TRAIL &lt;SHOE&gt; RUNNER</b><i title="TRAIL &lt;SHOE&gt; RUNNER"></i>"#
    );
    assert_eq!(
        render("{% set v = product.variations %}{{ v.cheapest() }}"),
        "4900"
    );
    assert_eq!(
        render("{% set big = product.sold > 1000 %}{% if big %}big{% endif %}"),
        "big"
    );
    // A set inside a branch ends with the branch.
    assert!(error("{% if true %}{% set x = 1 %}{% endif %}{{ x }}").contains("unknown name `x`"));
    // A set in a loop body does not hide the loop from `loop`.
    assert_eq!(
        render(
            "{% for t in product.tags %}{% set u = t | upper %}{{ loop.index }}{{ u }}{% endfor %}"
        ),
        "1RUN2TRAIL3SHOE"
    );
    assert!(error("{% set product = 1 %}").contains("root value"));
    assert!(error("{% set = 1 %}").contains("expected `set name = value`"));
}

#[test]
fn built_in_filters() {
    assert_eq!(render("{{ product.name | truncate(5) }}"), "Trail...");
    assert_eq!(
        render("{{ product.name | truncate(100) }}"),
        "Trail &lt;shoe&gt; runner"
    );
    assert_eq!(
        render(r#"{{ product.tags | join(", ") }}"#),
        "run, trail, shoe"
    );
    assert_eq!(
        render("{{ product.tags | first }}-{{ product.tags | last }}"),
        "run-shoe"
    );
    assert_eq!(
        render(r#"{{ product.name | replace("shoe", "boot") }}"#),
        "Trail &lt;boot&gt; runner"
    );
    assert_eq!(
        render(r#"{{ product.published | date("%d %B %Y, %H:%M") }}"#),
        "08 November 2025, 00:00"
    );
    assert_eq!(
        render(r#"{{ product.published | date("%Y-%m-%d %b %%") }}"#),
        "2025-11-08 Nov %"
    );
    assert_eq!(render("{{ product.sold | number }}"), "1,234,567");
    for (src, want) in [
        (
            r#"{{ product.name | join(",") }}"#,
            "`join` applies to a list",
        ),
        (
            "{{ product.tags | truncate(2) }}",
            "`truncate` applies to text",
        ),
        (r#"{{ product.name | truncate("x") }}"#, "should be an int"),
        (
            r#"{{ product.name | date("%Y") }}"#,
            "`date` applies to an int",
        ),
        ("{{ product.name | number }}", "applies to numbers"),
        ("{{ product.name | first }}", "applies to a list"),
    ] {
        let e = error(src);
        assert!(e.contains(want), "{src}: {e}");
    }
}

#[test]
fn dates_across_eras() {
    struct T {
        at: i64,
    }
    luminess::view!(T as "t" { at: int });
    let e = Engine::<T>::builder()
        .template("d", r#"{{ t.at | date("%Y-%m-%d %H:%M:%S") }}"#)
        .build()
        .unwrap();
    for (secs, want) in [
        (0, "1970-01-01 00:00:00"),
        (951_782_400, "2000-02-29 00:00:00"),
        (-86_400, "1969-12-31 00:00:00"),
        (4_102_444_799, "2099-12-31 23:59:59"),
    ] {
        assert_eq!(e.render_to_string("d", &T { at: secs }).unwrap(), want);
    }
}

#[test]
fn application_filters() {
    let engine = Engine::<Product>::builder()
        .filter("money", &[Kind::Text], Kind::Text, |v, args| {
            let (Value::Int(c), [Value::Text(cur)]) = (v, args) else {
                return Value::Absent.into();
            };
            Computed::Text(format!("{}.{:02} {cur}", c / 100, c % 100))
        })
        .filter("shout", &[], Kind::Text, |v, _| match v {
            Value::Text(s) => Computed::Text(format!("{s}!")),
            other => other.into(),
        })
        .template(
            "t",
            r#"{{ product.variations.cheapest() | money("EUR") }} {{ product.name | upper | shout }}"#,
        )
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        engine.render_to_string("t", &product()).unwrap(),
        "49.00 EUR TRAIL &lt;SHOE&gt; RUNNER!"
    );

    let err = Engine::<Product>::builder()
        .filter("money", &[Kind::Text], Kind::Text, |v, _| v.into())
        .template("a", "{{ product.sold | money }}")
        .template("b", "{{ product.sold | money(1) }}")
        .template("c", r#"{{ product.sold | mony("x") }}"#)
        .build()
        .unwrap_err()
        .to_string();
    assert!(err.contains("`money` takes 1 argument"), "{err}");
    assert!(err.contains("should be text"), "{err}");
    assert!(err.contains("did you mean `money`"), "{err}");
}

#[test]
#[should_panic(expected = "built-in filter")]
fn built_in_names_are_reserved() {
    let _ = Engine::<Product>::builder().filter("upper", &[], Kind::Text, |v, _| v.into());
}
