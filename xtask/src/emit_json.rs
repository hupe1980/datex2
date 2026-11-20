//! JSON names, derived from the XML model by the rules of the DATEX II 3.7 JSON mapping, and the
//! check that the official 3.7 JSON schemas agree with every one of them.

use std::collections::BTreeSet;

use super::{Card, Emitter, Field, Key, Kind};
use crate::schema::{JsonDefs, TypeRef};

/// Payload attributes that the JSON mapping moves to the payload's wrapper (and that have no
/// member anywhere else).
pub const HEADER_ATTRIBUTES: [&str; 5] = [
    "modelBaseVersion",
    "extensionName",
    "extensionVersion",
    "profileName",
    "profileVersion",
];

/// Namespaces for which DATEX II publishes no JSON schema.
const NO_OFFICIAL_JSON: [&str; 4] = [
    "messageContainer",
    "exchangeInformation",
    "informationManagement",
    "cisInformation",
];

/// An extension slot type with known members, such as `_LocationExtensionType`.
pub fn is_typed_extension(key: &Key) -> bool {
    key.1.starts_with('_') && key.1.ends_with("ExtensionType")
}

/// The enumeration literal as JSON spells it.
pub fn json_literal(literal: &str) -> String {
    if literal == "_extended" {
        "extendedG".to_owned()
    } else {
        literal.to_owned()
    }
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

impl Emitter<'_> {
    /// The name of the JSON definition that a struct is written as.
    pub(super) fn json_struct_def(key: &Key) -> String {
        match key.1.strip_prefix('_') {
            Some(rest) if is_typed_extension(key) => format!("{rest}G"),
            // An indexed association, such as `_VmsControllerStatusVmsIndexVmsStatus`.
            Some(rest) => {
                let mut c = rest.chars();
                let first = c
                    .next()
                    .map(|f| f.to_lowercase().collect::<String>())
                    .unwrap_or_default();
                format!("{first}{}G", c.as_str())
            }
            None => key.1.clone(),
        }
    }

    /// The JSON member name of a field of `owner`, or `None` when the field has no member of its
    /// own (the payload's header attributes, the collected foreign content).
    pub(super) fn json_member(&self, owner: &Key, f: &Field) -> Result<Option<String>, String> {
        if f.any || f.fixed {
            return Ok(None);
        }
        if f.is_attr {
            return Ok(match f.xml.as_str() {
                "id" => Some("idG".to_owned()),
                "version" => Some("versionG".to_owned()),
                name if HEADER_ATTRIBUTES.contains(&name) => None,
                name => Some(name.to_owned()),
            });
        }
        if let Some(slot) = f.xml.strip_prefix('_') {
            return Ok(Some(format!(
                "{}{}G",
                super::alias(&f.decl_ns),
                upper_first(slot)
            )));
        }
        if is_typed_extension(owner) {
            // Members of a typed extension slot are named after their type.
            let declared = self.declared(owner)?;
            if let Some((TypeRef::Named(_, name), _, _)) =
                declared.get(&(false, f.decl_ns.clone(), f.xml.clone()))
            {
                return Ok(Some(name.clone()));
            }
        }
        Ok(Some(f.xml.clone()))
    }

    /// The member of a polymorphic wrapper that holds `variant`.
    pub(super) fn json_wrapper_key(variant: &Key) -> String {
        format!("{}{}", super::alias(&variant.0), variant.1)
    }

    /// Compares every derived name with the official 3.7 JSON schemas. A difference means the
    /// mapping rules above are incomplete, and stops the generator.
    pub(super) fn check_json_names(
        &self,
        structs: &BTreeSet<Key>,
        polys: &BTreeSet<Key>,
        enums: &BTreeSet<Key>,
        official: &JsonDefs,
    ) -> Result<(), String> {
        let mut problems = Vec::new();
        let mut compare = |what: String,
                           ours: BTreeSet<String>,
                           def: Option<&crate::schema::JsonDef>,
                           list: bool| {
            let Some(def) = def else {
                problems.push(format!(
                    "{what}: no definition in the official JSON schemas"
                ));
                return;
            };
            let theirs: BTreeSet<String> = if list {
                def.literals.clone().unwrap_or_default()
            } else {
                def.members.clone()
            }
            .into_iter()
            .collect();
            if ours != theirs {
                let missing: Vec<_> = theirs.difference(&ours).collect();
                let extra: Vec<_> = ours.difference(&theirs).collect();
                problems.push(format!("{what}: schema has {missing:?} that we do not, we have {extra:?} that it does not"));
            }
        };
        for key in structs
            .iter()
            .filter(|k| !NO_OFFICIAL_JSON.contains(&k.0.as_str()))
        {
            let mut ours = BTreeSet::new();
            for f in self.fields(key)? {
                ours.extend(self.json_member(key, &f)?);
            }
            let def = Self::json_struct_def(key);
            compare(
                format!("{}:{def}", key.0),
                ours,
                official.get(&(key.0.clone(), def.clone())),
                false,
            );
        }
        for key in polys
            .iter()
            .filter(|k| !NO_OFFICIAL_JSON.contains(&k.0.as_str()))
        {
            let mut ours: BTreeSet<String> = self
                .variants(key)?
                .iter()
                .map(Self::json_wrapper_key)
                .collect();
            if *key == ("common".to_owned(), "PayloadPublication".to_owned()) {
                ours.insert("versionG".to_owned());
                ours.extend(HEADER_ATTRIBUTES.iter().map(|a| format!("{a}G")));
            }
            let def = format!("{}G", key.1);
            compare(
                format!("{}:{def}", key.0),
                ours,
                official.get(&(key.0.clone(), def.clone())),
                false,
            );
        }
        // Enumerations no element uses have no JSON definition.
        let mut used = BTreeSet::new();
        for key in structs {
            for f in self.fields(key)? {
                if let Kind::Enum(e) = f.kind {
                    used.insert(e);
                }
            }
        }
        for key in enums
            .iter()
            .filter(|k| used.contains(*k) && !NO_OFFICIAL_JSON.contains(&k.0.as_str()))
        {
            let ours = self.s.simple[key]
                .literals
                .iter()
                .map(|l| json_literal(l))
                .collect();
            compare(
                format!("{}:{}", key.0, key.1),
                ours,
                official.get(key),
                true,
            );
        }
        if problems.is_empty() {
            return Ok(());
        }
        let count = problems.len();
        problems.truncate(25);
        Err(format!(
            "{count} JSON names differ from the official 3.7 JSON schemas:\n  {}",
            problems.join("\n  ")
        ))
    }
}

