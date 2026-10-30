//! Source-only native mixin bundles. Reading a bundle never executes its code.
use super::{Facts, valid_id};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

pub const MANIFEST: &str = "horizon-native.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub game: String,
    /// Exact source revisions tested together, never mutable tags.
    pub horizon: String,
    pub mixins: String,
    /// The version of Horizon's native hook points (`game/native_hooks.rs`,
    /// `NATIVE_HOOKS_API`) a mod that hooks nothing else needs: the launcher
    /// builds it on any Horizon with that version or a later one, and
    /// `horizon` is only the revision it was tested on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<u32>,
    /// Other native mods this one needs, by id, with the versions it takes
    /// ([`version_matches`]): the launcher builds it only with them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub depends: BTreeMap<String, String>,
    /// Native mods this one can't be built with, by id, with the versions
    /// that break it (`*` for all): the launcher refuses the pair.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub breaks: BTreeMap<String, String>,
}

/// Whether `version` is one `requirement` takes: `*` any, `>=1.2.0` that
/// one or a later one, else exactly it. Versions compare by their dotted
/// numbers (`1.10` after `1.9`); a `-` or `+` suffix is left out.
pub fn version_matches(requirement: &str, version: &str) -> bool {
    fn numbers(v: &str) -> Option<Vec<u64>> {
        let core = v.trim().split(['-', '+']).next()?;
        core.split('.').map(|n| n.parse().ok()).collect()
    }
    let requirement = requirement.trim();
    if requirement == "*" {
        return true;
    }
    if let Some(min) = requirement.strip_prefix(">=") {
        return match (numbers(version), numbers(min)) {
            (Some(mut v), Some(mut m)) => {
                let len = v.len().max(m.len());
                v.resize(len, 0);
                m.resize(len, 0);
                v >= m
            }
            _ => false,
        };
    }
    requirement == version.trim()
}

/// Whether a requirement is one [`version_matches`] reads.
fn valid_requirement(requirement: &str) -> bool {
    let r = requirement.trim();
    match r.strip_prefix(">=") {
        Some(min) => min.split(['-', '+']).next().is_some_and(|core| {
            core.split('.')
                .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        }),
        None => !r.is_empty(),
    }
}

/// What the mods (id, version, needs) are missing from one another, one line
/// each; empty when every need is met.
pub fn unmet_dependencies<'a>(
    mods: impl IntoIterator<Item = (&'a str, &'a str, &'a BTreeMap<String, String>)>,
) -> Vec<String> {
    let mods: Vec<_> = mods.into_iter().collect();
    let mut unmet = Vec::new();
    for (id, _, depends) in &mods {
        for (need, requirement) in *depends {
            match mods.iter().find(|(other, _, _)| other == need) {
                None => unmet.push(format!(
                    "{id} needs {need} {requirement}, which isn't turned on."
                )),
                Some((_, version, _)) if !version_matches(requirement, version) => unmet.push(
                    format!("{id} needs {need} {requirement}, and {version} is installed."),
                ),
                Some(_) => {}
            }
        }
    }
    unmet
}

/// The mods (id, version, breaks) that one of them can't be built with, one
/// line each; empty when none is.
pub fn broken_pairs<'a>(
    mods: impl IntoIterator<Item = (&'a str, &'a str, &'a BTreeMap<String, String>)>,
) -> Vec<String> {
    let mods: Vec<_> = mods.into_iter().collect();
    let mut broken = Vec::new();
    for (id, _, breaks) in &mods {
        for (other, requirement) in *breaks {
            if let Some((_, version, _)) = mods.iter().find(|(o, _, _)| o == other)
                && version_matches(requirement, version)
            {
                broken.push(format!(
                    "{id} doesn't work with {other} {version} ({requirement})."
                ));
            }
        }
    }
    broken
}

pub fn revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Manifest {
    pub fn check(&self) -> Result<(), String> {
        if self.schema != 1
            || !valid_id(&self.id)
            || self.version.is_empty()
            || self.game.is_empty()
            || !revision(&self.horizon)
            || !revision(&self.mixins)
            || self.hooks == Some(0)
            || self
                .depends
                .iter()
                .chain(&self.breaks)
                .any(|(id, r)| !valid_id(id) || *id == self.id || !valid_requirement(r))
        {
            return Err(
                "invalid native manifest: schema 1, id, version, game and exact revisions required"
                    .into(),
            );
        }
        Ok(())
    }

    /// Carries the exact native compatibility contract in the index's API field.
    pub fn api(&self) -> String {
        format!("native-1/{}/{}", self.horizon, self.mixins)
    }

    pub fn facts(&self) -> Facts {
        Facts {
            id: Some(self.id.clone()),
            version: Some(self.version.clone()),
            api: self.api(),
            game: Some(self.game.clone()),
            ..Facts::default()
        }
    }
}

