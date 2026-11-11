//! The engine (a named set of templates you can reload), its builder, and a
//! single compiled template.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt, fs,
    hash::{BuildHasherDefault, Hasher},
    marker::PhantomData,
    path::Path,
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    compile::{self, FilterSig, Program, Source},
    error::{Diagnostic, Error},
    parse,
    render::{self, Rendered},
    view::{Answer, FilterFn, HasSchema, Kind, Schema, Value, View},
};

/// Collects templates, options, and filters, then compiles them all at once:
/// what [`Engine::builder`] returns when the one-line constructors are not
/// enough.
///
/// ```
/// use luminess::Engine;
///
/// struct Page { title: String }
/// luminess::view!(Page as "page" { title: text });
///
/// let engine = Engine::<Page>::builder()
///     .template("home", "<h1>{{ page.title }}</h1>")
///     .template("about", "<p>{{ page.title }}</p>")
///     .max_output(1 << 20)
///     .build()?;
/// assert_eq!(engine.render("about", &Page { title: "Us".into() })?, "<p>Us</p>");
///
/// // Every mistake in every template, with names, lines, and columns.
/// let err = Engine::<Page>::builder().template("home", "<h1>{{ page.titel }}</h1>").build().unwrap_err();
/// assert_eq!(
///     err.diagnostics()[0].to_string(),
///     "home:1:13: `page` (page) has no field `titel`, did you mean `title`?"
/// );
/// # Ok::<(), luminess::Error>(())
/// ```
#[must_use]
pub struct Builder<V> {
    schema: Schema,
    max_output: usize,
    max_template: usize,
    sources: Vec<(String, String, Option<Schema>)>,
    filters: Vec<(FilterSig, Arc<dyn FilterFn>)>,
    _view: PhantomData<fn(&V)>,
}