#[allow(dead_code)]
fn _uses(_: Card, _: Kind) {}

// ---- the German AFIR Recharging profile ---------------------------------------------------------

/// Profile definitions that are a 3.7 definition under another name or in another namespace.
/// A definition by (namespace, name).
type Def = (&'static str, &'static str);

/// Names by definition label: members of an object, or literals of an enumeration.
pub type Names = std::collections::BTreeMap<String, BTreeSet<String>>;

/// A definition of the profile: open, allowed members, required members.
pub type ProfileDef = (bool, BTreeSet<String>, BTreeSet<String>);

const PROFILE_DEF_ALIASES: &[(Def, Def)] = &[
    (
        ("locationExtension", "FacilityLocation"),
        ("afirFacilities", "AfirFacilityLocation"),
    ),
    (
        ("locationExtension", "NutsArea"),
        ("afirFacilities", "NutsArea"),
    ),
    (
        ("commonExtension", "DurationValue"),
        ("afirFacilities", "DurationValue"),
    ),
];

/// Members the profile spells differently: (our definition, our member, the profile's member,
/// the only variant allowed when the profile writes the value without its wrapper).
/// (our definition, our member, the profile's member, the variant written without its wrapper).
type Spelling = (Def, &'static str, &'static str, Option<&'static str>);

const PROFILE_SPELLINGS: &[Spelling] = &[
    (
        ("afirEnergyInfrastructure", "ElectricChargingPoint"),
        "energyProduct",
        "electricEnergy",
        Some("aegiElectricEnergy"),
    ),
    (
        ("afirEnergyInfrastructure", "EnergyInfrastructureStation"),
        "energyProduct",
        "electricEnergy",
        Some("aegiElectricEnergy"),
    ),
    (
        ("locationReferencing", "LocationExtensionTypeG"),
        "AfirFacilityLocation",
        "FacilityLocation",
        None,
    ),
    (
        ("afirFacilities", "NutsArea"),
        "afacNutsAreaExtensionG",
        "locxNutsAreaExtensionG",
        None,
    ),
    (
        ("afirFacilities", "DurationValue"),
        "afacDurationValueExtensionG",
        "comxDurationValueExtensionG",
        None,
    ),
    (
        ("afirEnergyInfrastructure", "EnergyInfrastructureSite"),
        "roamingPlatform",
        "roamingPlatfom",
        None,
    ),
];

/// A member the profile keeps where 3.7 moved it one level down, into an entry of a list:
/// (definition, member, the 3.7 list member whose entries hold it). The profile's
/// `"euVehicleCategory": […]` is 3.7's `"regulatedCharacteristics": [{"euVehicleCategory": […]}]`.
pub const PROFILE_LIFTS: &[(Def, &str, &str)] = &[(
    ("commonExtension", "VehicleCharacteristicsExtended"),
    "euVehicleCategory",
    "regulatedCharacteristics",
)];

/// Enumerations to which the profile adds literals 3.7 does not have. The model holds them as
/// extended values; they are read without a warning and written back as the profile's literal.
const PROFILE_EXTRA_LITERALS: &[(&str, &str)] = &[("commonExtension", "EuVehicleCategoryEnum")];

/// Polymorphic types the profile writes without their wrapper wherever they occur, and the only
/// variant it allows then.
const PROFILE_UNWRAPPED_TYPES: &[(Def, &str)] = &[(("common", "Fault"), "comFault")];

/// Enumerations whose literals the profile spells differently: ours are the profile's plus a suffix.
const PROFILE_LITERAL_SUFFIXES: &[(Def, &str, &str)] = &[(
    ("afirEnergyInfrastructure", "ConnectorTypeEnum"),
    "domestic",
    "Type",
)];

/// Members only the profile has; 3.7 cannot express them, so they are read as unknown members
/// and never written.
const PROFILE_ONLY_MEMBERS: &[(Def, &str)] = &[(
    ("afirEnergyInfrastructure", "EnergyInfrastructureStation"),
    "refillPointByReference",
)];

/// Profile definitions the JSON core handles itself, or that the model has under other names:
/// multilingual text, free extension objects, enumeration objects, closed lists of extended
/// enumeration values (kept as text), and typed references (written as plain references).
fn core_handled(def: &str) -> bool {
    matches!(
        def,
        "MultilingualString" | "MultiLingualStringValue" | "ExtensionTypeG"
    ) || def.ends_with("EnumG")
        || def.ends_with("EnumExtensionTypeG")
}

/// What the generator learned about the profile, in our names.
pub struct ProfileTable {
    /// Our definition: open, allowed members, required members.
    pub defs: std::collections::BTreeMap<String, ProfileDef>,
    /// Our enumeration definition: allowed literals (ours).
    pub literals: std::collections::BTreeMap<String, BTreeSet<String>>,
    /// (our definition, our member, profile member).
    pub renames: Vec<(String, String, String)>,
    /// (our definition, our member, variant key).
    pub unwraps: Vec<(String, String, String)>,
    /// (our enumeration definition, our literal, profile literal).
    pub literal_renames: Vec<(String, String, String)>,
    /// (our definition, member) that 3.7 requires and the profile leaves out of a definition it has.
    pub dropped_required: BTreeSet<(String, String)>,
    /// (our enumeration definition, literal only the profile has).
    pub extra_literals: Vec<(String, String)>,
}

/// A member the profile writes in place of a list, in generated code pieces.
struct Lift {
    id: u32,
    member: String,
    list: String,
    field: usize,
    entry_type: String,
    inner_rust: String,
    read: String,
    write: String,
}

/// The definition name we use for a (namespace, definition) pair.
pub fn def_label(ns: &str, def: &str) -> String {
    format!("{ns}:{def}")
}

impl Emitter<'_> {
    /// Our JSON definitions: label → (members by our spelling) for objects; enumerations listed
    /// with their literals.
    pub(super) fn our_json_defs(
        &self,
        structs: &BTreeSet<Key>,
        polys: &BTreeSet<Key>,
        enums: &BTreeSet<Key>,
    ) -> Result<(Names, Names), String> {
        let mut objects = std::collections::BTreeMap::new();
        for key in structs {
            let mut members = BTreeSet::new();
            for f in self.fields(key)? {
                members.extend(self.json_member(key, &f)?);
            }
            if self.is_reference(key) {
                members.insert("targetClass".to_owned());
            }
            objects.insert(def_label(&key.0, &Self::json_struct_def(key)), members);
        }
        let root = ("common".to_owned(), "PayloadPublication".to_owned());
        for key in polys {
            let mut members: BTreeSet<String> = self
                .variants(key)?
                .iter()
                .map(Self::json_wrapper_key)
                .collect();
            if *key == root {
                members.insert("versionG".to_owned());
                members.extend(HEADER_ATTRIBUTES.iter().map(|a| format!("{a}G")));
            }
            objects.insert(def_label(&key.0, &format!("{}G", key.1)), members);
        }
        let mut literals = std::collections::BTreeMap::new();
        for key in enums {
            literals.insert(
                def_label(&key.0, &key.1),
                self.s.simple[key]
                    .literals
                    .iter()
                    .map(|l| json_literal(l))
                    .collect(),
            );
        }
        Ok((objects, literals))
    }

    /// Whether a struct is a reference type that other reference types collapse into.
    pub(super) fn is_reference(&self, key: &Key) -> bool {
        self.children.get(key).is_some_and(|kids| {
            kids.iter()
                .any(|k| self.collapse(k).is_ok_and(|c| c == *key))
        })
    }

    /// Reads the profile's definitions into our names, and stops on any difference from 3.7 that
    /// the lists above do not explain.
    pub(super) fn profile_table(
        &self,
        structs: &BTreeSet<Key>,
        polys: &BTreeSet<Key>,
        enums: &BTreeSet<Key>,
        profile: &JsonDefs,
    ) -> Result<ProfileTable, String> {
        let (objects, literals) = self.our_json_defs(structs, polys, enums)?;
        let mut table = ProfileTable {
            defs: Default::default(),
            literals: Default::default(),
            renames: Vec::new(),
            unwraps: Vec::new(),
            literal_renames: Vec::new(),
            dropped_required: BTreeSet::new(),
            extra_literals: Vec::new(),
        };
        let mut problems = Vec::new();
        for ((pns, pname), def) in profile {
            if core_handled(pname) || (def.members.is_empty() && def.literals.is_none()) {
                continue;
            }
            let (ons, oname) = PROFILE_DEF_ALIASES
                .iter()
                .find(|(p, _)| p.0 == pns && p.1 == pname)
                .map_or((pns.as_str(), pname.as_str()), |(_, o)| *o);
            let ours = def_label(ons, oname);
            if let Some(profile_literals) = &def.literals {
                let Some(our_literals) = literals.get(&ours) else {
                    problems.push(format!(
                        "{pns}:{pname}: an enumeration the model does not have"
                    ));
                    continue;
                };
                let suffix = PROFILE_LITERAL_SUFFIXES
                    .iter()
                    .find(|(d, _, _)| def_label(d.0, d.1) == ours);
                let mut allowed = BTreeSet::new();
                for literal in profile_literals {
                    let mine = match suffix {
                        Some((_, prefix, suffix))
                            if literal.starts_with(prefix) && !our_literals.contains(literal) =>
                        {
                            let mine = format!("{literal}{suffix}");
                            table.literal_renames.push((
                                ours.clone(),
                                mine.clone(),
                                literal.clone(),
                            ));
                            mine
                        }
                        _ => literal.clone(),
                    };
                    if our_literals.contains(&mine) {
                        allowed.insert(mine);
                    } else if PROFILE_EXTRA_LITERALS.contains(&(ons, oname)) {
                        table.extra_literals.push((ours.clone(), literal.clone()));
                        allowed.insert(literal.clone());
                    } else {
                        problems.push(format!("{pns}:{pname}: literal {literal:?} is not in 3.7"));
                    }
                }
                table.literals.insert(ours, allowed);
                continue;
            }
            let Some(our_members) = objects.get(&ours) else {
                // The wrapper of a polymorphic type that no member of the model uses is never written.
                let unused_wrapper = oname.strip_suffix('G').is_some_and(|base| {
                    let base = (ons.to_owned(), base.to_owned());
                    self.children.contains_key(&base) && !polys.contains(&base)
                });
                // Typed references are written as plain references, which the profile also has.
                let typed_reference = pname.ends_with("ReferenceG");
                if !unused_wrapper
                    && !typed_reference
                    && !matches!(pns.as_str(), "messageContainer" | "exchangeInformation")
                {
                    problems.push(format!(
                        "{pns}:{pname}: a definition the model does not have"
                    ));
                }
                continue;
            };
            let mut allowed = BTreeSet::new();
            let mut required = BTreeSet::new();
            for member in &def.members {
                if PROFILE_ONLY_MEMBERS
                    .iter()
                    .any(|(d, m)| d.0 == ons && d.1 == oname && m == member)
                {
                    // The model cannot hold it. Where the profile requires it, the object cannot
                    // be written for the profile, and the writer says which member is missing.
                    if def.required.contains(member) {
                        required.insert(member.clone());
                    }
                    continue;
                }
                if PROFILE_LIFTS
                    .iter()
                    .any(|(d, m, _)| d.0 == ons && d.1 == oname && m == member)
                {
                    if def.required.contains(member) {
                        required.insert(member.clone());
                    }
                    allowed.insert(member.clone());
                    continue;
                }
                let spelling = PROFILE_SPELLINGS
                    .iter()
                    .find(|(d, _, theirs, _)| d.0 == ons && d.1 == oname && theirs == member);
                let mine = spelling.map_or(member.clone(), |(_, ours, _, _)| (*ours).to_owned());
                if !our_members.contains(&mine) {
                    problems.push(format!("{pns}:{pname}: member {member:?} is neither in 3.7 nor in the spelling list"));
                    continue;
                }
                if let Some((_, ours_member, theirs, variant)) = spelling {
                    table.renames.push((
                        ours.clone(),
                        (*ours_member).to_owned(),
                        (*theirs).to_owned(),
                    ));
                    if let Some(variant) = variant {
                        table.unwraps.push((
                            ours.clone(),
                            (*ours_member).to_owned(),
                            (*variant).to_owned(),
                        ));
                    }
                }
                if def.required.contains(member) {
                    required.insert(mine.clone());
                }
                allowed.insert(mine);
            }
            table.defs.insert(ours, (def.open, allowed, required));
        }
        // A typed extension slot of 3.7 that the profile keeps generic: its `ExtensionTypeG` takes
        // any member, so whatever the slot holds is written as it is.
        for key in structs.iter().filter(|k| is_typed_extension(k)) {
            let label = def_label(&key.0, &Self::json_struct_def(key));
            table
                .defs
                .entry(label)
                .or_insert_with(|| (true, BTreeSet::new(), BTreeSet::new()));
        }
        // Members 3.7 requires that the profile leaves out.
        for key in structs {
            let label = def_label(&key.0, &Self::json_struct_def(key));
            let Some((_, allowed, _)) = table.defs.get(&label) else {
                continue;
            };
            for f in self.fields(key)? {
                if f.card == Card::One
                    && let Some(member) = self.json_member(key, &f)?
                    && !allowed.contains(&member)
                {
                    table.dropped_required.insert((label.clone(), member));
                }
            }
        }
        // Members of an unwrapped type, wherever the profile allows them.
        for key in structs {
            let label = def_label(&key.0, &Self::json_struct_def(key));
            let Some((_, allowed, _)) = table.defs.get(&label) else {
                continue;
            };
            for f in self.fields(key)? {
                if let Kind::Poly(base) = &f.kind
                    && let Some((_, variant)) = PROFILE_UNWRAPPED_TYPES
                        .iter()
                        .find(|(t, _)| t.0 == base.0 && t.1 == base.1)
                    && let Some(member) = self.json_member(key, &f)?
                    && allowed.contains(&member)
                {
                    table
                        .unwraps
                        .push((label.clone(), member, (*variant).to_owned()));
                }
            }
        }
        // The message container's exchange information has no 3.7 JSON schema; its members are
        // checked like any other.
        for (pns, pname) in profile
            .keys()
            .filter(|(ns, _)| matches!(ns.as_str(), "messageContainer" | "exchangeInformation"))
        {
            let def = &profile[&(pns.clone(), pname.clone())];
            if def.members.is_empty() || core_handled(pname) {
                continue;
            }
            let ours = def_label(pns, pname);
            match objects.get(&ours) {
                Some(our_members) => {
                    for member in &def.members {
                        if !our_members.contains(member) {
                            problems.push(format!(
                                "{pns}:{pname}: member {member:?} is not in Exchange 2020"
                            ));
                        }
                    }
                    table.defs.insert(
                        ours,
                        (
                            def.open,
                            def.members.iter().cloned().collect(),
                            def.required.iter().cloned().collect(),
                        ),
                    );
                }
                None => problems.push(format!(
                    "{pns}:{pname}: a definition Exchange 2020 does not have"
                )),
            }
        }
        if problems.is_empty() {
            return Ok(table);
        }
        Err(format!(
            "the Mobilithek profile differs from 3.7 in ways the generator does not know:\n  {}",
            problems.join("\n  ")
        ))
    }
}

