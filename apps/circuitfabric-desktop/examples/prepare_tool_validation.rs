//! Prepare two actual project roots for the isolated native UI acceptance run.
use circuitfabric_contracts::Project;
use circuitfabric_project::{ProjectConfiguration, ProjectRegistry, ProjectStorage};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::args().nth(1).ok_or("fixture root required")?);
    let mut registry = ProjectRegistry::default();
    for (id, skill, server) in
        [("native-a", "skill-00", "first"), ("native-b", "skill-01", "second")]
    {
        let path = root.join(id);
        std::fs::create_dir_all(&path)?;
        let storage = if path.join(".circuitfabric/project.json").exists() {
            ProjectStorage::open(&path)?
        } else {
            ProjectStorage::create(
                &path,
                Project { id: id.into(), name: id.into(), description: None },
            )?
        };
        storage.save_configuration(&ProjectConfiguration {
            enabled_skill_ids: vec![skill.into()],
            enabled_mcp_server_ids: vec![server.into()],
            ..Default::default()
        })?;
        registry.register(&storage)?;
    }
    registry.save(root.join("profile/CircuitFabric/projects.json"))?;
    println!("Prepared native-a and native-b with disjoint project authorizations");
    Ok(())
}