impl<V> fmt::Debug for Builder<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("schema", &self.schema.name())
            .field(
                "templates",
                &self.sources.iter().map(|s| &s.0).collect::<Vec<_>>(),
            )
            .field(
                "filters",
                &self
                    .filters
                    .iter()
                    .map(|(s, _)| &s.name)
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl<V: View> Builder<V> {
    pub(crate) fn new(schema: Schema) -> Self {
        Self {
            schema,
            max_output: usize::MAX,
            max_template: DEFAULT_MAX_TEMPLATE,
            sources: Vec::new(),
            filters: Vec::new(),
            _view: PhantomData,
        }
    }

    /// Adds a template that compiles against the builder's schema.
    pub fn template(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.sources.push((name.into(), source.into(), None));
        self
    }

    /// Adds a template that compiles against its own schema: a partial
    /// included `with` a value of that schema (`{% include "card" with post %}`).
    ///
    /// Every template of an engine is rendered with the engine's view type,
    /// which answers its own schema's fields. So a template added here can be
    /// rendered by name only when its schema reads the view's fields in the
    /// same order (the same names and kinds at the same positions, as a
    /// prefix of the view's schema). Otherwise it can only be included with a
    /// value of its own schema, and rendering it by name finds no template.
    pub fn template_with_schema(
        mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        schema: Schema,
    ) -> Self {
        self.sources
            .push((name.into(), source.into(), Some(schema)));
        self
    }

    /// Sets the most bytes one render may write. A render that would pass
    /// it stops, removes what it wrote, and fails with
    /// [`ErrorKind::TooLarge`](crate::ErrorKind::TooLarge).
    /// Unlimited by default.
    pub fn max_output(mut self, bytes: usize) -> Self {
        self.max_output = bytes;
        self
    }

    /// Sets the largest template source the builder accepts, in bytes; a
    /// larger one is a compile error. 1 MiB by default.
    pub fn max_template_size(mut self, bytes: usize) -> Self {
        self.max_template = bytes;
        self
    }

    /// Compiles every template and reports the errors, keeping nothing:
    /// for checking a theme in an editor or in CI.
    ///
    /// ```
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let theme = luminess::Engine::<Page>::builder().template("home", "{{ page.tilte }}");
    /// assert!(theme.check().is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// As [`Builder::build`].
    pub fn check(&self) -> Result<(), Error> {
        let sources = self.sources.clone();
        let sigs: Vec<FilterSig> = self.filters.iter().map(|(s, _)| s.clone()).collect();
        Self::compile_parts(&self.schema, sources, &sigs, self.max_template).map(|_| ())
    }

    /// Adds an application filter. `params` are the kinds of its arguments
    /// and `returns` the kind of what it gives back; the compiler checks
    /// every use against them. A built-in filter's name cannot be taken.
    ///
    /// ```
    /// use luminess::{Answer, Engine, Kind, Value};
    ///
    /// struct Order { cents: i64 }
    /// luminess::view!(Order as "order" { cents: int });
    ///
    /// let engine = Engine::<Order>::builder()
    ///     .filter("money", &[Kind::Text], Kind::Text, |v, args| {
    ///         let (Value::Int(c), [Value::Text(cur)]) = (v, args) else { return Value::Absent.into() };
    ///         Answer::Text(format!("{}.{:02} {cur}", c / 100, c % 100))
    ///     })
    ///     .template("t", r#"{{ order.cents | money("EUR") }}"#)
    ///     .build()?;
    /// assert_eq!(engine.render("t", &Order { cents: 1999 })?, "19.99 EUR");
    /// # Ok::<(), luminess::Error>(())
    /// ```
    ///
    /// # Panics
    ///
    /// When `name` is a built-in filter or was added before.
    pub fn filter<F>(mut self, name: &str, params: &[Kind], returns: Kind, f: F) -> Self
    where
        F: for<'v, 'w> Fn(Value<'v>, &[Value<'w>]) -> Answer<'v> + Send + Sync + 'static,
    {
        assert!(
            !compile::BUILTIN_FILTERS.contains(&name),
            "luminess: `{name}` is a built-in filter"
        );
        assert!(
            !self.filters.iter().any(|(s, _)| &*s.name == name),
            "luminess: filter `{name}` added twice"
        );
        let sig = FilterSig {
            name: name.into(),
            params: params.into(),
            returns,
        };
        self.filters.push((sig, Arc::new(f)));
        self
    }

    /// Adds every `.html` file below `dir`, named by its path below `dir`
    /// with `/` between folders and without the extension:
    /// `partials/nav.html` becomes `partials/nav`. Files are read in a fixed
    /// order, so errors come out the same way every time.
    ///
    /// ```no_run
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = luminess::Engine::<Page>::builder()
    ///     .directory("themes/default")?
    ///     .directory("themes/shared")?
    ///     .build()?;
    /// # Ok::<(), luminess::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when the folder or a file
    /// cannot be read, or a file is not UTF-8.
    pub fn directory(self, dir: impl AsRef<Path>) -> Result<Self, Error> {
        self.directory_with_extension(dir, "html")
    }

    /// As [`Builder::directory`], for files ending in `.extension` instead.
    ///
    /// # Errors
    ///
    /// As [`Builder::directory`].
    pub fn directory_with_extension(
        mut self,
        dir: impl AsRef<Path>,
        extension: &str,
    ) -> Result<Self, Error> {
        let root = dir.as_ref();
        let suffix = format!(".{}", extension.trim_start_matches('.'));
        let mut pending = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(d) = pending.pop() {
            let entries = fs::read_dir(&d).and_then(Iterator::collect::<Result<Vec<_>, _>>);
            let mut entries = entries.map_err(|e| Error::io(&d, e))?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for e in entries {
                let path = e.path();
                if e.file_type().map_err(|err| Error::io(&path, err))?.is_dir() {
                    pending.push(path);
                    continue;
                }
                let Ok(rel) = path.strip_prefix(root) else {
                    continue;
                };
                let parts: Option<Vec<&str>> =
                    rel.components().map(|c| c.as_os_str().to_str()).collect();
                let Some(parts) = parts else { continue };
                let rel = parts.join("/");
                if let Some(name) = rel.strip_suffix(&suffix) {
                    found.push((name.to_owned(), path));
                }
            }
        }
        found.sort();
        for (name, path) in found {
            let src = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            self.sources.push((name, src, None));
        }
        Ok(self)
    }

    /// Compiles every template into a set.
    fn compile(self) -> Result<Arc<Set>, Error> {
        let (sigs, fns): (Vec<FilterSig>, Vec<Arc<dyn FilterFn>>) =
            self.filters.into_iter().unzip();
        let (names, programs) =
            Self::compile_parts(&self.schema, self.sources, &sigs, self.max_template)?;
        Ok(Arc::new(Set {
            names: Arc::new(names),
            programs: programs.into(),
            filters: fns.into(),
            max_output: self.max_output,
            ids: Box::new([]),
        }))
    }

    fn compile_parts(
        schema: &Schema,
        sources: Vec<(String, String, Option<Schema>)>,
        sigs: &[FilterSig],
        max_template: usize,
    ) -> Result<(Names, Vec<Program>), Error> {
        let sources: Vec<Source> = sources
            .into_iter()
            .map(|(name, src, own)| {
                let ast = if src.len() > max_template {
                    Err(parse::ParseError {
                        at: 0,
                        msg: format!(
                            "the template is {} bytes, past the limit of {max_template}",
                            src.len()
                        ),
                    })
                } else {
                    parse::parse(&src)
                };
                Source {
                    name: name.into_boxed_str(),
                    src: src.into_boxed_str(),
                    schema: own.unwrap_or_else(|| schema.clone()),
                    ast,
                }
            })
            .collect();
        match compile::compile_all(&sources, sigs) {
            Ok(programs) => {
                let mut names =
                    Names::with_capacity_and_hasher(sources.len(), BuildHasherDefault::default());
                for (i, s) in sources.into_iter().enumerate() {
                    // A template whose schema the view type does not answer
                    // is only for `include ... with`: rendered by name, it
                    // would read the wrong fields.
                    if s.schema.reads_as(schema) {
                        let _ = names.insert(s.name, i);
                    }
                }
                Ok((names, programs))
            }
            Err(diags) => Err(Error::compile(
                diags
                    .into_iter()
                    .map(|d| {
                        let s = &sources[d.template];
                        let from = d
                            .from
                            .map(|(t, at)| (&*sources[t].name, &*sources[t].src, at));
                        Diagnostic::new(&s.name, &s.src, d.at, d.msg, from)
                    })
                    .collect(),
            )),
        }
    }

    /// Compiles every template into a new [`Engine`].
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Compile`](crate::ErrorKind::Compile) with every problem
    /// in every template, with names, lines, and columns.
    pub fn build(self) -> Result<Engine<V>, Error> {
        self.compile().map(Engine::from_set)
    }
}

/// A fast hash for template names. Names come from the application, not from
/// visitors, so a keyed hash buys nothing here.
#[derive(Default)]
struct NameHasher(u64);

impl Hasher for NameHasher {
    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut w = [0u8; 8];
            w.copy_from_slice(c);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(w)).wrapping_mul(K);
        }
        for &b in chunks.remainder() {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(K);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type Names = HashMap<Box<str>, usize, BuildHasherDefault<NameHasher>>;

struct Set {
    names: Arc<Names>,
    programs: Arc<[Program]>,
    filters: Arc<[Arc<dyn FilterFn>]>,
    max_output: usize,
    /// For each name registered with the engine (`Engine::register`), its
    /// program in this set, if the set has it. Read only by the `web-net`
    /// integration.
    #[cfg_attr(not(feature = "web-net"), allow(dead_code))]
    ids: Box<[Option<usize>]>,
}

impl Set {
    /// This set with ids for `registry`. Cheap: the programs are shared.
    fn with_ids(&self, registry: &[Box<str>]) -> Self {
        Self {
            names: Arc::clone(&self.names),
            programs: Arc::clone(&self.programs),
            filters: Arc::clone(&self.filters),
            max_output: self.max_output,
            ids: registry
                .iter()
                .map(|n| self.names.get(n).copied())
                .collect(),
        }
    }
}

/// The default for [`Builder::max_template_size`].
const DEFAULT_MAX_TEMPLATE: usize = 1 << 20;

/// One compiled template, for a page or a fragment that is rendered on its
/// own. For a theme of named templates, use an [`Engine`].
///
/// ```
/// use luminess::Template;
///
/// struct Card { title: String, url: String }
/// luminess::view!(Card as "card" { title: text, url: text });
///
/// let card = Template::<Card>::new(r#"<a href="{{ card.url }}">{{ card.title }}</a>"#)?;
/// let html = card.render(&Card { title: "Docs & more".into(), url: "/docs".into() })?;
/// assert_eq!(html, r#"<a href="/docs">Docs &amp; more</a>"#);
/// # Ok::<(), luminess::Error>(())
/// ```
pub struct Template<V> {
    set: Arc<Set>,
    _view: PhantomData<fn(&V)>,
}

impl<V> Clone for Template<V> {
    fn clone(&self) -> Self {
        Self {
            set: Arc::clone(&self.set),
            _view: PhantomData,
        }
    }
}

impl<V> fmt::Debug for Template<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Template").finish_non_exhaustive()
    }
}

