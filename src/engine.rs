//! The template set and the live handle that renders it.

use std::{
    collections::HashMap,
    fmt, fs, io,
    marker::PhantomData,
    path::Path,
    sync::{Arc, PoisonError, RwLock},
};

use crate::{
    compile::{self, Program, Source},
    error::{CompileError, Diagnostic, RenderError},
    parse,
    render::{self, Rendered},
    view::{Schema, Shaped, View},
};

/// Collects template sources, then compiles them all at once.
///
/// ```
/// use luminess::{Engine, Schema, Value, View, FieldId};
///
/// struct Page { title: String }
/// impl View for Page {
///     fn field(&self, id: FieldId) -> Value<'_> {
///         match id.index() { 0 => Value::Text(&self.title), _ => Value::Absent }
///     }
/// }
///
/// let schema = Schema::build("page").text("title").finish();
/// let err = Engine::<Page>::builder_with(schema)
///     .template("home", "<h1>{{ page.titel }}</h1>")
///     .build()
///     .unwrap_err();
/// assert_eq!(
///     err.diagnostics()[0].to_string(),
///     "home:1:13: `page` (page) has no field `titel`, did you mean `title`?"
/// );
/// ```
#[must_use]
pub struct Builder<V> {
    schema: Schema,
    sources: Vec<(String, String, Option<Schema>)>,
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
            .finish()
    }
}

impl<V: View> Builder<V> {
    /// A builder whose templates compile against `schema`.
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            sources: Vec::new(),
            _view: PhantomData,
        }
    }

    /// Adds a template that compiles against the builder's schema.
    pub fn template(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        self.sources.push((name.into(), source.into(), None));
        self
    }

    /// Adds a template that compiles against its own schema: a product page
    /// and a blog post rendered from the same content type, each with its own
    /// fields.
    pub fn template_with(
        mut self,
        name: impl Into<String>,
        source: impl Into<String>,
        schema: Schema,
    ) -> Self {
        self.sources
            .push((name.into(), source.into(), Some(schema)));
        self
    }

    /// Adds every file under `dir` whose name ends in `.extension`, named by
    /// its path below `dir` with `/` between folders and without the
    /// extension: `partials/nav.html` becomes `partials/nav`. Files are read
    /// in a fixed order, so errors come out the same way every time.
    ///
    /// # Errors
    ///
    /// Any error reading the folder or a file, including a file that is not
    /// UTF-8.
    pub fn directory(mut self, dir: impl AsRef<Path>, extension: &str) -> io::Result<Self> {
        let root = dir.as_ref();
        let suffix = format!(".{}", extension.trim_start_matches('.'));
        let mut pending = vec![root.to_path_buf()];
        let mut found = Vec::new();
        while let Some(d) = pending.pop() {
            let mut entries = fs::read_dir(&d)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for e in entries {
                let path = e.path();
                if e.file_type()?.is_dir() {
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
            let src = fs::read_to_string(&path)?;
            self.sources.push((name, src, None));
        }
        Ok(self)
    }

    /// Compiles every template.
    ///
    /// # Errors
    ///
    /// Every problem in every template, with names, lines, and columns.
    pub fn compile(self) -> Result<Templates<V>, CompileError> {
        let sources: Vec<Source> = self
            .sources
            .into_iter()
            .map(|(name, src, schema)| {
                let ast = parse::parse(&src);
                Source {
                    name: name.into_boxed_str(),
                    src: src.into_boxed_str(),
                    schema: schema.unwrap_or_else(|| self.schema.clone()),
                    ast,
                }
            })
            .collect();
        match compile::compile_all(&sources) {
            Ok(programs) => {
                let mut names = HashMap::with_capacity(sources.len());
                for (i, s) in sources.into_iter().enumerate() {
                    let _ = names.insert(s.name, i);
                }
                Ok(Templates {
                    set: Arc::new(Set { names, programs }),
                    _view: PhantomData,
                })
            }
            Err(diags) => Err(CompileError::new(
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
    /// As [`Builder::compile`].
    pub fn build(self) -> Result<Engine<V>, CompileError> {
        self.compile().map(Engine::new)
    }
}

struct Set {
    names: HashMap<Box<str>, usize>,
    programs: Vec<Program>,
}

/// A compiled template set. Cloning is cheap, and a clone keeps rendering the
/// same templates while an [`Engine`] swaps in new ones.
pub struct Templates<V> {
    set: Arc<Set>,
    _view: PhantomData<fn(&V)>,
}

impl<V> Clone for Templates<V> {
    fn clone(&self) -> Self {
        Self {
            set: Arc::clone(&self.set),
            _view: PhantomData,
        }
    }
}

impl<V> fmt::Debug for Templates<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<&str> = self.set.names.keys().map(|k| &**k).collect();
        names.sort_unstable();
        f.debug_struct("Templates").field("names", &names).finish()
    }
}

impl<V: View> Templates<V> {
    /// Renders the template called `name` for `view`, appending to `out`.
    ///
    /// # Errors
    ///
    /// [`RenderErrorKind::NotFound`](crate::RenderErrorKind::NotFound) when no
    /// template has that name. Nothing is written then.
    pub fn render(&self, name: &str, view: &V, out: &mut Vec<u8>) -> Result<Rendered, RenderError> {
        match self.set.names.get(name) {
            Some(&i) => Ok(render::run(&self.set.programs[i], view, out)),
            None => Err(RenderError::not_found(name)),
        }
    }

    /// Renders into a new string.
    ///
    /// # Errors
    ///
    /// As [`Templates::render`].
    pub fn render_to_string(&self, name: &str, view: &V) -> Result<String, RenderError> {
        let mut out = Vec::new();
        let _ = self.render(name, view, &mut out)?;
        // Every escaper and every literal keeps UTF-8 intact; a block's
        // renderer writes through `Output`, whose methods take `&str`.
        Ok(String::from_utf8(out)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }

    /// `true` when a template has this name.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.set.names.contains_key(name)
    }

    /// The template names, in no particular order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.set.names.keys().map(|k| &**k)
    }

    /// The number of templates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.set.programs.len()
    }

    /// `true` when the set has no templates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.programs.is_empty()
    }
}

