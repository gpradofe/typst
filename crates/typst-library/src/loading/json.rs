use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;

use ecow::{EcoVec, eco_format};
use indexmap::IndexMap;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::de::{
    DeserializeSeed, Deserializer, Error as DeError, MapAccess, SeqAccess, Visitor,
};
use typst_syntax::Spanned;

use crate::diag::{At, LineCol, LoadError, LoadedWithin, SourceResult, bail};
use crate::engine::Engine;
use crate::foundations::{Array, Datetime, Dict, IntoValue, Str, Value, func, scope};
use crate::loading::{DataSource, Load, Readable};

/// Deduplicates equal strings encountered while deserializing JSON so that the
/// resulting value tree shares a single [`Str`] allocation per distinct string.
///
/// Data-heavy JSON repeats the same dictionary keys on every record (e.g. a
/// report with millions of rows repeats its ~10 column keys on each row) and
/// reuses a small set of categorical values (currencies, dates, names). Without
/// interning, each repetition is a fresh heap allocation; interning collapses
/// them to one, which can cut the parsed tree's peak memory by gigabytes for
/// large inputs. Unique strings (e.g. per-row identifiers) cost one extra hash
/// lookup and are stored once, as before.
#[derive(Default)]
struct Interner {
    strings: FxHashMap<Box<str>, Str>,
}

impl Interner {
    fn intern(&mut self, s: &str) -> Str {
        if let Some(existing) = self.strings.get(s) {
            existing.clone()
        } else {
            let value: Str = s.into();
            self.strings.insert(Box::from(s), value.clone());
            value
        }
    }
}

/// A [`DeserializeSeed`] threading an [`Interner`] through the value tree.
struct InterningSeed<'a>(&'a RefCell<Interner>);

impl<'de> DeserializeSeed<'de> for InterningSeed<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        de.deserialize_any(InterningVisitor(self.0))
    }
}

/// Mirrors typst's standard `ValueVisitor` for the value types JSON can
/// produce, but interns every string (dict key or string value). Numeric and
/// boolean handling is identical to the standard visitor, so the resulting
/// `Value` is byte-for-byte equivalent — only string allocations are shared.
struct InterningVisitor<'a>(&'a RefCell<Interner>);

