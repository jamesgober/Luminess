//! Typed views: what a template can read, declared once and checked at compile time.
//!
//! A [`Schema`] names the fields a view answers and the [`Kind`] of each. Templates
//! compile against it, so `{{ page.titel }}` is an error at startup rather than an
//! empty string in production. At render time the engine asks the [`View`] for
//! each field by its [`FieldId`], the position the field has in the schema.

use std::{fmt, sync::Arc};

use crate::output::{Failed, Output};

/// The position of a field in its [`Schema`]: the first field declared is `0`.
///
/// The compiler resolves every field name in a template to its id once, so a
/// render never compares names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(u16);

impl FieldId {
    /// The id of the field declared at `index`.
    #[must_use]
    pub const fn new(index: u16) -> Self {
        Self(index)
    }

    /// The position of the field in its schema.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A value a template can read: implemented by the application's content types.
///
/// `field` is called with the ids of the fields the schema declared. An id the
/// view does not know answers [`Value::Absent`].
///
/// A view whose schema is marked [`SchemaBuilder::renders`] writes its own
/// markup when a template outputs it directly (`{{ page.gallery }}`). That is
/// how a custom block type renders itself.
///
/// ```
/// use luminess::{FieldId, Schema, Value, View};
///
/// struct Post { title: String, views: u32 }
///
/// impl View for Post {
///     fn field(&self, id: FieldId) -> Value<'_> {
///         match id.index() {
///             0 => Value::Text(&self.title),
///             1 => Value::Int(i64::from(self.views)),
///             _ => Value::Absent,
///         }
///     }
/// }
///
/// let schema = Schema::build("post").text("title").int("views").finish();
/// assert_eq!(schema.field("views").map(|(id, _)| id.index()), Some(1));
/// ```
pub trait View {
    /// The value of the field with this id.
    fn field(&self, id: FieldId) -> Value<'_>;

    /// Writes this value's own markup. Called only for views whose schema
    /// renders. The default writes nothing and fails.
    ///
    /// # Errors
    ///
    /// A [`Failed`] makes the engine remove what this call wrote and write
    /// [`View::fallback`] instead; the rest of the page still renders.
    fn render(&self, out: &mut Output<'_>) -> Result<(), Failed> {
        let _ = out;
        Err(Failed::new("this value does not render itself"))
    }

    /// Written in place of [`View::render`] when it fails. The default writes
    /// nothing.
    fn fallback(&self, out: &mut Output<'_>) {
        let _ = out;
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
    Object(SchemaRef),
    /// [`Value::List`] whose items have this kind.
    List(Box<Kind>),
}

impl Kind {
    /// An object with the schema of `T`.
    #[must_use]
    pub fn object_of<T: Shaped + ?Sized>() -> Self {
        Self::Object(SchemaRef::Lazy(T::schema))
    }

    /// A list of objects with the schema of `T`.
    #[must_use]
    pub fn list_of<T: Shaped + ?Sized>() -> Self {
        Self::List(Box::new(Self::object_of::<T>()))
    }

    /// A list whose items have `item` kind.
    #[must_use]
    pub fn list(item: Self) -> Self {
        Self::List(Box::new(item))
    }
}

/// A schema, either built already or built on first use. The lazy form lets a
/// type refer to itself (a comment with a list of replies).
#[derive(Clone)]
pub enum SchemaRef {
    /// A schema built already.
    Ready(Schema),
    /// A function that returns the schema, called when the compiler needs it.
    Lazy(fn() -> Schema),
}

impl SchemaRef {
    /// The schema.
    #[must_use]
    pub fn get(&self) -> Schema {
        match self {
            Self::Ready(s) => s.clone(),
            Self::Lazy(f) => f(),
        }
    }
}

impl fmt::Debug for SchemaRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready(s) => write!(f, "Ready({})", s.name()),
            Self::Lazy(_) => f.write_str("Lazy(..)"),
        }
    }
}