/// The name a [`Template`] has inside its one-template set.
const ONLY: &str = "";

impl<V: View + HasSchema> Template<V> {
    /// Compiles `source` against `V`'s schema.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Compile`](crate::ErrorKind::Compile) with every problem
    /// in the template.
    pub fn new(source: impl Into<String>) -> Result<Self, Error> {
        let set = Builder::<V>::new(V::schema())
            .template(ONLY, source)
            .compile()?;
        Ok(Self {
            set,
            _view: PhantomData,
        })
    }
}

impl<V: View> Template<V> {
    /// Renders for `view` into a new string.
    ///
    /// # Errors
    ///
    /// None in practice: a template without an output limit cannot fail to
    /// render. The `Result` keeps every render call the same shape.
    pub fn render(&self, view: &V) -> Result<String, Error> {
        let mut out = Vec::new();
        let _ = self.render_into(view, &mut out)?;
        Ok(into_string(out))
    }

    /// Renders for `view`, appending to `out`, and reports how it went.
    ///
    /// # Errors
    ///
    /// As [`Template::render`].
    pub fn render_into(&self, view: &V, out: &mut Vec<u8>) -> Result<Rendered, Error> {
        self.set.render(ONLY, view, out)
    }
}

/// A theme: named templates compiled against the view type `V`, which a
/// server renders through and can reload while it runs.
///
/// Clones share one template set, and [`Engine::reload_dir`] or
/// [`Engine::replace`] changes it for all of them at once; renders in flight
/// finish on the set they started with.
/// `V` is named because the templates are checked against it: a misspelled
/// field is an error when the engine is built, not a blank page later.
///
/// ```
/// use luminess::Engine;
///
/// struct Page { title: String }
/// luminess::view!(Page as "page" { title: text });
///
/// let engine = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
/// let page = Page { title: "Hi".into() };
/// assert_eq!(engine.render("home", &page)?, "<h1>Hi</h1>");
///
/// // A theme changed: build the new one, then swap it in.
/// engine.replace(Engine::<Page>::builder().template("home", "<h2>{{ page.title }}</h2>").build()?);
/// assert_eq!(engine.render("home", &page)?, "<h2>Hi</h2>");
///
/// // A broken theme fails to build, so the running one stays.
/// assert!(Engine::<Page>::builder().template("home", "{{ page.nope }}").build().is_err());
/// assert_eq!(engine.render("home", &page)?, "<h2>Hi</h2>");
/// # Ok::<(), luminess::Error>(())
/// ```
pub struct Engine<V> {
    shared: Arc<Shared>,
    _view: PhantomData<fn(&V)>,
}

