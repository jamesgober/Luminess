//! The one-line entry points: a theme from a folder, one template, one
//! render, and the one error type they share.

use std::path::PathBuf;

use luminess::{Engine, ErrorKind, Template};

struct Page {
    title: String,
}
luminess::view!(Page as "page" { title: text });

fn page(title: &str) -> Page {
    Page {
        title: title.into(),
    }
}

/// A fresh folder for one test, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("luminess-tier1-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("partials")).unwrap();
        Self(p)
    }

    fn write(&self, rel: &str, text: &str) {
        std::fs::write(self.0.join(rel), text).unwrap();
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_from_dir_loads_html_files_by_path() {
    let d = Dir::new("load");
    d.write(
        "home.html",
        "<h1>{{ page.title }}</h1>{% include \"partials/nav\" %}",
    );
    d.write("partials/nav.html", "<nav>{{ page.title }}</nav>");
    d.write("notes.txt", "not a template {{ x }}");
    let engine = Engine::<Page>::from_dir(&d.0).unwrap();
    assert_eq!(
        engine.render("home", &page("A & B")).unwrap(),
        "<h1>A &amp; B</h1><nav>A &amp; B</nav>"
    );
    assert_eq!(
        engine.render("notes", &page("x")).unwrap_err().kind(),
        ErrorKind::NotFound
    );
}

#[test]
fn test_from_dir_missing_folder_is_io() {
    let e = Engine::<Page>::from_dir("no/such/luminess/folder").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Io);
    assert!(e.template().contains("folder"), "{}", e.template());
    assert!(std::error::Error::source(&e).is_some());
    assert!(e.to_string().starts_with("could not read `"), "{e}");
    assert!(e.diagnostics().is_empty());
}

#[test]
fn test_from_dir_bad_template_is_compile() {
    let d = Dir::new("bad");
    d.write("home.html", "{{ page.titel }}");
    let e = Engine::<Page>::from_dir(&d.0).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Compile);
    assert_eq!(e.diagnostics().len(), 1);
    assert!(
        e.report().contains("did you mean `title`"),
        "{}",
        e.report()
    );
}

#[test]
fn test_question_mark_works_across_loading_and_rendering() {
    fn site(dir: &std::path::Path) -> Result<String, luminess::Error> {
        let engine = Engine::<Page>::from_dir(dir)?;
        engine.render("home", &page("ok"))
    }
    fn boxed(dir: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
        Ok(site(dir)?)
    }
    let d = Dir::new("qm");
    d.write("home.html", "{{ page.title }}");
    assert_eq!(site(&d.0).unwrap(), "ok");
    assert_eq!(boxed(&d.0).unwrap(), "ok");
}

#[test]
fn test_reload_dir_swaps_and_keeps_on_error() {
    let d = Dir::new("reload");
    d.write("home.html", "one {{ page.title }}");
    let engine = Engine::<Page>::from_dir(&d.0).unwrap();
    d.write("home.html", "two {{ page.title }}");
    engine.reload_dir(&d.0).unwrap();
    assert_eq!(engine.render("home", &page("x")).unwrap(), "two x");
    d.write("home.html", "{{ page.nope }}");
    assert_eq!(
        engine.reload_dir(&d.0).unwrap_err().kind(),
        ErrorKind::Compile
    );
    assert_eq!(engine.render("home", &page("x")).unwrap(), "two x");
    assert_eq!(
        engine.reload_dir(d.0.join("gone")).unwrap_err().kind(),
        ErrorKind::Io
    );
}

#[test]
fn test_directory_with_extension_picks_other_files() {
    let d = Dir::new("ext");
    d.write("mail.txt", "Hello {{ page.title }}");
    d.write("page.html", "{{ page.nope }}");
    let engine = Engine::<Page>::builder()
        .directory_with_extension(&d.0, ".txt")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(engine.render("mail", &page("Ann")).unwrap(), "Hello Ann");
}

#[test]
fn test_template_renders_and_reports() {
    let t = Template::<Page>::new("<b>{{ page.title }}</b>").unwrap();
    assert_eq!(t.render(&page("<x>")).unwrap(), "<b>&lt;x&gt;</b>");
    let mut out = b"keep:".to_vec();
    let r = t.render_into(&page("y"), &mut out).unwrap();
    assert_eq!(out, b"keep:<b>y</b>");
    assert_eq!(r.bytes(), 8);
    let clone = t.clone();
    assert_eq!(clone.render(&page("z")).unwrap(), "<b>z</b>");
}

#[test]
fn test_template_errors_are_compile() {
    let e = Template::<Page>::new("{% if %}").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Compile);
    let e = Template::<Page>::new("{% include \"other\" %}").unwrap_err();
    assert!(e.to_string().contains("unknown template `other`"), "{e}");
}

#[test]
fn test_one_shot_render() {
    assert_eq!(
        luminess::render("{{ page.title }}!", &page("hi")).unwrap(),
        "hi!"
    );
    let e = luminess::render("{{ page.x }}", &page("hi")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Compile);
}

#[test]
fn test_error_reports_for_every_kind() {
    let not_found = Engine::<Page>::builder()
        .build()
        .unwrap()
        .render("x", &page(""))
        .unwrap_err();
    assert_eq!(not_found.report(), "error: no template named `x`\n");
    assert_eq!(not_found.template(), "x");
    let too_large = Engine::<Page>::builder()
        .max_output(3)
        .template("t", "{{ page.title }}")
        .build()
        .unwrap()
        .render("t", &page("long title"))
        .unwrap_err();
    assert_eq!(too_large.kind(), ErrorKind::TooLarge);
    assert!(
        too_large
            .to_html()
            .starts_with("<pre class=\"luminess-errors\">error: the output of `t`")
    );
}
