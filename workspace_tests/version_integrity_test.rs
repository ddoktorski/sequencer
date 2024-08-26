use crate::toml_utils::{LocalCrate, ROOT_TOML};

#[test]
fn test_path_dependencies_are_members() {
    let non_member_path_crates: Vec<_> = ROOT_TOML
        .workspace_path_dependencies()
        .filter(|LocalCrate { path, .. }| !ROOT_TOML.members().contains(path))
        .collect();
    assert!(
        non_member_path_crates.is_empty(),
        "The following crates are path dependencies but not members of the workspace: \
         {non_member_path_crates:?}."
    );
}

#[test]
fn test_version_alignment() {
    let workspace_version = ROOT_TOML.workspace_version();
    let crates_with_incorrect_version: Vec<_> = ROOT_TOML
        .workspace_path_dependencies()
        .filter(|LocalCrate { version, .. }| version != workspace_version)
        .collect();
    assert!(
        crates_with_incorrect_version.is_empty(),
        "The following crates have versions different from the workspace version \
         '{workspace_version}': {crates_with_incorrect_version:?}."
    );
}

#[test]
fn validate_no_path_dependencies() {
    let mut all_crate_paths: Vec<LocalCrate> = Vec::new();
    for crate_cargo_toml in ROOT_TOML.member_cargo_tomls().iter() {
        if crate_cargo_toml.has_dependencies() {
            let crate_paths: Vec<LocalCrate> = crate_cargo_toml.crate_path_dependencies().collect();
            all_crate_paths.extend(crate_paths);
        } else {
            println!("No dependencies exist");
        }
        assert!(
            all_crate_paths.is_empty(),
            "The following crates have path dependency {all_crate_paths:?}."
        );
    }
}