impl<'de> Visitor<'de> for InterningVisitor<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a Typst value")
    }

    fn visit_bool<E: DeError>(self, v: bool) -> Result<Value, E> {
        Ok(v.into_value())
    }
    fn visit_i64<E: DeError>(self, v: i64) -> Result<Value, E> {
        Ok(v.into_value())
    }
    fn visit_i128<E: DeError>(self, v: i128) -> Result<Value, E> {
        Ok(v.into_value())
    }
    fn visit_u64<E: DeError>(self, v: u64) -> Result<Value, E> {
        Ok(v.into_value())
    }
    fn visit_u128<E: DeError>(self, v: u128) -> Result<Value, E> {
        Ok(v.into_value())
    }
    fn visit_f64<E: DeError>(self, v: f64) -> Result<Value, E> {
        Ok(v.into_value())
    }

    fn visit_str<E: DeError>(self, v: &str) -> Result<Value, E> {
        Ok(Value::Str(self.0.borrow_mut().intern(v)))
    }
    fn visit_borrowed_str<E: DeError>(self, v: &'de str) -> Result<Value, E> {
        Ok(Value::Str(self.0.borrow_mut().intern(v)))
    }
    fn visit_string<E: DeError>(self, v: String) -> Result<Value, E> {
        Ok(Value::Str(self.0.borrow_mut().intern(&v)))
    }

    fn visit_none<E: DeError>(self) -> Result<Value, E> {
        Ok(Value::None)
    }
    fn visit_unit<E: DeError>(self) -> Result<Value, E> {
        Ok(Value::None)
    }
    fn visit_some<D: Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        de.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = EcoVec::new();
        while let Some(value) = seq.next_element_seed(InterningSeed(self.0))? {
            items.push(value);
        }
        Ok(Value::Array(Array::from(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut dict = IndexMap::<Str, Value, FxBuildHasher>::default();
        while let Some(key) = map.next_key::<Cow<str>>()? {
            let key = self.0.borrow_mut().intern(&key);
            let value = map.next_value_seed(InterningSeed(self.0))?;
            dict.insert(key, value);
        }
        // Preserve the standard visitor's TOML-datetime detection so output is
        // identical to the non-interning path.
        let dict = Dict::from(dict);
        Ok(match Datetime::from_toml_dict(&dict) {
            None => dict.into_value(),
            Some(datetime) => datetime.into_value(),
        })
    }
}

/// Reads structured data from a JSON file.
///
/// The file must contain a valid JSON value, such as object or array. The JSON
/// values will be converted into corresponding Typst values as listed in the
/// @json:conversion[table below].
///
/// The function returns a dictionary, an array or, depending on the JSON file,
/// another JSON data type.
///
/// The JSON files in the example contain objects with the keys `temperature`,
/// `unit`, and `weather`.
///
/// = Example <example>
/// ```example
/// #let forecast(day) = block[
///   #box(square(
///     width: 2cm,
///     inset: 8pt,
///     fill: if day.weather == "sunny" {
///       yellow
///     } else {
///       aqua
///     },
///     align(
///       bottom + right,
///       strong(day.weather),
///     ),
///   ))
///   #h(6pt)
///   #set text(22pt, baseline: -8pt)
///   #day.temperature °#day.unit
/// ]
///
/// #forecast(json("monday.json"))
/// #forecast(json("tuesday.json"))
/// ```
///
/// = #short-or-long[Conversion][Conversion details] <conversion>
/// #docs-table(
///   table.header[JSON value][Converted into Typst],
///
///   [`null`],
///   [`{none}`],
///
///   [bool],
///   [@bool],
///
///   [number],
///   [@float or @int],
///
///   [string],
///   [@str],
///
///   [array],
///   [@array],
///
///   [object],
///   [@dictionary],
/// )
///
/// #docs-table(
///   table.header[Typst value][Converted into JSON],
///
///   [types that can be converted from JSON],
///   [corresponding JSON value],
///
///   [@bytes],
///   [string via @repr],
///
///   [@symbol],
///   [string],
///
///   [@content],
///   [an object describing the content],
///
///   [other types (@length, etc.)],
///   [string via @repr],
/// )
///
/// == Notes <notes>
/// - In most cases, JSON numbers will be converted to floats or integers
///   depending on whether they are whole numbers. However, be aware that
///   integers larger than 2#super[63]-1 or smaller than -2#super[63] will be
///   converted to floating-point numbers, which may result in an approximative
///   value.
///
/// - Bytes are not encoded as JSON arrays for performance and readability
///   reasons. Consider using @cbor.encode for binary data.
///
/// - The `repr` function is @repr:debugging-only[for debugging purposes only],
///   and its output is not guaranteed to be stable across Typst versions.
#[func(scope, title = "JSON")]
pub fn json(
    engine: &mut Engine,
    /// A path to a JSON file or raw JSON bytes.
    source: Spanned<DataSource>,
) -> SourceResult<Value> {
    let loaded = source.load(engine.world)?;
    let raw = loaded.data.as_slice();
    // If the file starts with a UTF-8 Byte Order Mark (BOM), return a
    // friendly error message.
    if raw.starts_with(b"\xef\xbb\xbf") {
        bail!(
            LoadError::new(
                LineCol::one_based(1, 1),
                "failed to parse JSON",
                "unexpected Byte Order Mark",
            )
            .within(&loaded)
            .with_hint("JSON requires UTF-8 without a BOM")
        );
    }

    // Deserialize directly into a `Value`, interning repeated strings (dict
    // keys and categorical string values) so large data files don't allocate a
    // fresh `Str` for every repetition. This is byte-for-byte equivalent to the
    // standard `serde_json::from_slice::<Value>(raw)` path — only string
    // allocations are shared — but cuts peak memory dramatically for inputs
    // with heavy key/value repetition (e.g. million-row reports).
    let interner = RefCell::new(Interner::default());
    let mut de = serde_json::Deserializer::from_slice(raw);
    InterningSeed(&interner)
        .deserialize(&mut de)
        .and_then(|value| de.end().map(|()| value))
        .map_err(|err| {
            let pos = LineCol::one_based(err.line(), err.column());
            LoadError::new(pos, "failed to parse JSON", err)
        })
        .within(&loaded)
}

#[scope]
impl json {
    /// Reads structured data from a JSON string/bytes.
    #[func(title = "Decode JSON")]
    #[deprecated(
        message = "`json.decode` is deprecated, directly pass bytes to `json` instead",
        until = "0.15.0"
    )]
    pub fn decode(
        engine: &mut Engine,
        /// JSON data.
        data: Spanned<Readable>,
    ) -> SourceResult<Value> {
        json(engine, data.map(Readable::into_source))
    }

    /// Encodes structured data into a JSON string.
    #[func(title = "Encode JSON")]
    pub fn encode(
        /// Value to be encoded.
        value: Spanned<Value>,
        /// Whether to pretty print the JSON with newlines and indentation.
        #[named]
        #[default(true)]
        pretty: bool,
    ) -> SourceResult<Str> {
        let Spanned { v: value, span } = value;
        if pretty {
            serde_json::to_string_pretty(&value)
        } else {
            serde_json::to_string(&value)
        }
        .map(|v| v.into())
        .map_err(|err| eco_format!("failed to encode value as JSON ({err})"))
        .at(span)
    }
}
