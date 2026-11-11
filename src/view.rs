//! Typed views: what a template can read, declared once and checked at compile time.
//!
//! A [`Schema`] names the fields a view answers and the [`Kind`] of each. Templates
//! compile against it, so `{{ page.titel }}` is an error at startup rather than an
//! empty string in production. At render time the engine asks the [`View`] for
//! each field by its index, the position the field has in the schema.

use std::{
    fmt,
    sync::{Arc, OnceLock},
};

use crate::output::{BlockError, Output};

/// The position of a field in its schema, as the compiler stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FieldId(u16);

impl FieldId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A value a template can read: implemented by the application's content types.
///
/// `field` is called with the index of a field the schema declared: its
/// position, counting from 0 in the order the fields were declared. An index
/// the view does not know answers [`Value::Absent`]. The compiler resolved
/// every name to its index at startup, so a render never compares names.
///
/// A view whose schema is marked [`SchemaBuilder::renders`] writes its own
/// markup when a template outputs it directly (`{{ page.gallery }}`). That is
/// how a custom block type renders itself.
///
/// ```
/// use luminess::{Schema, Value, View};
///
/// struct Post { title: String, views: u32 }
///
/// impl View for Post {
///     fn field(&self, index: usize) -> Value<'_> {
///         match index {
///             0 => Value::Text(&self.title),
///             1 => Value::Int(i64::from(self.views)),
///             _ => Value::Absent,
///         }
///     }
/// }
///
/// let schema = Schema::build("post").text("title").int("views").finish();
/// assert_eq!(schema.field("views").map(|(index, _)| index), Some(1));
/// ```
pub trait View {
    /// The value of the field at this index.
    fn field(&self, index: usize) -> Value<'_>;

    /// Writes this value's own markup. Called only for views whose schema
    /// renders. The default writes nothing and fails.
    ///
    /// # Errors
    ///
    /// A [`BlockError`] makes the engine remove what this call wrote and write
    /// [`View::fallback`] instead; the rest of the page still renders.
    fn render(&self, out: &mut Output<'_>) -> Result<(), BlockError> {
        let _ = out;
        Err(BlockError::new("this value does not render itself"))
    }

    /// Written in place of [`View::render`] when it fails. The default writes
    /// nothing.
    fn fallback(&self, out: &mut Output<'_>) {
        let _ = out;
    }

    /// Answers a method the schema declared with
    /// [`SchemaBuilder::method`], called with arguments of the declared
    /// kinds. The default answers nothing.
    /// `index` is the method's position in the schema, as for fields.
    fn call<'s>(&'s self, index: usize, args: &[Value<'_>]) -> Answer<'s> {
        let _ = (index, args);
        Answer::Value(Value::Absent)
    }

    /// The plain-text form of a value that renders itself, used where markup
    /// cannot go: an attribute, a URL, a script, or a style. The engine
    /// escapes it for the place. The default has none and fails, which
    /// writes the template's `| fallback(...)` text, or nothing.
    ///
    /// # Errors
    ///
    /// A [`BlockError`] when there is no text form or it cannot be produced.
    fn text(&self, out: &mut String) -> Result<(), BlockError> {
        let _ = out;
        Err(BlockError::new("this value has no text form"))
    }
}

/// What a method ([`View::call`]) or an application filter
/// ([`Builder::filter`](crate::Builder::filter)) answers: a value borrowed
/// from the view, or text built for this call. Both convert with `.into()`.
///
/// The name says what it is for: the answer to a call. A [`Value`] alone
/// cannot hold text built during the call, because it only borrows.
#[derive(Debug)]
pub enum Answer<'a> {
    /// A value, borrowed or scalar.
    Value(Value<'a>),
    /// Text built for this call, escaped like [`Value::Text`].
    Text(String),
}

impl<'a> From<Value<'a>> for Answer<'a> {
    fn from(v: Value<'a>) -> Self {
        Self::Value(v)
    }
}

impl From<String> for Answer<'_> {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

/// An application filter, as registered with
/// [`Builder::filter`](crate::Builder::filter).
pub(crate) trait FilterFn: Send + Sync + 'static {
    /// Applies the filter to `value` with the template's arguments.
    fn apply<'v>(&self, value: Value<'v>, args: &[Value<'_>]) -> Answer<'v>;
}