impl From<Schema> for SchemaRef {
    fn from(s: Schema) -> Self {
        Self::Ready(s)
    }
}

impl From<&Schema> for SchemaRef {
    fn from(s: &Schema) -> Self {
        Self::Ready(s.clone())
    }
}

/// The fields a [`View`] answers: names, kinds, and ids.
///
/// Cloning is cheap (the schema is shared).
///
/// ```
/// use luminess::{Kind, Schema};
///
/// let post = Schema::build("post").text("title").text("url").finish();
/// let page = Schema::build("page")
///     .text("title")
///     .html("body")
///     .list("posts", Kind::Object(post.into()))
///     .finish();
/// assert_eq!(page.name(), "page");
/// assert_eq!(page.len(), 3);
/// ```
#[derive(Clone)]
pub struct Schema(Arc<SchemaInner>);

struct SchemaInner {
    name: Box<str>,
    fields: Vec<(Box<str>, Kind)>,
    renders: bool,
}

impl Schema {
    /// Starts a schema. `name` is how templates refer to a root value of this
    /// schema: a page view named `page` is read as `{{ page.title }}`.
    pub fn build(name: &str) -> SchemaBuilder {
        SchemaBuilder {
            name: name.into(),
            fields: Vec::new(),
            renders: false,
        }
    }

    /// The schema's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0.name
    }

    /// The number of fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.fields.len()
    }

    /// `true` when the schema has no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.fields.is_empty()
    }

    /// `true` when views of this schema render their own markup.
    #[must_use]
    pub fn renders(&self) -> bool {
        self.0.renders
    }

    /// The id and kind of the field called `name`.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<(FieldId, &Kind)> {
        self.0
            .fields
            .iter()
            .position(|(n, _)| &**n == name)
            .map(|i| (FieldId(i as u16), &self.0.fields[i].1))
    }

    /// The field names, in id order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.fields.iter().map(|(n, _)| &**n)
    }
}

impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema")
            .field("name", &self.0.name)
            .field("fields", &self.0.fields)
            .field("renders", &self.0.renders)
            .finish()
    }
}

/// Builds a [`Schema`]. Each field gets the next id, starting at `0`.
#[derive(Debug)]
#[must_use]
pub struct SchemaBuilder {
    name: Box<str>,
    fields: Vec<(Box<str>, Kind)>,
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
    pub fn object(self, name: &str, schema: impl Into<SchemaRef>) -> Self {
        self.field(name, Kind::Object(schema.into()))
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
        Schema(Arc::new(SchemaInner {
            name: self.name,
            fields: self.fields,
            renders: self.renders,
        }))
    }
}

/// A Rust type with a fixed schema. [`view!`](crate::view!) implements it, and
/// [`Engine::builder`](crate::Engine::builder) uses it.
pub trait Shaped {
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

/// Implements [`View`], [`Shaped`], and [`ToValue`] for a struct from a list of
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
///     engine.render_to_string("home", &page).unwrap(),
///     "<h1>News &amp; notes</h1><a href=\"/first\">First</a>"
/// );
/// ```
#[macro_export]
macro_rules! view {
    ($ty:ty as $name:literal { $($field:ident : $kind:ident $(( $($arg:tt)* ))?),* $(,)? }) => {
        impl $crate::Shaped for $ty {
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
            #[allow(unused_assignments, unused_mut, unused_variables)]
            fn field(&self, id: $crate::FieldId) -> $crate::Value<'_> {
                let i = id.index();
                let mut n = 0usize;
                $(
                    if i == n {
                        return $crate::view!(@value self.$field, $kind);
                    }
                    n += 1;
                )*
                $crate::Value::Absent
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

    (@value $e:expr, html) => { $crate::view::__html($crate::ToValue::to_value(&$e)) };
    (@value $e:expr, $other:ident) => { $crate::ToValue::to_value(&$e) };
}
