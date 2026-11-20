//! Reads the XSD files into a small model. The DATEX II schemas use a narrow dialect; anything
//! outside it stops the generator instead of being guessed at.

use std::collections::BTreeMap;
use std::path::Path;

const XS: &str = "http://www.w3.org/2001/XMLSchema";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeRef {
    /// A built-in XML Schema type, by local name.
    Xs(String),
    /// A named type: namespace name (the part after the common prefix) and type name.
    Named(String, String),
}

#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub ty: TypeRef,
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub ty: TypeRef,
    pub required: bool,
    pub fixed: bool,
    /// The fixed value, when the attribute has one.
    pub fixed_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Complex {
    pub ns: String,
    pub name: String,
    pub is_abstract: bool,
    pub base: Option<TypeRef>,
    pub elements: Vec<Element>,
    pub attributes: Vec<Attribute>,
    /// `simpleContent` base: this type wraps a simple type and adds attributes.
    pub simple_base: Option<TypeRef>,
    /// The sequence ends in `xs:any`.
    pub any: bool,
}

#[derive(Debug, Clone)]
pub struct Simple {
    pub name: String,
    pub base: TypeRef,
    pub literals: Vec<String>,
    /// Facets other than enumeration: name and value, as declared on this type.
    pub facets: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct Schema {
    /// Namespace names in file order, for example `common`.
    pub namespaces: Vec<String>,
    pub complex: BTreeMap<(String, String), Complex>,
    pub simple: BTreeMap<(String, String), Simple>,
}

fn fail(file: &Path, what: impl std::fmt::Display) -> String {
    format!("{}: {what}", file.display())
}

fn children<'a, 'i>(node: roxmltree::Node<'a, 'i>) -> Vec<(String, roxmltree::Node<'a, 'i>)> {
    node.children()
        .filter(|n| n.is_element())
        .map(|n| (n.tag_name().name().to_owned(), n))
        .collect()
}

pub fn parse(schema: &mut Schema, file: &Path, text: &str, base: &str) -> Result<(), String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| fail(file, e))?;
    let root = doc.root_element();
    let target = root
        .attribute("targetNamespace")
        .ok_or_else(|| fail(file, "no targetNamespace"))?;
    let ns = target
        .strip_prefix(base)
        .ok_or_else(|| fail(file, format!("namespace {target} is outside {base}")))?
        .to_owned();
    if root.attribute("elementFormDefault") != Some("qualified")
        || root.attribute("attributeFormDefault") != Some("unqualified")
    {
        return Err(fail(
            file,
            "expected qualified elements and unqualified attributes",
        ));
    }
    schema.namespaces.push(ns.clone());

    let type_ref = |node: roxmltree::Node, value: &str| -> Result<TypeRef, String> {
        let (prefix, local) = value.split_once(':').unwrap_or(("", value));
        let uri = node
            .lookup_namespace_uri(if prefix.is_empty() {
                None
            } else {
                Some(prefix)
            })
            .ok_or_else(|| fail(file, format!("unbound prefix in {value}")))?;
        if uri == XS {
            Ok(TypeRef::Xs(local.to_owned()))
        } else {
            let ns = uri
                .strip_prefix(base)
                .ok_or_else(|| fail(file, format!("type {value} is outside {base}")))?;
            Ok(TypeRef::Named(ns.to_owned(), local.to_owned()))
        }
    };
    let occurs = |node: roxmltree::Node, attr: &str| -> Result<u32, String> {
        match node.attribute(attr) {
            None => Ok(1),
            Some("unbounded") => Ok(u32::MAX),
            Some(v) => v.parse().map_err(|_| fail(file, format!("bad {attr}={v}"))),
        }
    };

    for (tag, node) in children(root) {
        let name = node.attribute("name").unwrap_or_default().to_owned();
        match tag.as_str() {
            "import" => {}
            // The single global element is the payload root; the reader knows it by name.
            "element" => {}
            "simpleType" => {
                let kids = children(node);
                let [(t, restriction)] = kids.as_slice() else {
                    return Err(fail(
                        file,
                        format!("simpleType {name}: expected one restriction"),
                    ));
                };
                if t != "restriction" {
                    return Err(fail(file, format!("simpleType {name}: unsupported {t}")));
                }
                let base_ref = type_ref(
                    *restriction,
                    restriction
                        .attribute("base")
                        .ok_or_else(|| fail(file, format!("simpleType {name}: no base")))?,
                )?;
                let mut literals = Vec::new();
                let mut facets = Vec::new();
                for (facet, n) in children(*restriction) {
                    match facet.as_str() {
                        "enumeration" => {
                            literals.push(n.attribute("value").unwrap_or_default().to_owned())
                        }
                        // The reader is tolerant and ignores these; the writer enforces them.
                        "pattern" | "maxLength" | "minInclusive" | "maxInclusive"
                        | "totalDigits" | "fractionDigits" => facets.push((
                            facet.clone(),
                            n.attribute("value").unwrap_or_default().to_owned(),
                        )),
                        other => {
                            return Err(fail(
                                file,
                                format!("simpleType {name}: unsupported facet {other}"),
                            ));
                        }
                    }
                }
                schema.simple.insert(
                    (ns.clone(), name.clone()),
                    Simple {
                        name,
                        base: base_ref,
                        literals,
                        facets,
                    },
                );
            }
            "complexType" => {
                let mut ct = Complex {
                    ns: ns.clone(),
                    name: name.clone(),
                    is_abstract: node.attribute("abstract") == Some("true"),
                    base: None,
                    elements: Vec::new(),
                    attributes: Vec::new(),
                    simple_base: None,
                    any: false,
                };
                let mut body = node;
                for (t, n) in children(node) {
                    match t.as_str() {
                        "complexContent" | "simpleContent" => {
                            let kids = children(n);
                            let [(e, ext)] = kids.as_slice() else {
                                return Err(fail(file, format!("{name}: expected one extension")));
                            };
                            if e != "extension" {
                                return Err(fail(file, format!("{name}: unsupported {e}")));
                            }
                            let base_ref = type_ref(
                                *ext,
                                ext.attribute("base").ok_or_else(|| {
                                    fail(file, format!("{name}: extension without base"))
                                })?,
                            )?;
                            if t == "complexContent" {
                                ct.base = Some(base_ref)
                            } else {
                                ct.simple_base = Some(base_ref)
                            }
                            body = *ext;
                        }
                        "sequence" | "attribute" => {}
                        other => return Err(fail(file, format!("{name}: unsupported {other}"))),
                    }
                }
                for (t, n) in children(body) {
                    match t.as_str() {
                        "sequence" => {
                            for (e, el) in children(n) {
                                match e.as_str() {
                                    "element" => {
                                        let el_name = el.attribute("name").ok_or_else(|| {
                                            fail(file, format!("{name}: element without name"))
                                        })?;
                                        let ty = match el.attribute("type") {
                                            Some(t) => type_ref(el, t)?,
                                            // The one anonymous type in the schema set; the reader handles it by hand.
                                            None if name == "MultilingualString"
                                                && el_name == "values" =>
                                            {
                                                TypeRef::Xs("anonymous".to_owned())
                                            }
                                            None => {
                                                return Err(fail(
                                                    file,
                                                    format!("{name}/{el_name}: anonymous type"),
                                                ));
                                            }
                                        };
                                        for (c, _) in children(el) {
                                            // Identity constraints are not enforced.
                                            if !matches!(c.as_str(), "unique" | "complexType") {
                                                return Err(fail(
                                                    file,
                                                    format!("{name}/{el_name}: unsupported {c}"),
                                                ));
                                            }
                                        }
                                        ct.elements.push(Element {
                                            name: el_name.to_owned(),
                                            ty,
                                            min: occurs(el, "minOccurs")?,
                                            max: occurs(el, "maxOccurs")?,
                                        });
                                    }
                                    "any" => ct.any = true,
                                    other => {
                                        return Err(fail(
                                            file,
                                            format!("{name}: unsupported {other} in sequence"),
                                        ));
                                    }
                                }
                            }
                        }
                        "attribute" => {
                            let a_name = n.attribute("name").ok_or_else(|| {
                                fail(file, format!("{name}: attribute without name"))
                            })?;
                            let ty = type_ref(
                                n,
                                n.attribute("type").ok_or_else(|| {
                                    fail(file, format!("{name}/@{a_name}: no type"))
                                })?,
                            )?;
                            ct.attributes.push(Attribute {
                                name: a_name.to_owned(),
                                ty,
                                required: n.attribute("use") == Some("required"),
                                fixed: n.attribute("fixed").is_some(),
                                fixed_value: n.attribute("fixed").map(str::to_owned),
                            });
                        }
                        "complexContent" | "simpleContent" => {}
                        other => return Err(fail(file, format!("{name}: unsupported {other}"))),
                    }
                }
                schema.complex.insert((ns.clone(), name), ct);
            }
            other => return Err(fail(file, format!("unsupported top-level {other}"))),
        }
    }
    Ok(())
}

