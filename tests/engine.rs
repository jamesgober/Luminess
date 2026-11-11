//! The engine handle: reloads reach every thread, and a render may render
//! through another engine from inside a block.

use std::sync::{
    Arc, Barrier,
    atomic::{AtomicBool, Ordering},
};

use luminess::{BlockError, Engine, HasSchema, Output, Schema, ToValue, Value, View};

struct Page {
    n: i64,
}
luminess::view!(Page as "page" { n: int });

#[test]
fn reloads_reach_threads_that_rendered_before() {
    let engine = Engine::<Page>::builder()
        .template("t", "old {{ page.n }}")
        .build()
        .unwrap();
    let rendered = Arc::new(Barrier::new(2));
    let reloaded = Arc::new(Barrier::new(2));
    let e = engine.clone();
    let (a, b) = (Arc::clone(&rendered), Arc::clone(&reloaded));
    let worker = std::thread::spawn(move || {
        let p = Page { n: 1 };
        // Fill this thread's cache with the old set.
        assert_eq!(e.render("t", &p).unwrap(), "old 1");
        let _ = a.wait();
        let _ = b.wait();
        e.render("t", &p).unwrap()
    });
    let _ = rendered.wait();
    engine.replace(
        Engine::<Page>::builder()
            .template("t", "new {{ page.n }}")
            .build()
            .unwrap(),
    );
    let _ = reloaded.wait();
    assert_eq!(worker.join().unwrap(), "new 1");
}

#[test]
fn many_engines_on_one_thread() {
    // More engines than the per-thread cache holds: each still renders its
    // own set.
    let engines: Vec<Engine<Page>> = (0..10)
        .map(|i| {
            Engine::<Page>::builder()
                .template("t", format!("{i}:{{{{ page.n }}}}"))
                .build()
                .unwrap()
        })
        .collect();
    for round in 0..3 {
        for (i, e) in engines.iter().enumerate() {
            assert_eq!(
                e.render("t", &Page { n: round }).unwrap(),
                format!("{i}:{round}")
            );
        }
    }
}

/// A block whose own markup comes from a second engine.
struct Card {
    inner: Engine<Page>,
    hits: Arc<AtomicBool>,
}

impl HasSchema for Card {
    fn schema() -> Schema {
        Schema::build("card").renders().finish()
    }
}

impl View for Card {
    fn field(&self, _: usize) -> Value<'_> {
        Value::Absent
    }

    fn render(&self, out: &mut Output<'_>) -> Result<(), BlockError> {
        self.hits.store(true, Ordering::Relaxed);
        let s = self
            .inner
            .render("inner", &Page { n: 7 })
            .map_err(|e| BlockError::new(e.to_string()))?;
        out.markup(&s);
        Ok(())
    }
}

impl ToValue for Card {
    fn to_value(&self) -> Value<'_> {
        Value::View(self)
    }
}

struct Outer {
    card: Card,
}
luminess::view!(Outer as "outer" { card: object(Card) });

#[test]
fn a_block_can_render_through_another_engine() {
    let inner = Engine::<Page>::builder()
        .template("inner", "<i>{{ page.n }}</i>")
        .build()
        .unwrap();
    let outer = Engine::<Outer>::builder()
        .template("o", "<b>{{ outer.card }}</b>")
        .build()
        .unwrap();
    let hits = Arc::new(AtomicBool::new(false));
    let v = Outer {
        card: Card {
            inner,
            hits: Arc::clone(&hits),
        },
    };
    for _ in 0..3 {
        assert_eq!(outer.render("o", &v).unwrap(), "<b><i>7</i></b>");
    }
    assert!(hits.load(Ordering::Relaxed));
}
