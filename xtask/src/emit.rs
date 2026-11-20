//! Turns the schema model into Rust source: types, readers, writers and stream routes.
//!
//! The mapping is fixed and small:
//!
//! - a concrete complex type becomes a struct holding its inherited and its own fields;
//! - a single element or attribute is `Option<T>`, a repeated element `Vec<T>`;
//! - a type with subtypes that is used as a field type becomes an enum chosen by `xsi:type`;
//! - a subtype that only fixes an attribute reads as its base type;
//! - an enumeration becomes an enum with an `Extended(String)` variant;
//! - other simple types become aliases of a primitive.
//!
//! Only names and structure are emitted, never documentation text from the schemas.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

#[path = "emit_json.rs"]
mod json;

use crate::schema::{Complex, JsonDefs, Schema, TypeRef};

type Key = (String, String);

/// Per field (attribute or not, declaring namespace, name): declared type, least and most occurrences.
type Declared = BTreeMap<(bool, String, String), (TypeRef, u32, u32)>;

/// Per record type (polymorphic or not, and its key): publication type key and element keys.
type Routes = BTreeMap<(bool, Key), Vec<(u32, Vec<u32>)>>;

/// The Exchange 2020 namespaces; the envelope reader needs constants for their ids.
pub const EXCHANGE_NAMESPACES: [&str; 4] = [
    "messageContainer",
    "exchangeInformation",
    "informationManagement",
    "cisInformation",
];

/// Conventional short names of the namespaces, used as module aliases inside generated code.
const ALIASES: &[(&str, &str)] = &[
    ("common", "com"),
    ("commonExtension", "comx"),
    ("locationReferencing", "loc"),
    ("locationExtension", "locx"),
    ("situation", "sit"),
    ("roadTrafficData", "roa"),
    ("vms", "vms"),
    ("parking", "prk"),
    ("energyInfrastructure", "egi"),
    ("afirEnergyInfrastructure", "aegi"),
    ("facilities", "fac"),
    ("afirFacilities", "afac"),
    ("trafficRegulation", "tro"),
    ("controlledZone", "cz"),
    ("trafficManagementPlan", "tmp"),
    ("reroutingManagementEnhanced", "rer"),
    ("urbanExtensions", "ubx"),
    ("faultAndStatus", "fst"),
    ("messageContainer", "con"),
    ("exchangeInformation", "ex"),
    ("informationManagement", "inf"),
    ("cisInformation", "cis"),
];

/// Longest route from a publication to a record: two containers and the record itself.
const MAX_ROUTE_LEN: usize = 3;

const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prim {
    Str,
    F32,
    F64,
    Dec,
    I64,
    U64,
    I32,
    Bool,
    DateTime,
    Date,
    Time,
    Bytes,
}