impl<F> FilterFn for F
where
    F: for<'v, 'w> Fn(Value<'v>, &[Value<'w>]) -> Answer<'v> + Send + Sync + 'static,
{
    fn apply<'v>(&self, value: Value<'v>, args: &[Value<'_>]) -> Answer<'v> {
        self(value, args)
    }
}

/// A sequence a template can loop over.
pub trait List {
    /// The number of items.
    fn len(&self) -> usize;

    /// The item at `index`, or [`Value::Absent`] past the end.
    fn get(&self, index: usize) -> Value<'_>;

    /// `true` when there are no items.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One value read from a [`View`] or a [`List`].
#[derive(Clone, Copy)]
pub enum Value<'a> {
    /// No value: writes nothing and is false in a condition.
    Absent,
    /// A boolean.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A floating-point number.
    Float(f64),
    /// Plain text, escaped for the context it is written in.
    Text(&'a str),
    /// Trusted markup, sanitized before it reached the view. Written as is in
    /// HTML text and escaped anywhere else.
    Html(&'a str),
    /// A value formatted with [`fmt::Display`] and escaped like text: a money
    /// amount or a date without building a string first.
    Display(&'a dyn fmt::Display),
    /// An object with fields of its own.
    View(&'a dyn View),
    /// A sequence of values.
    List(&'a dyn List),
}

impl fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => f.write_str("Absent"),
            Self::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            Self::Int(i) => f.debug_tuple("Int").field(i).finish(),
            Self::Float(x) => f.debug_tuple("Float").field(x).finish(),
            Self::Text(s) => f.debug_tuple("Text").field(s).finish(),
            Self::Html(s) => f.debug_tuple("Html").field(s).finish(),
            Self::Display(d) => f.debug_tuple("Display").field(&d.to_string()).finish(),
            Self::View(_) => f.write_str("View(..)"),
            Self::List(l) => write!(f, "List(len {})", l.len()),
        }
    }
}

/// Turns a Rust value into a [`Value`]. Used by [`view!`](crate::view!) and by
/// the [`List`] implementations for slices and vectors.
pub trait ToValue {
    /// This value as a template value.
    fn to_value(&self) -> Value<'_>;
}

impl ToValue for str {
    fn to_value(&self) -> Value<'_> {
        Value::Text(self)
    }
}

impl ToValue for String {
    fn to_value(&self) -> Value<'_> {
        Value::Text(self)
    }
}

impl ToValue for bool {
    fn to_value(&self) -> Value<'_> {
        Value::Bool(*self)
    }
}

macro_rules! int_to_value {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            fn to_value(&self) -> Value<'_> {
                Value::Int(i64::from(*self))
            }
        }
    )*};
}
int_to_value!(i8, i16, i32, i64, u8, u16, u32);

macro_rules! wide_int_to_value {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            /// Values past `i64::MAX` are written with `Display` instead.
            fn to_value(&self) -> Value<'_> {
                match i64::try_from(*self) {
                    Ok(i) => Value::Int(i),
                    Err(_) => Value::Display(self),
                }
            }
        }
    )*};
}
wide_int_to_value!(u64, usize, isize, i128, u128);

impl ToValue for f64 {
    fn to_value(&self) -> Value<'_> {
        Value::Float(*self)
    }
}

impl ToValue for f32 {
    fn to_value(&self) -> Value<'_> {
        Value::Float(f64::from(*self))
    }
}

impl<T: ToValue + ?Sized> ToValue for &T {
    fn to_value(&self) -> Value<'_> {
        (**self).to_value()
    }
}

impl<T: ToValue + ?Sized> ToValue for Box<T> {
    fn to_value(&self) -> Value<'_> {
        (**self).to_value()
    }
}

impl<T: ToValue + ?Sized> ToValue for Arc<T> {
    fn to_value(&self) -> Value<'_> {
        (**self).to_value()
    }
}

impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value<'_> {
        match self {
            Some(v) => v.to_value(),
            None => Value::Absent,
        }
    }
}

impl<T: ToValue> ToValue for Vec<T> {
    fn to_value(&self) -> Value<'_> {
        Value::List(self)
    }
}

impl<T: ToValue, const N: usize> ToValue for [T; N] {
    fn to_value(&self) -> Value<'_> {
        Value::List(self)
    }
}

impl<T: ToValue> List for Vec<T> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn get(&self, index: usize) -> Value<'_> {
        self.as_slice()
            .get(index)
            .map_or(Value::Absent, ToValue::to_value)
    }
}

impl<T: ToValue, const N: usize> List for [T; N] {
    fn len(&self) -> usize {
        N
    }

