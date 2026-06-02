#![no_main]

use std::collections::HashMap;

use arbitrary::{Arbitrary, Unstructured};
use atom_id::{Anchor, AtomId, Label};
use eos::lock::{
    AtomDep, ComposeConfig, Dependency, LockFile, NixDep, NixGitDep, NixSrcDep, NixTarDep,
    SetDetails,
};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
enum Mutation {
    InjectUnknownField { key: String, value: String },
    RemoveSetEntry,
    AddCyclicRequires,
    CorruptAtomId,
    FlipVersionToNonZero(u32),
}

fn generate_valid_lockfile(driver: &mut Unstructured<'_>) -> Result<LockFile, arbitrary::Error> {
    let num_sets = driver.int_in_range(1..=3)?;
    let mut sets = HashMap::new();
    let mut anchors = Vec::new();

    for _ in 0..num_sets {
        let anchor_bytes = driver.arbitrary::<[u8; 20]>()?;
        let anchor_hex = hex::encode(anchor_bytes);

        let tag = format!("set-{}", driver.arbitrary::<u8>()?);
        let mirrors = if driver.arbitrary::<bool>()? {
            vec!["::".to_string()]
        } else {
            vec![format!("https://git.example.com/{}.git", tag)]
        };

        sets.insert(anchor_hex.clone(), SetDetails { tag, mirrors });
        anchors.push((anchor_bytes.to_vec(), anchor_hex));
    }

    // Generate some atoms
    let num_atoms = driver.int_in_range(1..=5)?;
    let mut atom_deps: Vec<AtomDep> = Vec::new();
    let mut generated_ids = std::collections::HashSet::new();

    for i in 0..num_atoms {
        let (anchor_bytes, anchor_hex) = driver.choose(&anchors)?.clone();

        // Loop to get a unique label for this anchor
        let mut attempts = 0;
        let label = loop {
            let mut label_chars = vec![driver.int_in_range(b'a'..=b'z')? as char];
            for _ in 0..driver.int_in_range(0..=8)? {
                let c = match driver.int_in_range(0..=1)? {
                    0 => driver.int_in_range(b'a'..=b'z')? as char,
                    1 => driver.int_in_range(b'0'..=b'9')? as char,
                    _ => '-',
                };
                label_chars.push(c);
            }
            let cand_label = label_chars.into_iter().collect::<String>();
            if !generated_ids.contains(&(anchor_bytes.clone(), cand_label.clone())) {
                break cand_label;
            }
            attempts += 1;
            if attempts > 10 {
                return Err(arbitrary::Error::IncorrectFormat);
            }
        };
        generated_ids.insert((anchor_bytes.clone(), label.clone()));

        let version = format!(
            "{}.{}.{}",
            driver.int_in_range(0..=9)?,
            driver.int_in_range(0..=9)?,
            driver.int_in_range(0..=9)?
        );

        // rev is required if remote, optional if local
        let set_details = sets.get(&anchor_hex).unwrap();
        let is_local = set_details.mirrors.len() == 1 && set_details.mirrors[0] == "::";
        let rev = if is_local {
            if driver.arbitrary::<bool>()? {
                Some(hex::encode(driver.arbitrary::<[u8; 20]>()?))
            } else {
                None
            }
        } else {
            Some(hex::encode(driver.arbitrary::<[u8; 20]>()?))
        };

        let label_parsed = Label::try_from(label.as_str()).unwrap();
        let anchor_struct = Anchor::new(anchor_bytes);
        let id = AtomId::new(anchor_struct, label_parsed);

        // Topological sort trick for acyclic requires:
        // atom_i can only require atoms from 0..i
        let mut requires = Vec::new();
        if i > 0 {
            for j in 0..i {
                if driver.arbitrary::<bool>()? {
                    requires.push(atom_deps[j].id.clone());
                }
            }
        }

        atom_deps.push(AtomDep {
            label,
            version,
            set: anchor_hex,
            rev,
            id,
            requires,
            direct: driver.arbitrary::<bool>()?,
        });
    }

    // Add other dependency types (Nix, NixGit, etc.)
    let mut deps = Vec::new();
    for atom_dep in atom_deps.clone() {
        deps.push(Dependency::Atom(atom_dep));
    }

    let num_other_deps = driver.int_in_range(0..=3)?;
    for _ in 0..num_other_deps {
        let name = format!("dep-{}", driver.arbitrary::<u8>()?);
        let url = format!("https://example.com/{}.nix", name);
        let hash = format!("sha256:{}", hex::encode(driver.arbitrary::<[u8; 32]>()?));
        let owner = if driver.arbitrary::<bool>()? && !atom_deps.is_empty() {
            Some(driver.choose(&atom_deps)?.id.clone())
        } else {
            None
        };

        let dep = match driver.int_in_range(0..=2)? {
            0 => Dependency::Nix(NixDep {
                name,
                url,
                hash,
                owner,
            }),
            1 => Dependency::NixGit(NixGitDep {
                name,
                url,
                rev: hex::encode(driver.arbitrary::<[u8; 20]>()?),
                version: Some("1.0.0".to_string()),
                owner,
            }),
            2 => Dependency::NixTar(NixTarDep {
                name,
                url,
                hash,
                owner,
            }),
            _ => Dependency::NixSrc(NixSrcDep {
                name,
                url,
                hash,
                owner,
            }),
        };
        deps.push(dep);
    }

    // Composer
    let r#use = if driver.arbitrary::<bool>()? && !atom_deps.is_empty() {
        Some(driver.choose(&atom_deps)?.id.to_string())
    } else {
        match driver.int_in_range(0..=1)? {
            0 => Some("nix".to_string()),
            1 => Some("static".to_string()),
            _ => None,
        }
    };

    let compose = ComposeConfig {
        r#use,
        at: Some("1.0.0".to_string()),
        entry: Some("default.nix".to_string()),
        args: HashMap::new(),
    };

    Ok(LockFile {
        version: 0,
        sets,
        compose,
        deps,
    })
}