pub struct Bundle {
    pub manifest: Manifest,
    /// Validated relative paths: Rust and TOML, plus files under `mixins/<id>/data/`.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Bundle {
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        if zip.len() > 512 {
            return Err("native bundle has too many entries".into());
        }
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_owned();
            if entry.is_dir() {
                continue;
            }
            if entry
                .unix_mode()
                .is_some_and(|m| m & 0o170000 != 0 && m & 0o170000 != 0o100000)
                || name.contains('\\')
                || name
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == "..")
                || name.starts_with('/')
                || name.contains(':')
            {
                return Err(format!("invalid native bundle path: {name}"));
            }
            let mut bytes = Vec::new();
            (&mut entry)
                .take(super::MAX_FILE + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            total += bytes.len() as u64;
            if total > super::MAX_FILE {
                return Err("native bundle expands beyond 64 MiB".into());
            }
            if files.insert(name, bytes).is_some() {
                return Err("duplicate native bundle path".into());
            }
        }
        let manifest: Manifest =
            serde_json::from_slice(files.get(MANIFEST).ok_or("missing horizon-native.json")?)
                .map_err(|e| e.to_string())?;
        manifest.check()?;
        let prefix = format!("mixins/{}/", manifest.id);
        let data_prefix = format!("{prefix}data/");
        for name in files.keys().filter(|n| n.as_str() != MANIFEST) {
            if !name.starts_with(&prefix)
                || !(name.ends_with(".rs")
                    || name.ends_with(".toml")
                    || name.starts_with(&data_prefix))
            {
                return Err(format!("unexpected native source: {name}"));
            }
        }
        let source = files
            .get(&format!("{prefix}horizon-mixin.toml"))
            .ok_or("missing horizon-mixin.toml")?;
        let source: toml::Value =
            toml::from_str(std::str::from_utf8(source).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if source
            .get("mod")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            != Some(&manifest.id)
            || source
                .get("mod")
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                != Some(&manifest.version)
            || source
                .get("compatibility")
                .and_then(|v| v.get("horizon_revision"))
                .and_then(|v| v.as_str())
                != Some(&manifest.horizon)
        {
            return Err("native manifest and mixin identity/revision differ".into());
        }
        Ok(Self { manifest, files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manifest() -> Manifest {
        Manifest {
            schema: 1,
            id: "veinminer".into(),
            version: "0.3.0".into(),
            game: "26.3".into(),
            horizon: "a".repeat(40),
            mixins: "b".repeat(40),
            hooks: None,
            depends: BTreeMap::new(),
            breaks: BTreeMap::new(),
        }
    }
    fn read(name: &str, extras: &[(&str, &[u8])], mixin: &str) -> Result<Bundle, String> {
        let path =
            std::env::temp_dir().join(format!("native-bundle-{name}-{}.hnmod", std::process::id()));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file(MANIFEST, options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest()).unwrap())
            .unwrap();
        zip.start_file("mixins/veinminer/horizon-mixin.toml", options)
            .unwrap();
        zip.write_all(mixin.as_bytes()).unwrap();
        for (name, bytes) in extras {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let result = Bundle::read(&path);
        std::fs::remove_file(path).unwrap();
        result
    }
    fn mixin() -> String {
        format!(
            "[mod]\nid = \"veinminer\"\nversion = \"0.3.0\"\n[compatibility]\nhorizon_revision = \"{}\"\n",
            "a".repeat(40)
        )
    }
    #[test]
    fn source_bundle_carries_exact_compatibility_into_index_facts() {
        let bundle = read(
            "valid",
            &[("mixins/veinminer/hook.rs", b"fn hook() {}")],
            &mixin(),
        )
        .unwrap();
        assert_eq!(bundle.manifest, manifest());
        let facts = bundle.manifest.facts();
        assert_eq!(
            facts.api,
            format!("native-1/{}/{}", "a".repeat(40), "b".repeat(40))
        );
        assert_eq!(facts.id.as_deref(), Some("veinminer"));
        assert!(facts.capabilities.is_empty());
    }
    #[test]
    fn accepts_binary_data_in_the_bundle_data_directory() {
        let bytes = [0, 255, 42];
        let bundle = read(
            "data",
            &[("mixins/veinminer/data/nested/table.bin", &bytes)],
            &mixin(),
        )
        .unwrap();
        assert_eq!(
            bundle.files["mixins/veinminer/data/nested/table.bin"],
            bytes
        );
    }
    #[test]
    fn rejects_path_traversal_and_non_source_payloads() {
        for (i, name) in [
            "../escape.rs",
            "/absolute.rs",
            "mixins/other/hook.rs",
            "mixins/veinminer/code.dll",
            "mixins/veinminer/data/../code.dll",
            "mixins/veinminer/database/file.bin",
            "mixins/other/data/file.bin",
            "mixins\\veinminer\\hook.rs",
        ]
        .iter()
        .enumerate()
        {
            assert!(read(&format!("bad-path-{i}"), &[(name, b"bad")], &mixin()).is_err());
        }
    }
    #[test]
    fn rejects_missing_or_mismatched_mixin_identity_without_panicking() {
        assert!(read("missing-mod", &[], "[unrelated]\nvalue = 1").is_err());
        assert!(
            read(
                "wrong-revision",
                &[],
                &mixin().replace(&"a".repeat(40), &"c".repeat(40))
            )
            .is_err()
        );
        assert!(read("wrong-version", &[], &mixin().replace("0.3.0", "0.2.0")).is_err());
        assert!(
            Manifest {
                horizon: "nightly".into(),
                ..manifest()
            }
            .check()
            .is_err()
        );
    }
    #[test]
    fn hooks_is_optional_and_kept() {
        let hooked = Manifest {
            hooks: Some(1),
            ..manifest()
        };
        assert!(hooked.check().is_ok());
        let text = serde_json::to_string(&hooked).unwrap();
        assert!(text.contains("\"hooks\":1"), "{text}");
        assert_eq!(serde_json::from_str::<Manifest>(&text).unwrap(), hooked);
        let plain = serde_json::to_string(&manifest()).unwrap();
        assert!(!plain.contains("hooks"), "{plain}");
        let zero = Manifest {
            hooks: Some(0),
            ..manifest()
        };
        assert!(zero.check().is_err());
    }

    #[test]
    fn dependencies_name_versions_and_must_be_there() {
        assert!(version_matches("*", "0.1.0"));
        assert!(version_matches(">=1.2", "1.10.0"));
        assert!(!version_matches(">=1.10", "1.9.9"));
        assert!(version_matches(">=1.2.0", "1.2.0-beta"));
        assert!(version_matches("0.3.0", "0.3.0") && !version_matches("0.3.0", "0.3.1"));
        let needs = BTreeMap::from([("lib".to_owned(), ">=2.0".to_owned())]);
        let none = BTreeMap::new();
        assert_eq!(unmet_dependencies([("a", "1.0", &needs)]).len(), 1);
        assert_eq!(
            unmet_dependencies([("a", "1.0", &needs), ("lib", "1.5", &none)]).len(),
            1
        );
        assert!(unmet_dependencies([("a", "1.0", &needs), ("lib", "2.1", &none)]).is_empty());
        let mut m = manifest();
        m.depends = BTreeMap::from([("lib".to_owned(), ">=x".to_owned())]);
        assert!(m.check().is_err());
        m.depends = BTreeMap::from([("lib".to_owned(), ">=1.0".to_owned())]);
        assert!(m.check().is_ok());
    }

    #[test]
    fn breaks_names_the_versions_a_mod_cant_be_built_with() {
        let breaks = BTreeMap::from([("old_lib".to_owned(), "*".to_owned())]);
        let none = BTreeMap::new();
        assert!(broken_pairs([("a", "1.0", &breaks)]).is_empty());
        assert_eq!(
            broken_pairs([("a", "1.0", &breaks), ("old_lib", "0.1", &none)]).len(),
            1
        );
        let newer = BTreeMap::from([("lib".to_owned(), ">=2.0".to_owned())]);
        assert!(broken_pairs([("a", "1.0", &newer), ("lib", "1.9", &none)]).is_empty());
    }
}