    fn get(&self, index: usize) -> Value<'_> {
        self.as_slice()
            .get(index)
            .map_or(Value::Absent, ToValue::to_value)
    }
}

/// `true` when two kinds are the same; objects are compared by schema name.
fn same_kind(a: &Kind, b: &Kind) -> bool {
    match (a, b) {
        (Kind::List(x), Kind::List(y)) => same_kind(x, y),
        (Kind::Object(x), Kind::Object(y)) => x.name() == y.name(),
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

/// The kind of a field, which the compiler checks every use against.
#[derive(Clone, Debug)]
pub enum Kind {
    /// [`Value::Bool`].
    Bool,
    /// [`Value::Int`].
    Int,
    /// [`Value::Float`].
    Float,
    /// [`Value::Text`] or [`Value::Display`].
    Text,
    /// [`Value::Html`]: trusted markup.
    Html,
    /// [`Value::View`] with this schema.
    Object(Schema),
    /// [`Value::List`] whose items have this kind.
    List(Box<Kind>),
}

impl Kind {
    /// An object with the schema of `T`.
    #[must_use]
    pub fn object_of<T: HasSchema + ?Sized>() -> Self {
        Self::Object(Schema::lazy(T::schema))
    }

    /// A list of objects with the schema of `T`.
    #[must_use]
    pub fn list_of<T: HasSchema + ?Sized>() -> Self {
        Self::List(Box::new(Self::object_of::<T>()))
    }

    /// A list whose items have `item` kind.
    #[must_use]
    pub fn list(item: Self) -> Self {
        Self::List(Box::new(item))
    }
}

/// The fields a [`View`] answers: names, kinds, and indexes.
///
/// Cloning is cheap (the schema is shared). A schema made by
/// [`Kind::object_of`] is built on first use, which lets a type refer to
/// itself (a comment with a list of replies).
///
/// ```
/// use luminess::{Kind, Schema};
///
/// let post = Schema::build("post").text("title").text("url").finish();
/// let page = Schema::build("page")
///     .text("title")
///     .html("body")
///     .list("posts", Kind::Object(post))
///     .finish();
/// assert_eq!(page.name(), "page");
/// assert_eq!(page.len(), 3);
/// ```
#[derive(Clone)]
pub struct Schema(Repr);

#[derive(Clone)]
enum Repr {
    Ready(Arc<SchemaInner>),
    /// Built on first use (`Kind::object_of`).
    Lazy(Arc<LazySchema>),
}

struct LazySchema {
    build: fn() -> Schema,
    done: OnceLock<Arc<SchemaInner>>,
}

struct SchemaInner {
    name: Box<str>,
    fields: Vec<(Box<str>, Kind)>,
    /// For each entry of `fields`: its parameters when it is a method.
    params: Vec<Option<Box<[Kind]>>>,
    renders: bool,
}

impl Schema {
    /// A schema built by `build` the first time it is needed.
    pub(crate) fn lazy(build: fn() -> Schema) -> Self {
        Self(Repr::Lazy(Arc::new(LazySchema {
            build,
            done: OnceLock::new(),
        })))
    }

    fn inner(&self) -> &SchemaInner {
        match &self.0 {
            Repr::Ready(a) => a,
            Repr::Lazy(l) => l.done.get_or_init(|| (l.build)().into_arc()),
        }
    }

    fn into_arc(self) -> Arc<SchemaInner> {
        match self.0 {
            Repr::Ready(a) => a,
            Repr::Lazy(l) => Arc::clone(l.done.get_or_init(|| (l.build)().into_arc())),
        }
    }

    /// Starts a schema. `name` is how templates refer to a root value of this
    /// schema: a page view named `page` is read as `{{ page.title }}`.
    pub fn build(name: &str) -> SchemaBuilder {
        SchemaBuilder {
            name: name.into(),
            fields: Vec::new(),
            params: Vec::new(),
            renders: false,
        }
    }

    /// The schema's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.inner().name
    }

    /// The number of fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner().fields.len()
    }

    /// `true` when the schema has no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner().fields.is_empty()
    }

    /// `true` when views of this schema render their own markup.
    #[must_use]
    pub fn renders(&self) -> bool {
        self.inner().renders
    }

    /// `true` when a view answering `root` answers this schema too: every
    /// field and method here has the same name and kind at the same position
    /// in `root`. A template compiled against this schema can then be
    /// rendered with such a view.
    pub(crate) fn reads_as(&self, root: &Schema) -> bool {
        let (a, b) = (self.inner(), root.inner());
        if std::ptr::eq(a, b) {
            return true;
        }
        a.fields.len() <= b.fields.len()
            && a.fields
                .iter()
                .zip(&b.fields)
                .zip(a.params.iter().zip(&b.params))
                .all(|(((na, ka), (nb, kb)), (pa, pb))| {
                    na == nb
                        && same_kind(ka, kb)
                        && match (pa, pb) {
                            (None, None) => true,
                            (Some(x), Some(y)) => {
                                x.len() == y.len()
                                    && x.iter().zip(y.iter()).all(|(p, q)| same_kind(p, q))
                            }
                            _ => false,
                        }
                })
    }

    /// The index and kind of the field called `name`.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<(usize, &Kind)> {
        self.field_id(name).map(|(id, k)| (id.index(), k))
    }

    /// The index, parameter kinds, and return kind of the method called
    /// `name`.
    #[must_use]
    pub fn method(&self, name: &str) -> Option<(usize, &[Kind], &Kind)> {
        self.method_id(name).map(|(id, p, k)| (id.index(), p, k))
    }

    pub(crate) fn field_id(&self, name: &str) -> Option<(FieldId, &Kind)> {
        let inner = self.inner();
        let i = inner.fields.iter().position(|(n, _)| &**n == name)?;
        inner.params[i]
            .is_none()
            .then(|| (FieldId(i as u16), &inner.fields[i].1))
    }

    pub(crate) fn method_id(&self, name: &str) -> Option<(FieldId, &[Kind], &Kind)> {
        let inner = self.inner();
        let i = inner.fields.iter().position(|(n, _)| &**n == name)?;
        let params = inner.params[i].as_deref()?;
        Some((FieldId(i as u16), params, &inner.fields[i].1))
    }

    /// The field and method names, in index order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inner().fields.iter().map(|(n, _)| &**n)
    }
}

impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema")
            .field("name", &self.inner().name)
            .field("fields", &self.inner().fields)
            .field("renders", &self.inner().renders)
            .finish()
    }
}

/// Builds a [`Schema`]. Each field gets the next index, starting at `0`.
#[derive(Debug)]
#[must_use]
pub struct SchemaBuilder {
    name: Box<str>,
    fields: Vec<(Box<str>, Kind)>,
    params: Vec<Option<Box<[Kind]>>>,
    renders: bool,
}

impl SchemaBuilder {
    /// Adds a field of any kind.
    ///
    /// # Panics
    ///
    /// When `name` is declared twice or the schema passes 65,535 fields: both
    /// are mistakes in the program, found the first time it starts.
    pub fn field(mut self, name: &str, kind: Kind) -> Self {
        assert!(
            !self.fields.iter().any(|(n, _)| &**n == name),
            "luminess: field `{name}` declared twice in schema `{}`",
            self.name
        );
        assert!(
            self.fields.len() < usize::from(u16::MAX),
            "luminess: too many fields"
        );
        self.fields.push((name.into(), kind));
        self.params.push(None);
        self
    }

    /// Adds a method: a value computed from arguments, such as
    /// `variations.options("size")`. A template calls it with arguments of
    /// these kinds, checked at compile time, and [`View::call`] answers it.
    /// A method without parameters is called with `()`.
    ///
    /// # Panics
    ///
    /// As [`SchemaBuilder::field`].
    pub fn method(mut self, name: &str, params: &[Kind], returns: Kind) -> Self {
        self = self.field(name, returns);
        if let Some(last) = self.params.last_mut() {
            *last = Some(params.into());
        }
        self
    }

    /// Adds a text field.
    pub fn text(self, name: &str) -> Self {
        self.field(name, Kind::Text)
    }

    /// Adds a trusted-markup field.
    pub fn html(self, name: &str) -> Self {
        self.field(name, Kind::Html)
    }

    /// Adds an integer field.
    pub fn int(self, name: &str) -> Self {
        self.field(name, Kind::Int)
    }

    /// Adds a floating-point field.
    pub fn float(self, name: &str) -> Self {
        self.field(name, Kind::Float)
    }

    /// Adds a boolean field.
    pub fn bool(self, name: &str) -> Self {
        self.field(name, Kind::Bool)
    }

    /// Adds an object field with its own schema.
    pub fn object(self, name: &str, schema: Schema) -> Self {
        self.field(name, Kind::Object(schema))
    }