// ---- emitting JSON readers and writers ----------------------------------------------------------

use std::fmt::Write as _;

use super::{Prim, prefix, type_name};

/// Names and ids the JSON code is generated with.
pub struct JsonPlan {
    /// Every JSON member name, sorted.
    pub names: Vec<String>,
    /// Every definition label, sorted.
    pub defs: Vec<String>,
    pub profile: ProfileTable,
    /// The publication types, which take their header from the payload wrapper.
    pub publications: BTreeSet<Key>,
}

impl JsonPlan {
    pub fn name(&self, name: &str) -> u32 {
        self.names
            .binary_search_by(|n| n.as_str().cmp(name))
            .unwrap_or_else(|_| panic!("JSON name {name} is in the table")) as u32
    }

    pub fn def(&self, label: &str) -> u32 {
        self.defs
            .binary_search_by(|n| n.as_str().cmp(label))
            .unwrap_or_else(|_| panic!("definition {label} is in the table")) as u32
    }

    /// Profile spellings of a member of a definition: (profile name, variant key when unwrapped).
    fn aliases(&self, def: &str, member: &str) -> Vec<(String, Option<String>)> {
        self.profile
            .renames
            .iter()
            .filter(|(d, m, _)| d == def && m == member)
            .map(|(_, _, theirs)| {
                let variant = self
                    .profile
                    .unwraps
                    .iter()
                    .find(|(d, m, _)| d == def && m == member)
                    .map(|(_, _, v)| v.clone());
                (theirs.clone(), variant)
            })
            .collect()
    }
}