/// The live handle a server renders through. Clones share one template set,
/// and [`Engine::reload`] replaces it for all of them at once: renders in
/// flight finish on the set they started with.
///
/// ```
/// use luminess::Engine;
///
/// struct Page { title: String }
/// luminess::view!(Page as "page" { title: text });
///
/// let engine = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
/// let page = Page { title: "Hi".into() };
/// assert_eq!(engine.render_to_string("home", &page)?, "<h1>Hi</h1>");
///
/// // A theme changed: compile the new set and swap it in.
/// engine.reload(Engine::<Page>::builder().template("home", "<h2>{{ page.title }}</h2>"))?;
/// assert_eq!(engine.render_to_string("home", &page)?, "<h2>Hi</h2>");
///
/// // A broken theme is refused and the running one stays.
/// assert!(engine.reload(Engine::<Page>::builder().template("home", "{{ page.nope }}")).is_err());
/// assert_eq!(engine.render_to_string("home", &page)?, "<h2>Hi</h2>");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Engine<V> {
    current: Arc<RwLock<Templates<V>>>,
}

impl<V> Clone for Engine<V> {
    fn clone(&self) -> Self {
        Self {
            current: Arc::clone(&self.current),
        }
    }
}

impl<V> fmt::Debug for Engine<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = self.current.read().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Engine").field("templates", &*t).finish()
    }
}

impl<V: View + Shaped> Engine<V> {
    /// A builder whose templates compile against `V`'s own schema.
    pub fn builder() -> Builder<V> {
        Builder::new(V::schema())
    }
}

impl<V: View> Engine<V> {
    /// A builder whose templates compile against `schema`.
    pub fn builder_with(schema: Schema) -> Builder<V> {
        Builder::new(schema)
    }

    /// An engine serving `templates`.
    #[must_use]
    pub fn new(templates: Templates<V>) -> Self {
        Self {
            current: Arc::new(RwLock::new(templates)),
        }
    }

    /// The template set renders use right now.
    #[must_use]
    pub fn templates(&self) -> Templates<V> {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replaces the template set and returns the old one.
    pub fn swap(&self, templates: Templates<V>) -> Templates<V> {
        let mut cur = self.current.write().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *cur, templates)
    }

    /// Compiles `builder` and, when every template compiles, swaps it in.
    ///
    /// # Errors
    ///
    /// The compile errors. The running set stays in place.
    pub fn reload(&self, builder: Builder<V>) -> Result<(), CompileError> {
        let next = builder.compile()?;
        let _ = self.swap(next);
        Ok(())
    }

    /// Renders the template called `name` for `view`, appending to `out`.
    ///
    /// # Errors
    ///
    /// As [`Templates::render`].
    pub fn render(&self, name: &str, view: &V, out: &mut Vec<u8>) -> Result<Rendered, RenderError> {
        self.templates().render(name, view, out)
    }

    /// Renders into a new string.
    ///
    /// # Errors
    ///
    /// As [`Templates::render`].
    pub fn render_to_string(&self, name: &str, view: &V) -> Result<String, RenderError> {
        self.templates().render_to_string(name, view)
    }
}