impl Prim {
    fn rust(self) -> &'static str {
        match self {
            Prim::Str => "String",
            Prim::F32 => "f32",
            Prim::F64 => "f64",
            Prim::Dec => "Decimal",
            Prim::I64 => "i64",
            Prim::U64 => "u64",
            Prim::I32 => "i32",
            Prim::Bool => "bool",
            Prim::DateTime => "DateTime",
            Prim::Date => "Date",
            Prim::Time => "Time",
            Prim::Bytes => "Vec<u8>",
        }
    }

    fn of_xs(name: &str) -> Result<Self, String> {
        Ok(match name {
            "string" | "language" | "anyURI" | "duration" => Prim::Str,
            "float" => Prim::F32,
            "double" => Prim::F64,
            "decimal" => Prim::Dec,
            "integer" => Prim::I64,
            "nonNegativeInteger" | "positiveInteger" => Prim::U64,
            "int" => Prim::I32,
            "boolean" => Prim::Bool,
            "dateTime" => Prim::DateTime,
            "date" => Prim::Date,
            "time" => Prim::Time,
            "base64Binary" => Prim::Bytes,
            other => return Err(format!("unsupported built-in type xs:{other}")),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// An extension slot: raw content.
    Ext,
    /// `com:MultilingualString`.
    Multi,
    /// An enumeration, by its simple type.
    Enum(Key),
    /// A primitive, with the simple type that names it if any.
    Prim(Prim, Option<Key>),
    /// A concrete type with no alternatives.
    Struct(Key),
    /// A type with alternatives chosen by `xsi:type`, by its base type.
    Poly(Key),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Card {
    One,
    Opt,
    Many,
}

#[derive(Debug, Clone)]
struct Field {
    rust: String,
    xml: String,
    decl_ns: String,
    kind: Kind,
    card: Card,
    is_attr: bool,
    fixed: bool,
    /// Synthetic field collecting `xs:any` content.
    any: bool,
}

pub struct Emitter<'a> {
    s: &'a Schema,
    ns_ids: BTreeMap<String, u32>,
    names: Vec<String>,
    children: BTreeMap<Key, Vec<Key>>,
}

pub fn snake(name: &str) -> String {
    let chars: Vec<char> = name.trim_start_matches('_').chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let prev_lower =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let acronym_end = i > 0
                && chars[i - 1].is_ascii_uppercase()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev_lower || acronym_end {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn ident(name: String) -> String {
    match name.as_str() {
        "self" | "super" | "crate" | "Self" => format!("{name}_"),
        n if KEYWORDS.contains(&n) => format!("r#{name}"),
        _ => name,
    }
}

/// `PascalCase` identifier from any text: letters and digits are kept, everything else separates
/// words. This is the fixed rule that keeps schema text out of the generated code.
pub fn pascal(text: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(if upper { c.to_ascii_uppercase() } else { c });
            upper = false;
        } else {
            upper = true;
        }
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn type_name(name: &str) -> String {
    pascal(name)
}

impl<'a> Emitter<'a> {
    pub fn new(s: &'a Schema) -> Self {
        let mut sorted: Vec<String> = s.namespaces.clone();
        sorted.sort();
        let ns_ids = sorted
            .iter()
            .map(String::as_str)
            .enumerate()
            .map(|(i, n)| (n.to_owned(), i as u32 + 1))
            .collect();
        let mut names = BTreeSet::new();
        for c in s.complex.values() {
            names.insert(c.name.clone());
            names.extend(c.elements.iter().map(|e| e.name.clone()));
            names.extend(c.attributes.iter().map(|a| a.name.clone()));
        }
        // Names the hand-written readers look up.
        names.extend(["values", "value", "lang", "_extendedValue"].map(str::to_owned));
        let mut children: BTreeMap<Key, Vec<Key>> = BTreeMap::new();
        for (key, c) in &s.complex {
            if let Some(TypeRef::Named(ns, name)) = &c.base {
                children
                    .entry((ns.clone(), name.clone()))
                    .or_default()
                    .push(key.clone());
            }
        }
        Self {
            s,
            ns_ids,
            names: names.into_iter().collect(),
            children,
        }
    }

    fn name_id(&self, name: &str) -> u32 {
        self.names
            .binary_search_by(|n| n.as_str().cmp(name))
            .expect("name is in the table") as u32
    }

    fn key(&self, ns: &str, name: &str) -> u32 {
        self.ns_ids[ns] << 16 | self.name_id(name)
    }

    fn complex(&self, key: &Key) -> Result<&'a Complex, String> {
        self.s
            .complex
            .get(key)
            .ok_or_else(|| format!("unknown type {}:{}", key.0, key.1))
    }

    fn base_key(c: &Complex) -> Option<Key> {
        match &c.base {
            Some(TypeRef::Named(ns, name)) => Some((ns.clone(), name.clone())),
            _ => None,
        }
    }

    /// A subtype that adds nothing but fixed attributes is not a type of its own.
    fn collapse(&self, key: &Key) -> Result<Key, String> {
        let c = self.complex(key)?;
        match Self::base_key(c) {
            Some(base)
                if c.elements.is_empty() && !c.any && c.attributes.iter().all(|a| a.fixed) =>
            {
                self.collapse(&base)
            }
            _ => Ok(key.clone()),
        }
    }

    fn is_special(key: &Key) -> bool {
        key.0 == "common"
            && matches!(
                key.1.as_str(),
                "_ExtensionType" | "MultilingualString" | "MultilingualStringValue"
            )
    }

    fn is_enum_wrapper(&self, c: &Complex) -> Option<Key> {
        match &c.simple_base {
            Some(TypeRef::Named(ns, name))
                if self
                    .s
                    .simple
                    .get(&(ns.clone(), name.clone()))
                    .is_some_and(|s| !s.literals.is_empty()) =>
            {
                Some((ns.clone(), name.clone()))
            }
            _ => None,
        }
    }

    /// Every concrete type that can stand where `key` is declared, `key` itself first.
    fn variants(&self, key: &Key) -> Result<Vec<Key>, String> {
        let mut out = Vec::new();
        let mut stack = vec![key.clone()];
        while let Some(k) = stack.pop() {
            let c = self.complex(&k)?;
            if !c.is_abstract && self.collapse(&k)? == k {
                out.push(k.clone());
            }
            if let Some(kids) = self.children.get(&k) {
                stack.extend(kids.iter().rev().cloned());
            }
        }
        Ok(out)
    }

    fn prim_of_simple(&self, key: &Key) -> Result<Prim, String> {
        let s = self
            .s
            .simple
            .get(key)
            .ok_or_else(|| format!("unknown simple type {}:{}", key.0, key.1))?;
        match &s.base {
            TypeRef::Xs(name) => Prim::of_xs(name),
            TypeRef::Named(ns, name) => self.prim_of_simple(&(ns.clone(), name.clone())),
        }
    }

    fn kind(&self, ty: &TypeRef) -> Result<Kind, String> {
        let key = match ty {
            TypeRef::Xs(name) => return Ok(Kind::Prim(Prim::of_xs(name)?, None)),
            TypeRef::Named(ns, name) => (ns.clone(), name.clone()),
        };
        if let Some(s) = self.s.simple.get(&key) {
            return Ok(if s.literals.is_empty() {
                Kind::Prim(self.prim_of_simple(&key)?, Some(key))
            } else {
                Kind::Enum(key)
            });
        }
        let c = self.complex(&key)?;
        if key == ("common".to_owned(), "_ExtensionType".to_owned()) {
            return Ok(Kind::Ext);
        }
        if key == ("common".to_owned(), "MultilingualString".to_owned()) {
            return Ok(Kind::Multi);
        }
        if let Some(e) = self.is_enum_wrapper(c) {
            return Ok(Kind::Enum(e));
        }
        if c.simple_base.is_some() {
            return Err(format!(
                "{}:{}: simpleContent that is not an enumeration wrapper",
                key.0, key.1
            ));
        }
        let key = self.collapse(&key)?;
        let variants = self.variants(&key)?;
        match variants.as_slice() {
            [] => Err(format!("{}:{} has no concrete type", key.0, key.1)),
            [only] if *only == key => Ok(Kind::Struct(key)),
            _ => Ok(Kind::Poly(key)),
        }
    }

    /// All fields of a type, inherited ones first; attributes before elements.
    fn fields(&self, key: &Key) -> Result<Vec<Field>, String> {
        let mut chain = Vec::new();
        let mut cur = Some(key.clone());
        while let Some(k) = cur {
            let c = self.complex(&k)?;
            cur = Self::base_key(c);
            chain.push(c);
        }
        chain.reverse();
        let mut attrs = Vec::new();
        let mut elements = Vec::new();
        for c in &chain {
            for a in &c.attributes {
                attrs.push(Field {
                    rust: snake(&a.name),
                    xml: a.name.clone(),
                    decl_ns: c.ns.clone(),
                    kind: self.kind(&a.ty)?,
                    card: if a.required { Card::One } else { Card::Opt },
                    is_attr: true,
                    fixed: a.fixed,
                    any: false,
                });
            }
            for e in &c.elements {
                elements.push(Field {
                    rust: snake(&e.name),
                    xml: e.name.clone(),
                    decl_ns: c.ns.clone(),
                    kind: self.kind(&e.ty)?,
                    card: if e.max > 1 {
                        Card::Many
                    } else if e.min == 0 {
                        Card::Opt
                    } else {
                        Card::One
                    },
                    is_attr: false,
                    fixed: false,
                    any: false,
                });
            }
            if c.any {
                elements.push(Field {
                    rust: "other".into(),
                    xml: String::new(),
                    decl_ns: c.ns.clone(),
                    kind: Kind::Ext,
                    card: Card::Many,
                    is_attr: false,
                    fixed: false,
                    any: true,
                });
            }
        }
        attrs.extend(elements);
        // A derived type may declare a name its base already has; the later one is prefixed with
        // the short name of the namespace that declares it.
        let mut seen = BTreeSet::new();
        for f in attrs.iter_mut().filter(|f| !f.fixed) {
            if !seen.insert(f.rust.clone()) {
                f.rust = format!("{}_{}", alias(&f.decl_ns), f.rust);
                if !seen.insert(f.rust.clone()) {
                    return Err(format!(
                        "{}:{}: field name {} is ambiguous",
                        key.0, key.1, f.rust
                    ));
                }
            }
        }
        for f in &mut attrs {
            f.rust = ident(std::mem::take(&mut f.rust));
        }
        Ok(attrs)
    }

    fn path(&self, from_ns: &str, key: &Key, name: &str) -> String {
        if key.0 == from_ns {
            name.to_owned()
        } else {
            format!("{}::{name}", alias(&key.0))
        }
    }

    fn poly_name(&self, key: &Key) -> Result<String, String> {
        let name = type_name(&key.1);
        Ok(if self.complex(key)?.is_abstract {
            name
        } else {
            format!("Any{name}")
        })
    }

    /// Simple types whose Rust alias would shadow the type it stands for.
    fn alias_is_reserved(name: &str) -> bool {
        matches!(name, "String" | "DateTime" | "Date" | "Time" | "Decimal")
    }

    fn base_type(&self, from_ns: &str, kind: &Kind) -> Result<String, String> {
        Ok(match kind {
            Kind::Ext => "Extension".into(),
            Kind::Multi => "MultilingualString".into(),
            Kind::Enum(k) => self.path(from_ns, k, &type_name(&k.1)),
            Kind::Prim(p, Some(k)) if !Self::alias_is_reserved(&type_name(&k.1)) => {
                self.path(from_ns, k, &type_name(&k.1))
            }
            Kind::Prim(p, _) => p.rust().into(),
            Kind::Struct(k) => self.path(from_ns, k, &type_name(&k.1)),
            Kind::Poly(k) => self.path(from_ns, k, &self.poly_name(k)?),
        })
    }

    fn field_type(&self, from_ns: &str, f: &Field) -> Result<String, String> {
        let base = self.base_type(from_ns, &f.kind)?;
        // A required field is optional in the model too: real feeds omit required content, and the
        // reader reports that instead of failing or inventing a value.
        Ok(match f.card {
            Card::Many => format!("Vec<{base}>"),
            _ if matches!(f.kind, Kind::Struct(_) | Kind::Ext) => format!("Option<Box<{base}>>"),
            _ => format!("Option<{base}>"),
        })
    }

    /// Expression of type `Option<base>` that reads the open element.
    fn read_expr(&self, from_ns: &str, f: &Field) -> Result<String, String> {
        Ok(match &f.kind {
            Kind::Ext => "r.raw()?".into(),
            Kind::Multi => "r.multilingual()?".into(),
            Kind::Enum(_) => format!("r.enumeration::<{}>()?", self.base_type(from_ns, &f.kind)?),
            Kind::Prim(Prim::Str, _) => "r.string()?".into(),
            Kind::Prim(Prim::DateTime, _) => "r.date_time()?".into(),
            Kind::Prim(Prim::Bytes, _) => "r.base64()?".into(),
            Kind::Prim(p, _) => format!("r.value::<{}>()?", p.rust()),
            Kind::Struct(_) => format!("Some({}::read(r)?)", self.base_type(from_ns, &f.kind)?),
            Kind::Poly(_) => format!("{}::read(r)?", self.base_type(from_ns, &f.kind)?),
        })
    }

    fn emit_struct(&self, out: &mut String, key: &Key) -> Result<(), String> {
        let ns = &key.0;
        let name = type_name(&key.1);
        let fields = self.fields(key)?;
        let kept: Vec<&Field> = fields.iter().filter(|f| !f.fixed).collect();
        writeln!(out, "/// XML type `{}:{}`.", alias(ns), key.1).unwrap();
        writeln!(
            out,
            "#[derive(Debug, Clone, Default, PartialEq)]\npub struct {name} {{"
        )
        .unwrap();
        for f in &kept {
            let what = if f.any {
                "Elements from other namespaces, kept as written.".to_owned()
            } else if f.is_attr {
                format!("XML attribute `{}`.", f.xml)
            } else {
                format!("XML element `{}:{}`.", alias(&f.decl_ns), f.xml)
            };
            writeln!(
                out,
                "    /// {what}\n    pub {}: {},",
                f.rust,
                self.field_type(ns, f)?
            )
            .unwrap();
        }
        writeln!(out, "}}\n").unwrap();

        let elements: Vec<&Field> = fields.iter().filter(|f| !f.is_attr && !f.any).collect();
        let any = fields.iter().find(|f| f.any);
        writeln!(out, "impl {name} {{").unwrap();
        let keys: Vec<String> = elements
            .iter()
            .map(|f| format!("{:#x}", self.key(&f.decl_ns, &f.xml)))
            .collect();
        writeln!(
            out,
            "    const FIELDS: &'static [u32] = &[{}];\n",
            keys.join(", ")
        )
        .unwrap();
        writeln!(
            out,
            "    pub(crate) fn read(r: &mut Xml<'_>) -> Result<Self, Error> {{"
        )
        .unwrap();
        // A type whose subtypes collapse into it accepts their names too.
        let has_collapsed = self.children.get(key).is_some_and(|kids| {
            kids.iter()
                .any(|k| self.collapse(k).is_ok_and(|c| c == *key))
        });
        if !has_collapsed {
            writeln!(out, "        r.expect_type({:#x});", self.key(ns, &key.1)).unwrap();
        }
        for (i, f) in fields.iter().filter(|f| f.is_attr).enumerate() {
            let id = self.name_id(&f.xml);
            if f.fixed {
                writeln!(out, "        let _ = r.attr({id});").unwrap();
                continue;
            }
            let required = f.card == Card::One;
            let expr = match &f.kind {
                Kind::Prim(Prim::Str, _) if required => format!("r.req_attr({id})"),
                Kind::Prim(Prim::Str, _) => format!("r.attr({id})"),
                Kind::Prim(p, _) if required => format!("r.req_attr_value::<{}>({id})?", p.rust()),
                Kind::Prim(p, _) => format!("r.attr_value::<{}>({id})?", p.rust()),
                Kind::Enum(_) if !required => {
                    format!("r.attr_enum::<{}>({id})", self.base_type(ns, &f.kind)?)
                }
                _ => {
                    return Err(format!(
                        "{}:{}/@{}: unsupported attribute type",
                        ns, key.1, f.xml
                    ));
                }
            };
            writeln!(out, "        let a{i} = {expr};").unwrap();
        }
        if has_collapsed {
            // Subtypes that collapse into this type only add attributes with a fixed value.
            let mut fixed = BTreeSet::new();
            for kid in self
                .children
                .get(key)
                .into_iter()
                .flatten()
                .filter(|k| self.collapse(k).is_ok_and(|c| c == *key))
            {
                fixed.extend(
                    self.complex(kid)?
                        .attributes
                        .iter()
                        .map(|a| self.name_id(&a.name)),
                );
            }
            for id in fixed {
                writeln!(out, "        let _ = r.attr({id});").unwrap();
            }
        }
        writeln!(out, "        r.attrs_done();").unwrap();
        for (i, f) in elements.iter().enumerate() {
            writeln!(
                out,
                "        let mut e{i} = {};",
                if f.card == Card::Many {
                    "Vec::new()"
                } else {
                    "None"
                }
            )
            .unwrap();
        }
        if any.is_some() {
            writeln!(out, "        let mut other = Vec::new();").unwrap();
        }
        writeln!(out, "        while let Some(mut key) = r.next_child()? {{\n            loop {{\n                match key {{").unwrap();
        for (i, f) in elements.iter().enumerate() {
            let expr = self.read_expr(ns, f)?;
            let id = self.name_id(&f.xml);
            let store = match f.card {
                Card::Many => format!("e{i}.extend(v);"),
                _ if matches!(f.kind, Kind::Struct(_) | Kind::Ext) => {
                    format!("r.set(&mut e{i}, v.map(Box::new), {id});")
                }
                _ => format!("r.set(&mut e{i}, v, {id});"),
            };
            writeln!(
                out,
                "                    {:#x} => {{ let v = {expr}; {store} }}",
                self.key(&f.decl_ns, &f.xml)
            )
            .unwrap();
        }
        let unknown = if any.is_some() {
            "other.push(r.raw_element()?)"
        } else {
            "r.skip_unknown(key)?"
        };
        writeln!(out, "                    _ => match r.fallback(key, Self::FIELDS) {{\n                        Some(k) => {{ key = k; continue; }}\n                        None => {unknown},\n                    }},").unwrap();
        writeln!(
            out,
            "                }}\n                break;\n            }}\n        }}"
        )
        .unwrap();
        for (i, f) in elements
            .iter()
            .enumerate()
            .filter(|(_, f)| f.card == Card::One)
        {
            writeln!(out, "        r.required(&e{i}, {});", self.name_id(&f.xml)).unwrap();
        }
        writeln!(out, "        let v = Self {{").unwrap();
        for (i, f) in fields.iter().filter(|f| f.is_attr).enumerate() {
            if !f.fixed {
                writeln!(out, "            {}: a{i},", f.rust).unwrap();
            }
        }
        for (i, f) in elements.iter().enumerate() {
            writeln!(out, "            {}: e{i},", f.rust).unwrap();
        }
        if any.is_some() {
            writeln!(out, "            other,").unwrap();
        }
        writeln!(
            out,
            "        }};\n        r.close();\n        Ok(v)\n    }}\n}}\n"
        )
        .unwrap();
        Ok(())
    }

    fn variant_names(&self, base: &Key, variants: &[Key]) -> Result<Vec<String>, String> {
        let root = *base == ("common".to_owned(), "PayloadPublication".to_owned());
        let mut names: Vec<String> = variants
            .iter()
            .map(|k| {
                let n = type_name(&k.1);
                if root {
                    n.strip_suffix("Publication").unwrap_or(&n).to_owned()
                } else {
                    n
                }
            })
            .collect();
        for i in 0..names.len() {
            if names.iter().filter(|n| **n == names[i]).count() > 1
                && variants[i].0.starts_with("afir")
            {
                names[i] = format!("Afir{}", names[i]);
            }
        }
        let unique: BTreeSet<&String> = names.iter().collect();
        if unique.len() != names.len() {
            return Err(format!(
                "{}:{}: variant names are ambiguous: {names:?}",
                base.0, base.1
            ));
        }
        Ok(names)
    }

    fn emit_poly(&self, out: &mut String, key: &Key) -> Result<(), String> {
        let ns = &key.0;
        let name = self.poly_name(key)?;
        let variants = self.variants(key)?;
        let names = self.variant_names(key, &variants)?;
        let base_is_variant = variants.first() == Some(key);
        writeln!(
            out,
            "/// Any concrete kind of XML type `{}:{}`, chosen by `xsi:type`.",
            alias(ns),
            key.1
        )
        .unwrap();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq)]\npub enum {name} {{"
        )
        .unwrap();
        for (v, n) in variants.iter().zip(&names) {
            writeln!(
                out,
                "    /// XML type `{}:{}`.\n    {n}(Box<{}>),",
                alias(&v.0),
                v.1,
                self.path(ns, v, &type_name(&v.1))
            )
            .unwrap();
        }
        writeln!(out, "}}\n\nimpl {name} {{").unwrap();
        writeln!(out, "    pub(crate) fn read(r: &mut Xml<'_>) -> Result<Option<Self>, Error> {{\n        Self::read_as(r, r.xsi_type())\n    }}\n").unwrap();
        writeln!(out, "    pub(crate) fn read_as(r: &mut Xml<'_>, kind: Option<u32>) -> Result<Option<Self>, Error> {{\n        Ok(Some(match kind {{").unwrap();
        for (v, n) in variants.iter().zip(&names) {
            writeln!(
                out,
                "            Some({:#x}) => Self::{n}(Box::new({}::read(r)?)),",
                self.key(&v.0, &v.1),
                self.path(ns, v, &type_name(&v.1))
            )
            .unwrap();
        }
        if base_is_variant {
            writeln!(
                out,
                "            None => Self::{}(Box::new({}::read(r)?)),",
                names[0],
                type_name(&key.1)
            )
            .unwrap();
        }
        if base_is_variant {
            writeln!(out, "            _ => {{\n                r.unknown_type_as(\"{}\");\n                Self::{}(Box::new({}::read(r)?))\n            }}\n        }}))\n    }}", key.1, names[0], type_name(&key.1)).unwrap();
        } else {
            writeln!(out, "            _ => {{\n                r.unknown_type(\"{}\")?;\n                return Ok(None);\n            }}\n        }}))\n    }}", key.1).unwrap();
        }
        for f in self.fields(key)?.iter().filter(|f| !f.fixed) {
            let what = if f.is_attr {
                format!("XML attribute `{}`", f.xml)
            } else if f.any {
                "Elements from other namespaces".to_owned()
            } else {
                format!("XML element `{}:{}`", alias(&f.decl_ns), f.xml)
            };
            let base = self.base_type(ns, &f.kind)?;
            let (ty, get) = match f.card {
                Card::Many => (format!("&[{base}]"), ""),
                _ if matches!(f.kind, Kind::Struct(_) | Kind::Ext) => {
                    (format!("Option<&{base}>"), ".as_deref()")
                }
                _ if matches!(f.kind, Kind::Prim(Prim::Str, _)) => {
                    ("Option<&str>".to_owned(), ".as_deref()")
                }
                _ => (format!("Option<&{base}>"), ".as_ref()"),
            };
            writeln!(out, "\n    /// {what}, which every kind has.\n    pub fn {}(&self) -> {ty} {{\n        match self {{", f.rust).unwrap();
            for n in &names {
                writeln!(
                    out,
                    "            Self::{n}(v) => {}v.{}{get},",
                    if f.card == Card::Many { "&" } else { "" },
                    f.rust
                )
                .unwrap();
            }
            writeln!(out, "        }}\n    }}").unwrap();
        }
        writeln!(out, "}}\n").unwrap();
        Ok(())
    }

    fn emit_enumeration(&self, out: &mut String, key: &Key, aliases: &str) -> Result<(), String> {
        let s = &self.s.simple[key];
        let name = type_name(&s.name);
        let literals: Vec<&String> = s.literals.iter().filter(|l| *l != "_extended").collect();
        let variants: Vec<String> = literals.iter().map(|l| pascal(l)).collect();
        let unique: BTreeSet<&String> = variants.iter().collect();
        if unique.len() != variants.len() || variants.iter().any(|v| v == "Extended") {
            return Err(format!(
                "{}:{}: enumeration literals do not map to distinct variants",
                key.0, key.1
            ));
        }
        writeln!(out, "/// XML enumeration `{}:{}`.", alias(&key.0), s.name).unwrap();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub enum {name} {{"
        )
        .unwrap();
        for (l, v) in literals.iter().zip(&variants) {
            writeln!(out, "    /// `{l}`\n    {v},").unwrap();
        }
        writeln!(
            out,
            "    /// A value outside the standard list.\n    Extended(String),\n}}\n"
        )
        .unwrap();
        writeln!(out, "impl {name} {{\n    /// The value as written in a document.\n    pub fn as_str(&self) -> &str {{\n        match self {{").unwrap();
        for (l, v) in literals.iter().zip(&variants) {
            writeln!(out, "            Self::{v} => \"{l}\",").unwrap();
        }
        writeln!(
            out,
            "            Self::Extended(v) => v,\n        }}\n    }}\n}}\n"
        )
        .unwrap();
        writeln!(out, "impl Enumeration for {name} {{\n    fn from_literal(literal: &str) -> Option<Self> {{\n        match literal {{").unwrap();
        for (l, v) in literals.iter().zip(&variants) {
            writeln!(out, "            \"{l}\" => Some(Self::{v}),").unwrap();
        }
        writeln!(out, "            _ => None,\n        }}\n    }}\n\n    fn extended(value: String) -> Self {{\n        Self::Extended(value)\n    }}{aliases}\n}}\n").unwrap();
        Ok(())
    }

    /// The canonical tag of an element: prefix of the declaring namespace and the name.
    fn tag(ns: &str, name: &str) -> String {
        format!("{}:{name}", prefix(ns))
    }

    /// Schema information about each field of a type that the reader does not need: the declared
    /// type and the allowed number of occurrences.
    fn declared(&self, key: &Key) -> Result<Declared, String> {
        let mut out = BTreeMap::new();
        let mut cur = Some(key.clone());
        while let Some(k) = cur {
            let c = self.complex(&k)?;
            for e in &c.elements {
                out.insert(
                    (false, c.ns.clone(), e.name.clone()),
                    (e.ty.clone(), e.min, e.max),
                );
            }
            for a in &c.attributes {
                out.insert(
                    (true, c.ns.clone(), a.name.clone()),
                    (a.ty.clone(), u32::from(a.required), 1),
                );
            }
            cur = Self::base_key(c);
        }
        Ok(out)
    }

    /// Facets that apply to a simple type: its own and those of its bases, the nearest winning.
    fn facets(&self, ty: &TypeRef) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let mut cur = ty.clone();
        while let TypeRef::Named(ns, name) = &cur {
            let Some(s) = self.s.simple.get(&(ns.clone(), name.clone())) else {
                break;
            };
            for (facet, value) in &s.facets {
                out.entry(facet.clone()).or_insert_with(|| value.clone());
            }
            cur = s.base.clone();
        }
        out
    }

    /// The built-in XML Schema type at the root of a simple type.
    fn builtin<'t>(&'t self, ty: &'t TypeRef) -> &'t str {
        let mut cur = ty;
        loop {
            match cur {
                TypeRef::Xs(name) => return name,
                TypeRef::Named(ns, name) => match self.s.simple.get(&(ns.clone(), name.clone())) {
                    Some(s) => cur = &s.base,
                    None => return "",
                },
            }
        }
    }

    /// Statements that check `value` (an expression of the primitive's reference type) against the
    /// facets of `ty`.
    fn facet_checks(
        &self,
        ty: &TypeRef,
        prim: Prim,
        name: &str,
        value: &str,
    ) -> Result<String, String> {
        let facets = self.facets(ty);
        let mut out = String::new();
        let number = |v: &String| {
            v.parse::<u64>()
                .map_err(|_| format!("{name}: facet value {v} is not a non-negative integer"))
        };
        if let Some(max) = facets.get("maxLength") {
            if prim != Prim::Str {
                return Err(format!("{name}: maxLength on a type that is not text"));
            }
            write!(out, "w.max_len(\"{name}\", {value}, {})?; ", number(max)?).unwrap();
        }
        if facets.contains_key("minInclusive") || facets.contains_key("maxInclusive") {
            if prim != Prim::U64 {
                return Err(format!(
                    "{name}: a range on a type that is not a non-negative integer"
                ));
            }
            let min = facets
                .get("minInclusive")
                .map(number)
                .transpose()?
                .unwrap_or(0);
            let max = facets
                .get("maxInclusive")
                .map(number)
                .transpose()?
                .map_or("u64::MAX".to_owned(), |m| m.to_string());
            write!(out, "w.range(\"{name}\", *{value}, {min}, {max})?; ").unwrap();
        }
        if let (Some(total), Some(fraction)) =
            (facets.get("totalDigits"), facets.get("fractionDigits"))
        {
            if prim != Prim::Dec {
                return Err(format!(
                    "{name}: digit facets on a type that is not a decimal"
                ));
            }
            write!(
                out,
                "w.digits(\"{name}\", {value}, {}, {})?; ",
                number(total)?,
                number(fraction)?
            )
            .unwrap();
        }
        // Built-in types with a lexical form of their own that the model keeps as text.
        match self.builtin(ty) {
            "language" => {
                write!(out, "w.pattern(\"{name}\", {value}, Pattern::Language)?; ").unwrap()
            }
            "duration" => {
                write!(out, "w.pattern(\"{name}\", {value}, Pattern::Duration)?; ").unwrap()
            }
            _ => {}
        }
        if let Some(pattern) = facets.get("pattern") {
            // Each pattern of the schemas has a hand-written check; a new one must get one too.
            let check = match pattern.as_str() {
                "[1-9]|[4-6].[1-2]|4.3" => "AdrClass",
                r"((1?[0-9]?[0-9]|2[0-4][0-9]|25[0-5])\.){3}(1?[0-9]?[0-9]|2[0-4][0-9]|25[0-5])" => {
                    "IpAddress"
                }
                "[A-Z][A-Z][A-Z]" => "CurrencyCode",
                "[-+][0-9][0-9]:[0-9][0-9]|Z" => "TimeZone",
                r"[-+]?[0-9]*\.?[0-9]+(\s[-+]?[0-9]*\.?[0-9]+){3,}" => "GmlPosList",
                other => {
                    return Err(format!(
                        "{name}: no check is written for the pattern {other}"
                    ));
                }
            };
            if prim != Prim::Str {
                return Err(format!("{name}: a pattern on a type that is not text"));
            }
            write!(out, "w.pattern(\"{name}\", {value}, Pattern::{check})?; ").unwrap();
        }
        Ok(out)
    }

    /// Attributes with a fixed value that the declared type adds on the way to the type it
    /// collapses into, as constant attribute text.
    fn fixed_attributes(&self, ty: &TypeRef) -> Result<String, String> {
        let mut out = String::new();
        let TypeRef::Named(ns, name) = ty else {
            return Ok(out);
        };
        let mut key = (ns.clone(), name.clone());
        let Some(_) = self.s.complex.get(&key) else {
            return Ok(out);
        };
        let target = self.collapse(&key)?;
        while key != target {
            let c = self.complex(&key)?;
            for a in &c.attributes {
                let value = a.fixed_value.as_ref().ok_or_else(|| {
                    format!("{}:{}: a collapsed type has a free attribute", key.0, key.1)
                })?;
                if !value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '_' | '.' | '-'))
                {
                    return Err(format!(
                        "{}:{}/@{}: fixed value {value:?} needs escaping",
                        key.0, key.1, a.name
                    ));
                }
                write!(out, " {}=\\\"{value}\\\"", a.name).unwrap();
            }
            key = Self::base_key(c).ok_or("a collapsed type without base")?;
        }
        Ok(out)
    }

    fn emit_struct_write(&self, out: &mut String, key: &Key) -> Result<(), String> {
        let ns = &key.0;
        let name = type_name(&key.1);
        let fields = self.fields(key)?;
        let declared = self.declared(key)?;
        writeln!(out, "impl {name} {{\n    pub(crate) fn write(&self, w: &mut Out<'_>, tag: &'static str, kind: &'static str, fixed: &'static str) -> Result<(), Error> {{\n        w.begin(tag)?;\n        w.constant_attrs(kind)?;\n        w.constant_attrs(fixed)?;").unwrap();
        // Attributes the type itself fixes.
        let mut cur = Some(key.clone());
        while let Some(k) = cur {
            let c = self.complex(&k)?;
            for a in c.attributes.iter().filter(|a| a.fixed) {
                let value = a.fixed_value.clone().unwrap_or_default();
                if !value.chars().all(|ch| ch.is_ascii_alphanumeric()) {
                    return Err(format!(
                        "{}:{}/@{}: fixed value {value:?} needs escaping",
                        k.0, k.1, a.name
                    ));
                }
                writeln!(
                    out,
                    "        w.constant_attrs(\" {}=\\\"{value}\\\"\")?;",
                    a.name
                )
                .unwrap();
            }
            cur = Self::base_key(c);
        }
        for f in fields.iter().filter(|f| !f.fixed && !f.any) {
            let (ty, min, max) = declared
                .get(&(f.is_attr, f.decl_ns.clone(), f.xml.clone()))
                .ok_or_else(|| format!("{}:{}: no declaration for {}", ns, key.1, f.xml))?
                .clone();
            let field = format!("self.{}", f.rust);
            if f.is_attr {
                let write = match &f.kind {
                    Kind::Prim(Prim::Str, _) => format!(
                        "{}w.attr(\"{}\", v)?;",
                        self.facet_checks(&ty, Prim::Str, &f.xml, "v")?,
                        f.xml
                    ),
                    Kind::Prim(p, _) => format!(
                        "{}w.attr_value(\"{}\", v)?;",
                        self.facet_checks(&ty, *p, &f.xml, "v")?,
                        f.xml
                    ),
                    Kind::Enum(_) => format!("w.attr(\"{}\", v.as_str())?;", f.xml),
                    _ => {
                        return Err(format!(
                            "{}:{}/@{}: unsupported attribute type",
                            ns, key.1, f.xml
                        ));
                    }
                };
                if min == 1 {
                    writeln!(out, "        match &{field} {{\n            Some(v) => {{ {write} }}\n            None => return Err(w.missing_attr(\"{}\")),\n        }}", f.xml).unwrap();
                } else {
                    writeln!(out, "        if let Some(v) = &{field} {{ {write} }}").unwrap();
                }
                continue;
            }
            let tag = Self::tag(&f.decl_ns, &f.xml);
            let write = match &f.kind {
                Kind::Ext => format!("w.extension(\"{tag}\", v)?;"),
                Kind::Multi => format!("w.multilingual(\"{tag}\", v)?;"),
                Kind::Enum(_) => format!("w.enumeration(\"{tag}\", v.literal())?;"),
                Kind::Prim(p, _) => format!(
                    "{}w.leaf(\"{tag}\", v)?;",
                    self.facet_checks(&ty, *p, &f.xml, "v")?
                ),
                Kind::Struct(_) => format!(
                    "v.write(w, \"{tag}\", \"\", \"{}\")?;",
                    self.fixed_attributes(&ty)?
                ),
                Kind::Poly(_) => format!("v.write(w, \"{tag}\")?;"),
            };
            match f.card {
                Card::Many => {
                    let max = if max == u32::MAX { "usize::MAX".to_owned() } else { max.to_string() };
                    if min > 0 || max != "usize::MAX" {
                        writeln!(out, "        w.count(\"{tag}\", {field}.len(), {min}, {max})?;").unwrap();
                    }
                    writeln!(out, "        for v in &{field} {{ {write} }}").unwrap();
                }
                Card::One => writeln!(out, "        match &{field} {{\n            Some(v) => {{ {write} }}\n            None => return Err(w.missing(\"{tag}\")),\n        }}").unwrap(),
                Card::Opt => writeln!(out, "        if let Some(v) = &{field} {{ {write} }}").unwrap(),
            }
        }
        if fields.iter().any(|f| f.any) {
            writeln!(
                out,
                "        for v in &self.other {{ w.foreign_element(v)?; }}"
            )
            .unwrap();
        }
        writeln!(out, "        w.end(tag)\n    }}\n}}\n").unwrap();
        Ok(())
    }

    fn emit_poly_write(
        &self,
        out: &mut String,
        key: &Key,
        root: &Key,
        declarations: &BTreeMap<Key, String>,
    ) -> Result<(), String> {
        let name = self.poly_name(key)?;
        let variants = self.variants(key)?;
        let names = self.variant_names(key, &variants)?;
        writeln!(out, "impl {name} {{\n    pub(crate) fn write(&self, w: &mut Out<'_>, tag: &'static str) -> Result<(), Error> {{\n        match self {{").unwrap();
        for (v, n) in variants.iter().zip(&names) {
            // The declared type needs no `xsi:type`; every other kind does. The payload always does.
            let kind = if v == key && key != root {
                String::new()
            } else {
                format!(" xsi:type=\\\"{}\\\"", Self::tag(&v.0, &v.1))
            };
            let fixed = if key == root {
                declarations.get(v).cloned().unwrap_or_default()
            } else {
                String::new()
            };
            writeln!(
                out,
                "            Self::{n}(v) => v.write(w, tag, \"{kind}\", \"{fixed}\"),"
            )
            .unwrap();
        }
        writeln!(out, "        }}\n    }}\n}}\n").unwrap();
        Ok(())
    }

    fn emit_enumeration_write(&self, out: &mut String, key: &Key) {
        let name = type_name(&self.s.simple[key].name);
        writeln!(out, "impl {name} {{\n    /// The literal to write and, for a value outside the standard list, that value.\n    #[allow(unreachable_patterns)]\n    pub(crate) fn literal(&self) -> (&str, Option<&str>) {{\n        match self {{\n            Self::Extended(v) => (\"_extended\", Some(v)),\n            other => (other.as_str(), None),\n        }}\n    }}\n}}\n").unwrap();
    }

    /// Per publication type, the namespace declarations its documents need, as constant attribute
    /// text: every namespace reachable from the type through fields and their kinds.
    fn namespace_declarations(
        &self,
        publications: &[Key],
    ) -> Result<BTreeMap<Key, String>, String> {
        let mut out = BTreeMap::new();
        for publication in publications {
            let mut namespaces = BTreeSet::from(["d2Payload".to_owned(), "common".to_owned()]);
            let mut seen = BTreeSet::new();
            let mut stack = vec![publication.clone()];
            while let Some(k) = stack.pop() {
                if !seen.insert(k.clone()) {
                    continue;
                }
                namespaces.insert(k.0.clone());
                for f in self.fields(&k)? {
                    namespaces.insert(f.decl_ns.clone());
                    match &f.kind {
                        Kind::Struct(t) => stack.push(t.clone()),
                        Kind::Poly(t) => stack.extend(self.variants(t)?),
                        _ => {}
                    }
                }
            }
            let mut text =
                String::from(" xmlns:xsi=\\\"http://www.w3.org/2001/XMLSchema-instance\\\"");
            for ns in namespaces {
                write!(
                    text,
                    " xmlns:{}=\\\"http://datex2.eu/schema/3/{ns}\\\"",
                    prefix(&ns)
                )
                .unwrap();
            }
            out.insert(publication.clone(), text);
        }
        Ok(out)
    }

    /// Where each record type sits under each publication type: the chain of element keys from
    /// the publication to a repeated element, through at most two levels of containers.
    fn routes(&self, publications: &[Key]) -> Result<Routes, String> {
        let mut out = Routes::new();
        for publication in publications {
            let publication_key = self.key(&publication.0, &publication.1);
            let mut stack = vec![(publication.clone(), Vec::<u32>::new())];
            while let Some((container, path)) = stack.pop() {
                for f in self
                    .fields(&container)?
                    .iter()
                    .filter(|f| !f.is_attr && !f.any)
                {
                    let mut here = path.clone();
                    here.push(self.key(&f.decl_ns, &f.xml));
                    let target = match &f.kind {
                        Kind::Struct(k) => (false, k.clone()),
                        Kind::Poly(k) => (true, k.clone()),
                        _ => continue,
                    };
                    if f.card == Card::Many {
                        out.entry(target.clone())
                            .or_default()
                            .push((publication_key, here.clone()));
                    }
                    // Only a plain struct can be a container: its children do not depend on xsi:type.
                    if !target.0 && here.len() < MAX_ROUTE_LEN {
                        stack.push((target.1, here));
                    }
                }
            }
        }
        for routes in out.values_mut() {
            routes.sort();
        }
        Ok(out)
    }

    fn emit_records(
        &self,
        header: &str,
        root: &Key,
        publications: &[Key],
    ) -> Result<String, String> {
        let mut out = header.to_owned();
        out.push_str("//! Which types can be streamed as records, and where they sit under each publication.\n\nuse crate::Error;\nuse crate::v3::record::{Record, Sealed};\nuse crate::xml::Xml;\n\n");
        let full = |key: &Key, name: &str| format!("super::{}::{name}", snake(&key.0));
        let name = self.poly_name(root)?;
        writeln!(out, "impl Sealed for {} {{\n    const NAME: &'static str = \"Publication\";\n    const ROUTES: &'static [(u32, &'static [u32])] = &[", full(root, &name)).unwrap();
        for p in publications {
            writeln!(out, "        ({:#x}, &[]),", self.key(&p.0, &p.1)).unwrap();
        }
        writeln!(out, "    ];\n\n    fn read_record(r: &mut Xml<'_>, kind: Option<u32>) -> Result<Option<Self>, Error> {{\n        Self::read_as(r, kind)\n    }}\n}}\nimpl Record for {} {{}}\n", full(root, &name)).unwrap();
        for ((poly, key), routes) in self.routes(publications)? {
            let name = if poly {
                self.poly_name(&key)?
            } else {
                type_name(&key.1)
            };
            let path = full(&key, &name);
            writeln!(out, "impl Sealed for {path} {{\n    const NAME: &'static str = \"{name}\";\n    const ROUTES: &'static [(u32, &'static [u32])] = &[").unwrap();
            for (publication, route) in &routes {
                let keys: Vec<String> = route.iter().map(|k| format!("{k:#x}")).collect();
                writeln!(out, "        ({publication:#x}, &[{}]),", keys.join(", ")).unwrap();
            }
            let read = if poly {
                "Self::read(r)"
            } else {
                "Ok(Some(Self::read(r)?))"
            };
            writeln!(out, "    ];\n\n    fn read_record(r: &mut Xml<'_>, _kind: Option<u32>) -> Result<Option<Self>, Error> {{\n        {read}\n    }}\n}}\nimpl Record for {path} {{}}\n").unwrap();
        }
        Ok(out)
    }

    /// Returns file name → content for everything under `src/v3/generated/`.
    pub fn emit(
        &self,
        version: &str,
        official: &JsonDefs,
        profile: &JsonDefs,
    ) -> Result<BTreeMap<String, String>, String> {
        // Which types get a struct, and which bases get an enum.
        let mut structs: BTreeSet<Key> = BTreeSet::new();
        for (key, c) in &self.s.complex {
            if c.is_abstract
                || Self::is_special(key)
                || c.simple_base.is_some()
                || self.collapse(key)? != *key
            {
                continue;
            }
            structs.insert(key.clone());
        }
        let root: Key = ("common".into(), "PayloadPublication".into());
        let mut polys: BTreeSet<Key> = BTreeSet::from([root.clone()]);
        for key in &structs {
            for f in self.fields(key)? {
                if let Kind::Poly(k) = f.kind {
                    polys.insert(k);
                }
            }
        }

        let declarations = self.namespace_declarations(&self.variants(&root)?)?;
        let enums: BTreeSet<Key> = self
            .s
            .simple
            .iter()
            .filter(|(k, s)| {
                !s.literals.is_empty() && !(k.1.starts_with('_') && k.1.ends_with("ExtensionType"))
            })
            .map(|(k, _)| k.clone())
            .collect();
        self.check_json_names(&structs, &polys, &enums, official)?;
        let profile_table = self.profile_table(&structs, &polys, &enums, profile)?;
        let summary = format!(
            "profile: {} definitions, {} enumerations, {} renames, {} unwrapped, {} literal renames",
            profile_table.defs.len(),
            profile_table.literals.len(),
            profile_table.renames.len(),
            profile_table.unwraps.len(),
            profile_table.literal_renames.len()
        );
        let plan = self.json_plan(&structs, &polys, &enums, profile_table)?;
        let header = format!(
            "// @generated by `cargo xtask codegen` from the DATEX II {version} schemas. Do not edit.\n"
        );
        let mut files = BTreeMap::new();
        let mut modules = Vec::new();
        let mut sorted_ns: Vec<&String> = self.s.namespaces.iter().collect();
        sorted_ns.sort();
        for ns in sorted_ns {
            let mut body = String::new();
            let mut defined: BTreeMap<String, &str> = BTreeMap::new();
            let mut define = |name: String, what: &'static str| -> Result<(), String> {
                match defined.insert(name.clone(), what) {
                    Some(prev) => Err(format!(
                        "{ns}: the name {name} is used for a {prev} and a {what}"
                    )),
                    None => Ok(()),
                }
            };
            for (key, s) in self.s.simple.iter().filter(|(k, _)| k.0 == *ns) {
                if s.name.starts_with('_') && s.name.ends_with("ExtensionType") {
                    continue; // closed lists of extended values; the value is kept as text
                }
                let name = type_name(&s.name);
                if !s.literals.is_empty() {
                    define(name, "enumeration")?;
                    self.emit_enumeration(&mut body, key, &self.json_enum_aliases(key, &plan))?;
                    self.emit_enumeration_write(&mut body, key);
                } else if !Self::alias_is_reserved(&name) {
                    define(name.clone(), "type alias")?;
                    let target = match &s.base {
                        TypeRef::Named(bns, bname)
                            if !Self::alias_is_reserved(&type_name(bname)) =>
                        {
                            self.path(ns, &(bns.clone(), bname.clone()), &type_name(bname))
                        }
                        _ => self.prim_of_simple(key)?.rust().to_owned(),
                    };
                    writeln!(
                        body,
                        "/// XML simple type `{}:{}`.\npub type {name} = {target};\n",
                        alias(ns),
                        s.name
                    )
                    .unwrap();
                }
            }
            for key in structs.iter().filter(|k| k.0 == *ns) {
                define(type_name(&key.1), "struct")?;
                self.emit_struct(&mut body, key)?;
                self.emit_struct_write(&mut body, key)?;
                self.emit_struct_json(&mut body, key, &plan)?;
            }
            for key in polys.iter().filter(|k| k.0 == *ns) {
                define(self.poly_name(key)?, "enum")?;
                self.emit_poly(&mut body, key)?;
                self.emit_poly_write(&mut body, key, &root, &declarations)?;
                self.emit_poly_json(&mut body, key, &plan)?;
            }
            if body.is_empty() {
                continue;
            }
            let module = snake(ns);
            let uses: Vec<String> = ALIASES
                .iter()
                .filter(|(n, _)| *n != ns.as_str() && self.s.namespaces.iter().any(|x| x == n))
                .map(|(n, a)| {
                    if snake(n) == *a {
                        a.to_string()
                    } else {
                        format!("{} as {a}", snake(n))
                    }
                })
                .collect();
            let mut file = header.clone();
            writeln!(file, "//! Types of the DATEX II namespace `http://datex2.eu/schema/3/{ns}`.\n//!\n//! Names follow the standard; see <https://docs.datex2.eu/> for what each one means.\n").unwrap();
            writeln!(file, "#[allow(unused_imports)]\nuse super::{{{}}};\n#[allow(unused_imports)]\nuse crate::out::{{Out, Pattern, Rules}};\n#[allow(unused_imports)]\nuse crate::json::{{JsonReader, JsonWriter}};\n#[allow(unused_imports)]\nuse crate::{{ErrorKind, WarningKind}};\n#[allow(unused_imports)]\nuse crate::xml::{{Enumeration, Xml}};\n#[allow(unused_imports)]\nuse crate::{{Date, DateTime, Decimal, Error, Extension, MultilingualString, Time}};\n", uses.join(", ")).unwrap();
            file.push_str(&body);
            files.insert(format!("{module}.rs"), file);
            modules.push((module, ns.clone()));
        }

        let file_header = header.clone();
        let mut m = header;
        m.push_str("//! Vocabulary tables and the module list.\n\nuse crate::json::{JsonTables, Profile};\nuse crate::xml::Tables;\n\n#[rustfmt::skip]\nmod records;\n\n");
        for (module, ns) in &modules {
            writeln!(
                m,
                "#[rustfmt::skip]\n#[allow(clippy::all, dead_code, unused_mut, unused_variables)]\npub mod {module}; // {ns}"
            )
            .unwrap();
        }
        let mut by_id: Vec<(&String, &u32)> = self.ns_ids.iter().collect();
        by_id.sort_by_key(|(_, id)| **id);
        writeln!(m).unwrap();
        for (ns, id) in &by_id {
            if **ns == "d2Payload" || EXCHANGE_NAMESPACES.contains(&ns.as_str()) {
                writeln!(
                    m,
                    "pub(crate) const NS_{}: u32 = {id};",
                    snake(ns).to_uppercase()
                )
                .unwrap();
            }
        }
        writeln!(m, "\n/// Keys of the concrete publication types.\npub(crate) const PUBLICATION_TYPES: &[u32] = &[").unwrap();
        let publications = self.variants(&root)?;
        for p in &publications {
            writeln!(
                m,
                "    {:#x}, // {}:{}",
                self.key(&p.0, &p.1),
                alias(&p.0),
                p.1
            )
            .unwrap();
        }
        writeln!(m, "];\n\n/// Child elements that a publication type declares itself, with the type they belong to.\npub(crate) const PUBLICATION_CHILDREN: &[(u32, u32)] = &[").unwrap();
        let base_fields: BTreeSet<String> =
            self.fields(&root)?.into_iter().map(|f| f.xml).collect();
        for p in &publications {
            for f in self
                .fields(p)?
                .iter()
                .filter(|f| !f.is_attr && !f.any && !base_fields.contains(&f.xml))
            {
                writeln!(
                    m,
                    "    ({:#x}, {:#x}),",
                    self.key(&f.decl_ns, &f.xml),
                    self.key(&p.0, &p.1)
                )
                .unwrap();
            }
        }
        writeln!(m, "];\n\npub(crate) static TABLES: Tables = Tables {{\n    base: \"http://datex2.eu/schema/3/\",\n    level_c: \"http://levelC/schema/3/\",\n    namespaces: &[").unwrap();
        for (ns, _) in &by_id {
            writeln!(m, "        \"{ns}\",").unwrap();
        }
        writeln!(m, "    ],\n    names: &[").unwrap();
        for n in &self.names {
            if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!(
                    "name {n:?} has characters outside letters, digits and underscore"
                ));
            }
            writeln!(m, "        \"{n}\",").unwrap();
        }
        writeln!(m, "    ],\n}};").unwrap();
        files.insert(
            "records.rs".into(),
            self.emit_records(&file_header, &root, &publications)?,
        );
        self.emit_json_tables(&mut m, &plan)?;
        let _ = &summary;
        files.insert("mod.rs".into(), m);
        Ok(files)
    }
}

/// The canonical prefix of a namespace in documents.
fn prefix(ns: &str) -> String {
    if ns == "d2Payload" {
        "d2".to_owned()
    } else {
        alias(ns)
    }
}

fn alias(ns: &str) -> String {
    ALIASES
        .iter()
        .find(|(n, _)| *n == ns)
        .map_or_else(|| snake(ns), |(_, a)| (*a).to_owned())
}