/// One definition of a JSON schema, reduced to what the generator compares and enforces.
#[derive(Debug, Clone, Default)]
pub struct JsonDef {
    /// Member names, in schema order.
    pub members: Vec<String>,
    pub required: Vec<String>,
    /// The literals, when the definition is an enumeration.
    pub literals: Option<Vec<String>>,
    /// Whether members other than the listed ones are allowed (`additionalProperties` is not
    /// `false`).
    pub open: bool,
}

/// Definitions by (namespace, name), from JSON schema files named `DATEXII_3_{Namespace}.json`.
/// The same definition in several files of one set is merged: members, required members and
/// literals are united.
pub type JsonDefs = BTreeMap<(String, String), JsonDef>;

pub fn json_definitions(files: &[(String, String)]) -> Result<JsonDefs, String> {
    let mut out = JsonDefs::new();
    for (name, text) in files {
        let stem = name.rsplit('/').next().unwrap_or(name);
        let ns = stem
            .strip_prefix("DATEXII_3_")
            .and_then(|s| s.strip_suffix(".json"))
            .ok_or_else(|| format!("{name}: not a DATEX II JSON schema file name"))?;
        let ns = format!("{}{}", ns[..1].to_lowercase(), &ns[1..]);
        let doc: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        let defs = doc
            .get("$defs")
            .or_else(|| doc.get("definitions"))
            .and_then(|d| d.as_object())
            .ok_or_else(|| format!("{name}: no definitions"))?;
        let strings = |v: Option<&serde_json::Value>| -> Vec<String> {
            v.and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        for (def_name, def) in defs {
            let entry = out.entry((ns.clone(), def_name.clone())).or_default();
            entry.open |= def.get("additionalProperties").and_then(|v| v.as_bool()) != Some(false);
            if let Some(props) = def.get("properties").and_then(|p| p.as_object()) {
                for member in props.keys() {
                    if !entry.members.contains(member) {
                        entry.members.push(member.clone());
                    }
                }
            }
            for member in strings(def.get("required")) {
                if !entry.required.contains(&member) {
                    entry.required.push(member);
                }
            }
            if def.get("enum").is_some() {
                let literals = entry.literals.get_or_insert_with(Vec::new);
                for literal in strings(def.get("enum")) {
                    if !literals.contains(&literal) {
                        literals.push(literal);
                    }
                }
            }
        }
    }
    Ok(out)
}