    /// Adds a list field whose items have `item` kind.
    pub fn list(self, name: &str, item: Kind) -> Self {
        self.field(name, Kind::List(Box::new(item)))
    }

    /// Marks views of this schema as rendering their own markup
    /// ([`View::render`]), so a template can output them directly.
    pub fn renders(mut self) -> Self {
        self.renders = true;
        self
    }

    /// The finished schema.
    #[must_use]
    pub fn finish(self) -> Schema {
        Schema(Repr::Ready(Arc::new(SchemaInner {
            name: self.name,
            fields: self.fields,
            params: self.params,
            renders: self.renders,
        })))
    }
}

/// A Rust type whose schema is fixed: what `Engine::<V>` needs to compile
/// templates without being handed a schema. [`view!`](crate::view!)
/// implements it; implement it by hand only for views written by hand.
pub trait HasSchema {
    /// The schema of this type.
    fn schema() -> Schema;
}

#[doc(hidden)]
#[must_use]
pub fn __html(v: Value<'_>) -> Value<'_> {
    match v {
        Value::Text(s) => Value::Html(s),
        other => other,
    }
}

/// Implements [`View`], [`HasSchema`], and [`ToValue`] for a struct from a list of
/// its fields and their kinds.
///
/// Kinds: `text`, `html`, `int`, `float`, `bool`, `object(Type)`, and
/// `list(kind)`. A field of type `Option<T>` is absent when it is `None`.
///
/// ```
/// use luminess::Engine;
///
/// struct Post { title: String, url: String }
/// struct Page { title: String, posts: Vec<Post>, subtitle: Option<String> }
///
/// luminess::view!(Post as "post" { title: text, url: text });
/// luminess::view!(Page as "page" { title: text, posts: list(object(Post)), subtitle: text });
///
/// let engine = Engine::<Page>::builder()
///     .template("home", "<h1>{{ page.title }}</h1>{% for p in page.posts %}<a href=\"{{ p.url }}\">{{ p.title }}</a>{% endfor %}")
///     .build()
///     .unwrap();
/// let page = Page {
///     title: "News & notes".into(),
///     posts: vec![Post { title: "First".into(), url: "/first".into() }],
///     subtitle: None,
/// };
/// assert_eq!(
///     engine.render("home", &page).unwrap(),
///     "<h1>News &amp; notes</h1><a href=\"/first\">First</a>"
/// );
/// ```
#[macro_export]
macro_rules! view {
    ($ty:ty as $name:literal { $($field:ident : $kind:ident $(( $($arg:tt)* ))?),* $(,)? }) => {
        impl $crate::HasSchema for $ty {
            fn schema() -> $crate::Schema {
                static SCHEMA: ::std::sync::OnceLock<$crate::Schema> = ::std::sync::OnceLock::new();
                SCHEMA
                    .get_or_init(|| {
                        $crate::Schema::build($name)
                            $(.field(stringify!($field), $crate::view!(@kind $kind $(( $($arg)* ))?)))*
                            .finish()
                    })
                    .clone()
            }
        }

        impl $crate::View for $ty {
            fn field(&self, index: usize) -> $crate::Value<'_> {
                // One constant per field, numbered by an enum in declaration
                // order, so the match below is a jump table.
                mod __ids {
                    #![allow(non_upper_case_globals, non_camel_case_types, dead_code)]
                    #[repr(usize)]
                    enum Field { $($field),* }
                    $(pub const $field: usize = Field::$field as usize;)*
                }
                match index {
                    $(__ids::$field => $crate::view!(@value self.$field, $kind),)*
                    _ => $crate::Value::Absent,
                }
            }
        }

        impl $crate::ToValue for $ty {
            fn to_value(&self) -> $crate::Value<'_> {
                $crate::Value::View(self)
            }
        }
    };

    (@kind text) => { $crate::Kind::Text };
    (@kind html) => { $crate::Kind::Html };
    (@kind int) => { $crate::Kind::Int };
    (@kind float) => { $crate::Kind::Float };
    (@kind bool) => { $crate::Kind::Bool };
    (@kind object($t:ty)) => { $crate::Kind::object_of::<$t>() };
    (@kind list($k:ident $(( $($arg:tt)* ))?)) => {
        $crate::Kind::list($crate::view!(@kind $k $(( $($arg)* ))?))
    };

    (@value $e:expr, html) => { $crate::__html($crate::ToValue::to_value(&$e)) };
    (@value $e:expr, $other:ident) => { $crate::ToValue::to_value(&$e) };
}