/// What clones of one engine share.
struct Shared {
    /// Distinguishes engines in the per-thread cache.
    id: u64,
    /// Incremented after every swap.
    generation: AtomicU64,
    current: RwLock<Arc<Set>>,
    /// Template names registered for id-based rendering, in id order. Only
    /// grows; touched at startup and on swaps, never by a render.
    registry: Mutex<Vec<Box<str>>>,
}

static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

/// How many engines each thread remembers the current set of.
const CACHE_SLOTS: usize = 4;

/// (engine id, generation, set) for the engines this thread rendered with
/// lately. A render that finds its engine at the current generation uses
/// the set from here and touches no shared memory but one atomic load.
type Cache = [Option<(u64, u64, Arc<Set>)>; CACHE_SLOTS];

thread_local! {
    static CACHE: RefCell<Cache> = const { RefCell::new([const { None }; CACHE_SLOTS]) };
}

impl<V> Clone for Engine<V> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            _view: PhantomData,
        }
    }
}

impl<V> fmt::Debug for Engine<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = self.current_set();
        let mut names: Vec<&str> = set.names.keys().map(|k| &**k).collect();
        names.sort_unstable();
        f.debug_struct("Engine").field("templates", &names).finish()
    }
}

impl<V> Engine<V> {
    fn current_set(&self) -> Arc<Set> {
        Arc::clone(
            &self
                .shared
                .current
                .read()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// Runs `f` with the current set, from this thread's cache when it is
    /// up to date.
    fn with_set<R>(&self, f: impl FnOnce(&Set) -> R) -> R {
        let id = self.shared.id;
        let generation = self.shared.generation.load(Ordering::Acquire);
        CACHE.with(|cache| {
            // A block's renderer may render through another engine on the
            // same thread while the cache is borrowed here; then the inner
            // render takes the slow path.
            if let Ok(c) = cache.try_borrow() {
                if let Some((_, _, set)) = c
                    .iter()
                    .flatten()
                    .find(|(i, g, _)| *i == id && *g == generation)
                {
                    return f(set);
                }
            }
            let set = self.current_set();
            if let Ok(mut c) = cache.try_borrow_mut() {
                let slot = c
                    .iter()
                    .position(|e| e.as_ref().is_some_and(|(i, _, _)| *i == id))
                    .or_else(|| c.iter().position(Option::is_none))
                    .unwrap_or_else(|| {
                        c.rotate_left(1);
                        CACHE_SLOTS - 1
                    });
                c[slot] = Some((id, generation, Arc::clone(&set)));
            }
            f(&set)
        })
    }
}

impl<V: View + HasSchema> Engine<V> {
    /// Loads every `.html` file below `dir` as a template, named by its path
    /// without the extension (`partials/nav.html` is `partials/nav`), and
    /// compiles them against `V`.
    ///
    /// ```no_run
    /// use luminess::Engine;
    ///
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = Engine::<Page>::from_dir("themes/default")?;
    /// let html = engine.render("home", &Page { title: "Hello".into() })?;
    /// # Ok::<(), luminess::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`ErrorKind::Io`](crate::ErrorKind::Io) when the folder or a file
    /// cannot be read; [`ErrorKind::Compile`](crate::ErrorKind::Compile)
    /// with every problem in every template.
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, Error> {
        Self::builder().directory(dir)?.build()
    }

    /// Compiles the templates in `dir` again and swaps them in for every
    /// clone of this engine: `engine.replace(Engine::from_dir(dir)?)`. On
    /// any error the running set stays, so a mistake in a live theme never
    /// takes the site down.
    ///
    /// ```no_run
    /// use luminess::Engine;
    ///
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = Engine::<Page>::from_dir("themes/default")?;
    /// // The theme changed on disk.
    /// if let Err(e) = engine.reload_dir("themes/default") {
    ///     eprintln!("{}", e.report()); // still serving the last good theme
    /// }
    /// # Ok::<(), luminess::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// As [`Engine::from_dir`].
    pub fn reload_dir(&self, dir: impl AsRef<Path>) -> Result<(), Error> {
        self.replace(Self::from_dir(dir)?);
        Ok(())
    }

    /// A builder for templates compiled against `V`'s own schema: for
    /// options, filters, and templates from strings.
    pub fn builder() -> Builder<V> {
        Builder::new(V::schema())
    }
}

impl<V: View> Engine<V> {
    /// A builder for templates compiled against `schema`, for a view type
    /// whose fields are decided at startup (by configuration or a plugin)
    /// rather than by its Rust definition.
    ///
    /// ```
    /// use luminess::{Engine, Schema, Value, View};
    ///
    /// struct Doc(Vec<String>);
    /// impl View for Doc {
    ///     fn field(&self, index: usize) -> Value<'_> {
    ///         self.0.get(index).map_or(Value::Absent, |s| Value::Text(s))
    ///     }
    /// }
    ///
    /// let schema = Schema::build("doc").text("summary").finish();
    /// let engine = Engine::<Doc>::with_schema(schema).template("card", "{{ doc.summary }}").build()?;
    /// assert_eq!(engine.render("card", &Doc(vec!["Shoes".into()]))?, "Shoes");
    /// # Ok::<(), luminess::Error>(())
    /// ```
    pub fn with_schema(schema: Schema) -> Builder<V> {
        Builder::new(schema)
    }

    fn from_set(set: Arc<Set>) -> Self {
        let shared = Shared {
            id: NEXT_ENGINE.fetch_add(1, Ordering::Relaxed),
            generation: AtomicU64::new(0),
            current: RwLock::new(set),
            registry: Mutex::new(Vec::new()),
        };
        Self {
            shared: Arc::new(shared),
            _view: PhantomData,
        }
    }

    /// Swaps in the templates of `with` for every clone of this engine.
    /// Renders in flight finish on the set they started with.
    ///
    /// Building and swapping are two steps, so a server can build every
    /// engine of a new theme, check it, and swap only when all of it is
    /// good. `with` and its clones keep working on their own.
    ///
    /// ```
    /// use luminess::Engine;
    ///
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
    /// let clone = engine.clone();
    ///
    /// let next = Engine::<Page>::builder().template("home", "<h2>{{ page.title }}</h2>").build()?;
    /// assert!(next.contains("home")); // check what the routes need first
    /// engine.replace(next);
    /// assert_eq!(clone.render("home", &Page { title: "Hi".into() })?, "<h2>Hi</h2>");
    /// # Ok::<(), luminess::Error>(())
    /// ```
    pub fn replace(&self, with: Engine<V>) {
        let next = with.current_set();
        drop(with);
        {
            let registry = self
                .shared
                .registry
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let set = if registry.is_empty() {
                next
            } else {
                Arc::new(next.with_ids(&registry))
            };
            let mut cur = self
                .shared
                .current
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            *cur = set;
        }
        // After the new set is in place: a render that sees the new
        // generation is sure to read the new set.
        let _ = self.shared.generation.fetch_add(1, Ordering::Release);
    }

    /// `true` when this engine has a template called `name`: for checking at
    /// startup that a theme has the pages a site routes to.
    ///
    /// ```
    /// use luminess::Engine;
    ///
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = Engine::<Page>::builder().template("home", "{{ page.title }}").build()?;
    /// assert!(engine.contains("home"));
    /// assert!(!engine.contains("about"));
    /// # Ok::<(), luminess::Error>(())
    /// ```
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.with_set(|set| set.names.contains_key(name))
    }

    /// Renders the template called `name` for `view` into a new string.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotFound`](crate::ErrorKind::NotFound) when no template
    /// has that name; [`ErrorKind::TooLarge`](crate::ErrorKind::TooLarge)
    /// past [`Builder::max_output`].
    pub fn render(&self, name: &str, view: &V) -> Result<String, Error> {
        let mut out = Vec::new();
        let _ = self.render_into(name, view, &mut out)?;
        Ok(into_string(out))
    }

    /// Renders the template called `name` for `view`, appending to `out`,
    /// and reports how it went (bytes written, blocks that fell back). Use
    /// it to reuse one buffer across requests.
    ///
    /// ```
    /// use luminess::Engine;
    ///
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let engine = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
    /// let mut buf = Vec::with_capacity(16 * 1024);
    /// for title in ["One", "Two"] {
    ///     buf.clear();
    ///     let report = engine.render_into("home", &Page { title: title.into() }, &mut buf)?;
    ///     assert!(report.failures().is_empty());
    /// }
    /// assert_eq!(buf, b"<h1>Two</h1>");
    /// # Ok::<(), luminess::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// As [`Engine::render`]. Nothing is left in `out` from a failed render.
    pub fn render_into(&self, name: &str, view: &V, out: &mut Vec<u8>) -> Result<Rendered, Error> {
        self.with_set(|set| set.render(name, view, out))
    }
}

/// Every escaper and every literal keeps UTF-8 intact; a block's renderer
/// writes through `Output`, whose methods take `&str`.
fn into_string(out: Vec<u8>) -> String {
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

impl<V: View> Engine<V> {
    /// Registers `name` for rendering by id, and returns its id: the same
    /// for the life of the engine, across reloads. `None` when the current
    /// set has no such template. Meant for startup; each new name copies the
    /// set's id table once.
    #[cfg(feature = "web-net")]
    pub(crate) fn register(&self, name: &str) -> Option<u32> {
        let mut registry = self
            .shared
            .registry
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(i) = registry.iter().position(|n| &**n == name) {
            return u32::try_from(i).ok();
        }
        let mut cur = self
            .shared
            .current
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if !cur.names.contains_key(name) {
            return None;
        }
        let id = u32::try_from(registry.len()).ok()?;
        registry.push(name.into());
        *cur = Arc::new(cur.with_ids(&registry));
        drop(cur);
        let _ = self.shared.generation.fetch_add(1, Ordering::Release);
        Some(id)
    }

    /// Renders the template registered as `id`, writing at most `limit`
    /// bytes (and at most `Builder::max_output`): the caller's budget is
    /// kept while rendering, not checked afterwards.
    #[cfg(feature = "web-net")]
    pub(crate) fn render_id(
        &self,
        id: u32,
        view: &V,
        out: &mut Vec<u8>,
        limit: usize,
    ) -> Result<Rendered, Error> {
        self.with_set(|set| {
            let program = usize::try_from(id)
                .ok()
                .and_then(|i| set.ids.get(i).copied().flatten());
            match program {
                Some(p) => {
                    let limit = set.max_output.min(limit);
                    render::run(&set.programs[p], &set.filters, view, out, limit)
                        .ok_or_else(|| Error::too_large(&self.registered_name(id), limit))
                }
                None => Err(Error::not_found(&self.registered_name(id))),
            }
        })
    }

    #[cfg(feature = "web-net")]
    fn registered_name(&self, id: u32) -> String {
        let registry = self
            .shared
            .registry
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        usize::try_from(id)
            .ok()
            .and_then(|i| registry.get(i))
            .map_or_else(|| format!("#{id}"), ToString::to_string)
    }
}

impl Set {
    fn render(&self, name: &str, view: &dyn View, out: &mut Vec<u8>) -> Result<Rendered, Error> {
        let Some(&i) = self.names.get(name) else {
            return Err(Error::not_found(name));
        };
        render::run(&self.programs[i], &self.filters, view, out, self.max_output)
            .ok_or_else(|| Error::too_large(name, self.max_output))
    }
}