fuzz_target!(|data: &[u8]| {
    let mut driver = Unstructured::new(data);
    if let Ok(mut lockfile) = generate_valid_lockfile(&mut driver) {
        // Apply structured mutations
        if let Ok(mutation) = driver.arbitrary::<Mutation>() {
            match mutation {
                Mutation::InjectUnknownField { key, value } => {
                    // Try to inject an unknown field by serializing, appending, and deserializing
                    if let Ok(mut toml_val) = toml::to_string(&lockfile) {
                        toml_val.push_str(&format!("\n{} = \"{}\"\n", key, value));
                        let _ = LockFile::parse(&toml_val);
                    }
                },
                Mutation::RemoveSetEntry => {
                    if !lockfile.sets.is_empty() {
                        let keys: Vec<String> = lockfile.sets.keys().cloned().collect();
                        if let Ok(k) = driver.choose(&keys) {
                            lockfile.sets.remove(k);
                        }
                    }
                },
                Mutation::AddCyclicRequires => {
                    let mut atom_indices = Vec::new();
                    for (idx, dep) in lockfile.deps.iter().enumerate() {
                        if let Dependency::Atom(_) = dep {
                            atom_indices.push(idx);
                        }
                    }
                    if atom_indices.len() >= 2 {
                        let id_0 = match &lockfile.deps[atom_indices[0]] {
                            Dependency::Atom(a) => a.id.clone(),
                            _ => unreachable!(),
                        };
                        let id_1 = match &lockfile.deps[atom_indices[1]] {
                            Dependency::Atom(a) => a.id.clone(),
                            _ => unreachable!(),
                        };
                        if let Dependency::Atom(a) = &mut lockfile.deps[atom_indices[0]] {
                            a.requires.push(id_1);
                        }
                        if let Dependency::Atom(a) = &mut lockfile.deps[atom_indices[1]] {
                            a.requires.push(id_0);
                        }
                    }
                },
                Mutation::CorruptAtomId => {
                    for dep in &mut lockfile.deps {
                        if let Dependency::Atom(a) = dep {
                            // Break the AtomId format or label
                            a.label = "".to_string();
                        }
                    }
                },
                Mutation::FlipVersionToNonZero(v) => {
                    lockfile.version = v.max(1) as u64;
                },
            }
        }

        let _ = lockfile.validate();
    }
});