impl Emitter<'_> {
    pub(super) fn json_plan(
        &self,
        structs: &BTreeSet<Key>,
        polys: &BTreeSet<Key>,
        enums: &BTreeSet<Key>,
        profile: ProfileTable,
    ) -> Result<JsonPlan, String> {
        let (objects, literals) = self.our_json_defs(structs, polys, enums)?;
        let mut names: BTreeSet<String> = objects.values().flatten().cloned().collect();
        names.extend(
            [
                "values",
                "value",
                "lang",
                "extendedValueG",
                "targetClass",
                "payload",
                "messageContainer",
                "exchangeInformation",
                "versionG",
            ]
            .map(str::to_owned),
        );
        names.extend(HEADER_ATTRIBUTES.iter().map(|a| format!("{a}G")));
        names.extend(profile.renames.iter().map(|(_, _, theirs)| theirs.clone()));
        names.extend(
            profile
                .defs
                .values()
                .flat_map(|(_, _, required)| required.iter().cloned()),
        );
        let mut defs: Vec<String> = objects.keys().chain(literals.keys()).cloned().collect();
        defs.sort();
        defs.dedup();
        let root = ("common".to_owned(), "PayloadPublication".to_owned());
        Ok(JsonPlan {
            names: names.into_iter().collect(),
            defs,
            profile,
            publications: self.variants(&root)?.into_iter().collect(),
        })
    }

    /// The `targetClass` that JSON requires on a reference whose XML type fixes it.
    fn json_target_class(&self, ty: &TypeRef) -> Result<Option<String>, String> {
        let TypeRef::Named(ns, name) = ty else {
            return Ok(None);
        };
        let mut key = (ns.clone(), name.clone());
        if !self.s.complex.contains_key(&key) {
            return Ok(None);
        }
        let target = self.collapse(&key)?;
        while key != target {
            let c = self.complex(&key)?;
            if let Some(a) = c.attributes.iter().find(|a| a.name == "targetClass") {
                let value = a.fixed_value.clone().unwrap_or_default();
                let class = value.rsplit(':').next().unwrap_or_default().to_owned();
                if !class.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return Err(format!(
                        "{}:{}: target class {value:?} needs escaping",
                        key.0, key.1
                    ));
                }
                return Ok(Some(class));
            }
            key = Self::base_key(c).ok_or("a collapsed type without base")?;
        }
        Ok(None)
    }

    /// Expression of type `Option<T>` that reads the value at the reader's position.
    fn json_read_expr(&self, ns: &str, f: &Field) -> Result<String, String> {
        Ok(match &f.kind {
            Kind::Ext => "r.raw_object()?".into(),
            Kind::Multi => "r.multilingual()?".into(),
            Kind::Enum(_) => format!("r.enumeration::<{}>()?", self.base_type(ns, &f.kind)?),
            Kind::Prim(Prim::Str, _) => "r.string()?".into(),
            Kind::Prim(Prim::DateTime, _) => "r.date_time()?".into(),
            Kind::Prim(Prim::Date, _) => "r.text::<Date>()?".into(),
            Kind::Prim(Prim::Time, _) => "r.text::<Time>()?".into(),
            Kind::Prim(Prim::Bytes, _) => "r.base64()?".into(),
            Kind::Prim(Prim::Bool, _) => "r.boolean()?".into(),
            Kind::Prim(p, _) => format!("r.number::<{}>()?", p.rust()),
            Kind::Struct(_) | Kind::Poly(_) => {
                format!("{}::read_json(r)?", self.base_type(ns, &f.kind)?)
            }
        })
    }

    /// Statements that write the value `v` (a reference) after its member name.
    fn json_write_stmt(&self, f: &Field, ty: &TypeRef, plan: &JsonPlan) -> Result<String, String> {
        Ok(match &f.kind {
            Kind::Ext => "w.extension(v)?;".into(),
            Kind::Multi => "w.multilingual(v)?;".into(),
            Kind::Enum(e) => format!(
                "w.enumeration({}, v.literal())?;",
                plan.def(&def_label(&e.0, &e.1))
            ),
            Kind::Prim(p, _) => {
                let checks = self.facet_checks(ty, *p, &f.xml, "v")?;
                let write = match p {
                    Prim::Str => "w.string(v)?;",
                    Prim::DateTime => "w.date_time(v)?;",
                    Prim::Date => "w.date(v)?;",
                    Prim::Time => "w.time(v)?;",
                    Prim::Bytes => "w.base64(v)?;",
                    Prim::Bool => "w.boolean(*v)?;",
                    Prim::F32 => "w.float32(*v)?;",
                    Prim::F64 => "w.float64(*v)?;",
                    Prim::Dec => "w.decimal(v)?;",
                    Prim::I32 | Prim::I64 | Prim::U64 => "w.integer(*v)?;",
                };
                format!("{checks}{write}")
            }
            Kind::Struct(_) => {
                let class = self
                    .json_target_class(ty)?
                    .map_or("None".to_owned(), |c| format!("Some(\"{c}\")"));
                format!("v.write_json(w, {class})?;")
            }
            Kind::Poly(_) => "v.write_json(w)?;".into(),
        })
    }

    pub(super) fn emit_struct_json(
        &self,
        out: &mut String,
        key: &Key,
        plan: &JsonPlan,
    ) -> Result<(), String> {
        let ns = &key.0;
        let name = type_name(&key.1);
        let label = def_label(ns, &Self::json_struct_def(key));
        let def = plan.def(&label);
        let fields = self.fields(key)?;
        let declared = self.declared(key)?;
        let is_publication = plan.publications.contains(key);
        let typed_extension = is_typed_extension(key);

        // ---- reading ----
        writeln!(out, "impl {name} {{").unwrap();
        writeln!(out, "    pub(crate) fn read_json(r: &mut JsonReader<'_>) -> Result<Option<Self>, Error> {{\n        if !r.begin_object()? {{\n            return Ok(None);\n        }}\n        Self::read_json_members(r, None).map(Some)\n    }}\n").unwrap();
        writeln!(out, "    /// Reads the members of an object that is already open; `pending` is a member key read\n    /// already.\n    pub(crate) fn read_json_members(r: &mut JsonReader<'_>, mut pending: Option<u32>) -> Result<Self, Error> {{").unwrap();
        let kept: Vec<(usize, &Field)> = fields.iter().filter(|f| !f.fixed).enumerate().collect();
        writeln!(
            out,
            "        let mut seen = [false; {}];",
            kept.len().max(1)
        )
        .unwrap();
        let lifts = self.json_lifts(key, &kept, plan)?;
        if !lifts.is_empty() {
            writeln!(out, "        let mut lifted = [false; {}];", lifts.len()).unwrap();
        }
        for (i, f) in &kept {
            writeln!(
                out,
                "        let mut f{i} = {};",
                if f.card == Card::Many || f.any {
                    "Vec::new()"
                } else {
                    "None"
                }
            )
            .unwrap();
        }
        writeln!(out, "        loop {{\n            let key = match pending.take() {{\n                Some(key) => key,\n                None => match r.next_key()? {{\n                    Some(key) => key,\n                    None => break,\n                }},\n            }};\n            match key {{").unwrap();
        let mut required = Vec::new();
        for (i, f) in &kept {
            if f.any {
                continue;
            }
            let Some(member) = self.json_member(key, f)? else {
                continue;
            };
            let boxed = matches!(f.kind, Kind::Struct(_) | Kind::Ext);
            let read = self.json_read_expr(ns, f)?;
            let store = |read: &str| match f.card {
                Card::Many => format!(
                    "let mut l = r.list()?; while r.item(&mut l)? {{ if let Some(v) = {read} {{ f{i}.push(v); }} }}"
                ),
                _ if boxed => format!("f{i} = {read}.map(Box::new);"),
                _ => format!("f{i} = {read};"),
            };
            writeln!(
                out,
                "                {} => if r.first(&mut seen[{i}])? {{ {} }},",
                plan.name(&member),
                store(&read)
            )
            .unwrap();
            for (alias, variant) in plan.aliases(&label, &member) {
                let read = match (&variant, &f.kind) {
                    (Some(variant), Kind::Poly(base)) => {
                        let variants = self.variants(base)?;
                        let names = self.variant_names(base, &variants)?;
                        let (vkey, vname) = variants
                            .iter()
                            .zip(&names)
                            .find(|(v, _)| Self::json_wrapper_key(v) == *variant)
                            .ok_or_else(|| format!("{label}: no variant {variant}"))?;
                        let poly = self.base_type(ns, &f.kind)?;
                        let vtype = self.path(ns, vkey, &type_name(&vkey.1));
                        format!("{vtype}::read_json(r)?.map(|v| {poly}::{vname}(Box::new(v)))")
                    }
                    _ => read.clone(),
                };
                writeln!(
                    out,
                    "                {} => if r.first(&mut seen[{i}])? {{ {} }},",
                    plan.name(&alias),
                    store(&read)
                )
                .unwrap();
            }
            if f.card == Card::One
                && !plan
                    .profile
                    .dropped_required
                    .contains(&(label.clone(), member.clone()))
            {
                required.push((*i, plan.name(&member)));
            }
        }
        for (n, lift) in lifts.iter().enumerate() {
            writeln!(out, "                {} => if r.first(&mut lifted[{n}])? {{ let mut entry = {}::default(); let mut l = r.list()?; while r.item(&mut l)? {{ if let Some(v) = {} {{ entry.{}.push(v); }} }} f{}.insert(0, entry); }},", lift.id, lift.entry_type, lift.read, lift.inner_rust, lift.field).unwrap();
        }
        if self.is_reference(key) {
            writeln!(
                out,
                "                {} => r.skip_silently()?,",
                plan.name("targetClass")
            )
            .unwrap();
        }
        let unknown = match kept.iter().find(|(_, f)| f.any) {
            Some((i, _)) if typed_extension || fields.iter().any(|f| f.any) => {
                format!("f{i}.push(r.raw_member()?)")
            }
            _ => "r.skip_unknown()?".to_owned(),
        };
        writeln!(
            out,
            "                _ => {unknown},\n            }}\n        }}"
        )
        .unwrap();
        for (i, id) in &required {
            writeln!(out, "        r.required(&f{i}, {id});").unwrap();
        }
        writeln!(out, "        Ok(Self {{").unwrap();
        for (i, f) in &kept {
            writeln!(out, "            {}: f{i},", f.rust).unwrap();
        }
        writeln!(out, "        }})\n    }}\n").unwrap();

        // ---- writing ----
        writeln!(out, "    pub(crate) fn write_json(&self, w: &mut JsonWriter<'_>, target_class: Option<&'static str>) -> Result<(), Error> {{").unwrap();
        writeln!(out, "        w.begin_object({def})?;\n        if let Some(class) = target_class {{\n            w.target_class(class)?;\n        }}").unwrap();
        for f in fields.iter().filter(|f| !f.fixed && !f.any) {
            let field = format!("self.{}", f.rust);
            let Some(member) = self.json_member(key, f)? else {
                // A payload attribute: on a publication the payload wrapper writes it; elsewhere
                // JSON has no member for it.
                if !is_publication {
                    writeln!(out, "        if {field}.is_some() {{\n            return Err(w.no_place(\"{}\"));\n        }}", f.xml).unwrap();
                }
                continue;
            };
            let id = plan.name(&member);
            let (ty, min, max) = declared
                .get(&(f.is_attr, f.decl_ns.clone(), f.xml.clone()))
                .ok_or_else(|| format!("{}:{}: no declaration for {}", ns, key.1, f.xml))?
                .clone();
            let write = self.json_write_stmt(f, &ty, plan)?;
            match f.card {
                Card::Many => {
                    let max = if max == u32::MAX { "usize::MAX".to_owned() } else { max.to_string() };
                    if min > 0 || max != "usize::MAX" {
                        writeln!(out, "        w.count(\"{member}\", {field}.len(), {min}, {max})?;").unwrap();
                    }
                    let normal = format!("if !{field}.is_empty() {{\n            w.key({id})?;\n            w.begin_array()?;\n            for v in &{field} {{\n                w.item()?;\n                {write}\n            }}\n            w.end_array()?;\n        }}");
                    match lifts.iter().find(|l| l.list == member) {
                        // The profile writes the one entry's member in place of the list.
                        Some(lift) => writeln!(out, "        if w.profile_active() {{\n            match {field}.as_slice() {{\n                [] => {{}}\n                [one] if *one == {entry} {{ {inner}: one.{inner}.clone(), ..Default::default() }} => {{\n                    w.key({lid})?;\n                    w.begin_array()?;\n                    for v in &one.{inner} {{\n                        w.item()?;\n                        {lwrite}\n                    }}\n                    w.end_array()?;\n                }}\n                _ => return Err(w.lift_refused(\"{member}\", \"{lname}\")),\n            }}\n        }} else {normal}", entry = lift.entry_type, inner = lift.inner_rust, lid = lift.id, lwrite = lift.write, lname = lift.member).unwrap(),
                        None => writeln!(out, "        {normal}").unwrap(),
                    }
                }
                Card::One => {
                    let missing = if f.is_attr { "missing_attr" } else { "missing" };
                    writeln!(out, "        match &{field} {{\n            Some(v) => {{\n                w.key({id})?;\n                {write}\n            }}\n            None => w.absent({id}, w.{missing}(\"{member}\"))?,\n        }}").unwrap();
                }
                Card::Opt => writeln!(out, "        if let Some(v) = &{field} {{\n            w.key({id})?;\n            {write}\n        }}").unwrap(),
            }
        }
        if fields.iter().any(|f| f.any) {
            writeln!(out, "        w.foreign_members(&self.other)?;").unwrap();
        }
        writeln!(out, "        w.end_object()\n    }}\n}}\n").unwrap();
        Ok(())
    }

    /// The profile lifts of a struct, resolved to code pieces.
    fn json_lifts(
        &self,
        key: &Key,
        kept: &[(usize, &Field)],
        plan: &JsonPlan,
    ) -> Result<Vec<Lift>, String> {
        let mut out = Vec::new();
        for (_, member, list) in PROFILE_LIFTS
            .iter()
            .filter(|(d, _, _)| d.0 == key.0 && d.1 == key.1)
        {
            let (field, f) = kept
                .iter()
                .find(|(_, f)| self.json_member(key, f).ok().flatten().as_deref() == Some(*list))
                .ok_or_else(|| {
                    format!(
                        "{}:{}: no member {list} to lift {member} from",
                        key.0, key.1
                    )
                })?;
            let Kind::Struct(entry) = &f.kind else {
                return Err(format!("{list}: entries are not objects"));
            };
            let inner_fields = self.fields(entry)?;
            let inner = inner_fields
                .iter()
                .find(|g| {
                    self.json_member(entry, g).ok().flatten().as_deref() == Some(*member)
                        && g.card == Card::Many
                })
                .ok_or_else(|| format!("{}:{}: no list member {member}", entry.0, entry.1))?;
            let declared = self.declared(entry)?;
            let (ty, _, _) = declared
                .get(&(inner.is_attr, inner.decl_ns.clone(), inner.xml.clone()))
                .ok_or("no declaration")?
                .clone();
            out.push(Lift {
                id: plan.name(member),
                member: (*member).to_owned(),
                list: (*list).to_owned(),
                field: *field,
                entry_type: self.base_type(&key.0, &f.kind)?,
                inner_rust: inner.rust.clone(),
                read: self.json_read_expr(&key.0, inner)?,
                write: self.json_write_stmt(inner, &ty, plan)?,
            });
        }
        Ok(out)
    }

    pub(super) fn emit_poly_json(
        &self,
        out: &mut String,
        key: &Key,
        plan: &JsonPlan,
    ) -> Result<(), String> {
        let ns = &key.0;
        let name = self.poly_name(key)?;
        let variants = self.variants(key)?;
        let names = self.variant_names(key, &variants)?;
        let label = def_label(ns, &format!("{}G", key.1));
        let def = plan.def(&label);
        let root = *key == ("common".to_owned(), "PayloadPublication".to_owned());
        let base_concrete = variants.first() == Some(key);

        writeln!(out, "impl {name} {{").unwrap();
        // ---- reading ----
        writeln!(out, "    pub(crate) fn read_json(r: &mut JsonReader<'_>) -> Result<Option<Self>, Error> {{\n        if !r.begin_object()? {{\n            return Ok(None);\n        }}").unwrap();
        let arms: Vec<(String, String)> = variants
            .iter()
            .zip(&names)
            .map(|(v, n)| {
                (
                    plan.name(&Self::json_wrapper_key(v)).to_string(),
                    format!(
                        "{}::read_json(r)?.map(|v| Self::{n}(Box::new(v)))",
                        self.path(ns, v, &type_name(&v.1))
                    ),
                )
            })
            .collect();
        if root {
            let header: Vec<u32> = HEADER_ATTRIBUTES
                .iter()
                .map(|a| plan.name(&format!("{a}G")))
                .collect();
            writeln!(out, "        let mut header: [Option<String>; 4] = Default::default();\n        let mut seen = [false; 5];\n        let mut value = None;\n        while let Some(key) = r.next_key()? {{\n            match key {{").unwrap();
            writeln!(
                out,
                "                {} | {} => r.skip_silently()?,",
                plan.name("versionG"),
                header[0]
            )
            .unwrap();
            for (i, id) in header[1..].iter().enumerate() {
                writeln!(out, "                {id} => if r.first(&mut seen[{i}])? {{ header[{i}] = r.string()?; }},").unwrap();
            }
            for (id, read) in &arms {
                writeln!(
                    out,
                    "                {id} => if r.first(&mut seen[4])? {{ value = {read}; }},"
                )
                .unwrap();
            }
            writeln!(
                out,
                "                _ => r.skip_unknown()?,\n            }}\n        }}"
            )
            .unwrap();
            writeln!(out, "        if !seen[4] {{\n            return Err(r.error(ErrorKind::UnknownPublication, \"a publication member such as \\\"aegiEnergyInfrastructureTablePublication\\\"\", \"none\"));\n        }}").unwrap();
            writeln!(out, "        let [extension_name, extension_version, profile_name, profile_version] = header;\n        if let Some(v) = &mut value {{\n            match v {{").unwrap();
            for n in &names {
                writeln!(out, "                Self::{n}(v) => {{ v.extension_name = extension_name; v.extension_version = extension_version; v.profile_name = profile_name; v.profile_version = profile_version; }}").unwrap();
            }
            writeln!(
                out,
                "            }}\n        }}\n        Ok(value)\n    }}\n"
            )
            .unwrap();
        } else {
            writeln!(out, "        let mut pending = r.next_key()?;").unwrap();
            if base_concrete {
                let ids: Vec<&str> = arms.iter().map(|(id, _)| id.as_str()).collect();
                writeln!(out, "        // The concrete base may come without its wrapper.\n        if let Some(key) = pending {{\n            if !matches!(key, {}) {{\n                return {}::read_json_members(r, Some(key)).map(|v| Some(Self::{}(Box::new(v))));\n            }}\n        }}", ids.join(" | "), type_name(&key.1), names[0]).unwrap();
            }
            writeln!(out, "        let mut seen = false;\n        let mut value = None;\n        loop {{\n            let key = match pending.take() {{\n                Some(key) => key,\n                None => match r.next_key()? {{\n                    Some(key) => key,\n                    None => break,\n                }},\n            }};\n            match key {{").unwrap();
            for (id, read) in &arms {
                writeln!(
                    out,
                    "                {id} => if r.first(&mut seen)? {{ value = {read}; }},"
                )
                .unwrap();
            }
            writeln!(
                out,
                "                _ => r.skip_unknown()?,\n            }}\n        }}"
            )
            .unwrap();
            writeln!(out, "        if !seen {{\n            r.warn(WarningKind::UnknownType, \"no known kind of {} in its wrapper\".to_owned());\n        }}\n        Ok(value)\n    }}\n", key.1).unwrap();
        }
        // ---- writing ----
        writeln!(
            out,
            "    pub(crate) fn write_json(&self, w: &mut JsonWriter<'_>) -> Result<(), Error> {{"
        )
        .unwrap();
        if root {
            writeln!(
                out,
                "        w.begin_object({def})?;\n        let header = match self {{"
            )
            .unwrap();
            for n in &names {
                writeln!(out, "            Self::{n}(v) => [v.extension_name.as_deref(), v.extension_version.as_deref(), v.profile_name.as_deref(), v.profile_version.as_deref()],").unwrap();
            }
            writeln!(
                out,
                "        }};\n        w.payload_header(header)?;\n        match self {{"
            )
            .unwrap();
            for (v, n) in variants.iter().zip(&names) {
                writeln!(out, "            Self::{n}(v) => {{\n                w.key({})?;\n                v.write_json(w, None)?;\n            }}", plan.name(&Self::json_wrapper_key(v))).unwrap();
            }
            writeln!(out, "        }}\n        w.end_object()\n    }}\n}}\n").unwrap();
        } else {
            writeln!(out, "        match self {{").unwrap();
            for (v, n) in variants.iter().zip(&names) {
                let id = plan.name(&Self::json_wrapper_key(v));
                writeln!(out, "            Self::{n}(v) => {{\n                if w.wrapper({id})? {{\n                    w.begin_object({def})?;\n                    w.key({id})?;\n                    v.write_json(w, None)?;\n                    w.end_object()\n                }} else {{\n                    v.write_json(w, None)\n                }}\n            }}").unwrap();
            }
            writeln!(out, "        }}\n    }}\n}}\n").unwrap();
        }
        Ok(())
    }

    /// The `from_alias` method of an enumeration whose literals the profile spells differently.
    pub(super) fn json_enum_aliases(&self, key: &Key, plan: &JsonPlan) -> String {
        let label = def_label(&key.0, &key.1);
        let renames: Vec<&(String, String, String)> = plan
            .profile
            .literal_renames
            .iter()
            .filter(|(d, _, _)| *d == label)
            .collect();
        let extras: Vec<&String> = plan
            .profile
            .extra_literals
            .iter()
            .filter(|(d, _)| *d == label)
            .map(|(_, l)| l)
            .collect();
        if renames.is_empty() && extras.is_empty() {
            return String::new();
        }
        let mut out = String::from(
            "\n    fn from_alias(literal: &str) -> Option<Self> {\n        match literal {\n",
        );
        for (_, ours, theirs) in renames {
            writeln!(
                out,
                "            \"{theirs}\" => Some(Self::{}),",
                super::pascal(ours)
            )
            .unwrap();
        }
        for literal in extras {
            writeln!(
                out,
                "            \"{literal}\" => Some(Self::Extended(literal.to_owned())),"
            )
            .unwrap();
        }
        out.push_str("            _ => None,\n        }\n    }\n");
        out
    }

    /// The JSON tables and the profile, for `mod.rs`.
    pub(super) fn emit_json_tables(&self, m: &mut String, plan: &JsonPlan) -> Result<(), String> {
        let header: Vec<String> = HEADER_ATTRIBUTES
            .iter()
            .map(|a| plan.name(&format!("{a}G")).to_string())
            .collect();
        writeln!(m, "\n/// Every JSON member name, sorted, and the ids the JSON core needs.\npub(crate) static JSON: JsonTables = JsonTables {{\n    values: {},\n    value: {},\n    lang: {},\n    extended_value: {},\n    target_class: {},\n    header: [{}],\n    names: &[", plan.name("values"), plan.name("value"), plan.name("lang"), plan.name("extendedValueG"), plan.name("targetClass"), header.join(", ")).unwrap();
        for n in &plan.names {
            if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!(
                    "JSON name {n:?} has characters outside letters, digits and underscore"
                ));
            }
            writeln!(m, "        \"{n}\",").unwrap();
        }
        writeln!(m, "    ],\n}};\n\n/// Every JSON definition, by namespace and name; the index is the definition id.\npub(crate) static JSON_DEFS: &[&str] = &[").unwrap();
        for d in &plan.defs {
            writeln!(m, "    \"{d}\",").unwrap();
        }
        writeln!(m, "];\n\n/// The German AFIR Recharging profile 01-00-00 rev. 2, from its published schemas.\npub(crate) static MOBILITHEK: Profile = Profile {{\n    name: \"AFIR Energy Infrastructure\",\n    version: \"01-00-00\",\n    defs: &[").unwrap();
        let mut defs: Vec<(u32, &ProfileDef)> = plan
            .profile
            .defs
            .iter()
            .map(|(d, v)| (plan.def(d), v))
            .collect();
        defs.sort_by_key(|d| d.0);
        for (id, (open, allowed, required)) in defs {
            let ids = |set: &BTreeSet<String>| {
                set.iter()
                    .map(|m| plan.name(m).to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            writeln!(
                m,
                "        ({id}, {open}, &[{}], &[{}]),",
                ids(allowed),
                ids(required)
            )
            .unwrap();
        }
        writeln!(m, "    ],\n    literals: &[").unwrap();
        let mut literals: Vec<(u32, &BTreeSet<String>)> = plan
            .profile
            .literals
            .iter()
            .map(|(d, v)| (plan.def(d), v))
            .collect();
        literals.sort_by_key(|l| l.0);
        for (id, allowed) in literals {
            // Literals are written as they are in the model: the XML spelling of `_extended`.
            let list: Vec<String> = allowed
                .iter()
                .map(|l| format!("\"{}\"", if l == "extendedG" { "_extended" } else { l }))
                .collect();
            writeln!(m, "        ({id}, &[{}]),", list.join(", ")).unwrap();
        }
        writeln!(m, "    ],\n    renames: &[").unwrap();
        for (d, ours, theirs) in &plan.profile.renames {
            writeln!(
                m,
                "        ({}, {}, {}),",
                plan.def(d),
                plan.name(ours),
                plan.name(theirs)
            )
            .unwrap();
        }
        writeln!(m, "    ],\n    unwraps: &[").unwrap();
        for (d, member, variant) in &plan.profile.unwraps {
            writeln!(
                m,
                "        ({}, {}, {}),",
                plan.def(d),
                plan.name(member),
                plan.name(variant)
            )
            .unwrap();
        }
        writeln!(m, "    ],\n    literal_renames: &[").unwrap();
        for (d, ours, theirs) in &plan.profile.literal_renames {
            writeln!(m, "        ({}, \"{ours}\", \"{theirs}\"),", plan.def(d)).unwrap();
        }
        writeln!(m, "    ],\n}};").unwrap();
        Ok(())
    }
}

#[allow(dead_code)]
fn _prefix_in_use(ns: &str) -> String {
    prefix(ns)
}
