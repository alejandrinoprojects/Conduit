use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ModEnvironment {
    Both,
    ClientOnly,
    ServerOnly,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstalledMod {
    pub file_name: String,
    pub mod_id: Option<String>,
    pub mod_name: Option<String>,
    pub version: Option<String>,
    pub environment: ModEnvironment,
    pub size_bytes: u64,
}

// -----------------------------------------------------------------------------
// Client-Only Filter & Inspection Logic
// -----------------------------------------------------------------------------

const KNOWN_CLIENT_MODS: &[&str] = &[
    "sodium",
    "embeddium",
    "iris",
    "oculus",
    "bobby",
    "dynamic_fps",
    "dynamicfps",
    "entityculling",
    "entity_culling",
    "journeymap",
    "xaerominimap",
    "xaeroworldmap",
    "rubidium",
    "continuity",
    "lambdynamiclights",
    "sodiumextra",
    "sodium-extra",
    "reesessodiumoptions",
    "reeses-sodium-options",
    "appleskin",
    "modmenu",
    "controlify",
    "immediatelyfast",
    "ferritecore",
];

pub fn is_client_only_mod(jar_path: &Path) -> bool {
    let file = match File::open(jar_path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut archive = match zip::ZipArchive::new(BufReader::new(file)) {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Attempt to read META-INF/neoforge.mods.toml or META-INF/mods.toml
    let mut toml_content = String::new();
    let toml_names = ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"];

    for name in toml_names {
        if let Ok(mut entry) = archive.by_name(name) {
            let _ = entry.read_to_string(&mut toml_content);
            if !toml_content.is_empty() {
                break;
            }
        }
    }

    if toml_content.is_empty() {
        // Fallback: check filename against known client mods
        if let Some(file_stem) = jar_path.file_stem().and_then(|s| s.to_str()) {
            let lower = file_stem.to_lowercase();
            return KNOWN_CLIENT_MODS.iter().any(|k| lower.contains(k));
        }
        return false;
    }

    let parsed: toml::Value = match toml::from_str(&toml_content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // 1. Check displayTest == "IGNORE_SERVER_VERSION"
    if let Some(mods_arr) = parsed.get("mods").and_then(|m| m.as_array()) {
        for m in mods_arr {
            if let Some(disp) = m.get("displayTest").and_then(|d| d.as_str()) {
                if disp.eq_ignore_ascii_case("IGNORE_SERVER_VERSION")
                    || disp.eq_ignore_ascii_case("IGNOREALLVERSION")
                {
                    return true;
                }
            }

            // 2. Check modId against known client-only visual mods
            if let Some(mod_id) = m.get("modId").and_then(|id| id.as_str()) {
                let id_lower = mod_id.to_lowercase();
                if KNOWN_CLIENT_MODS.iter().any(|&k| id_lower == k || id_lower.contains(k)) {
                    return true;
                }
            }
        }
    }

    // 3. Check [[dependencies.<modid>]] for side = "CLIENT"
    if let Some(deps_table) = parsed.get("dependencies").and_then(|d| d.as_table()) {
        for (_modid, deps_val) in deps_table {
            if let Some(deps_list) = deps_val.as_array() {
                for dep in deps_list {
                    if let Some(side) = dep.get("side").and_then(|s| s.as_str()) {
                        if side.eq_ignore_ascii_case("CLIENT") {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

pub fn sync_server_mods(instance_dir: &Path) -> Result<usize, String> {
    let client_mods_dir = instance_dir.join(".minecraft").join("mods");
    let server_mods_dir = instance_dir.join("server").join("mods");

    fs::create_dir_all(&server_mods_dir)
        .map_err(|e| format!("Failed to create server mods directory: {e}"))?;

    // Clear existing JARs in server/mods
    if let Ok(entries) = fs::read_dir(&server_mods_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jar") {
                let _ = fs::remove_file(p);
            }
        }
    }

    if !client_mods_dir.exists() {
        return Ok(0);
    }

    let mut synced_count = 0;
    let entries = fs::read_dir(&client_mods_dir)
        .map_err(|e| format!("Failed to read client mods directory: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
            if !is_client_only_mod(&path) {
                if let Some(file_name) = path.file_name() {
                    let dest = server_mods_dir.join(file_name);
                    // Constraint 4: Always use hard_link with copy fallback
                    if fs::hard_link(&path, &dest).is_err() {
                        fs::copy(&path, &dest)
                            .map_err(|e| format!("Failed copying mod to server: {e}"))?;
                    }
                    synced_count += 1;
                }
            }
        }
    }

    Ok(synced_count)
}

// -----------------------------------------------------------------------------
// Modrinth API Integration
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn search_modrinth(query: String, offset: u32) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let url = "https://api.modrinth.com/v2/search";

    let facets = r#"[["categories:neoforge"],["versions:1.21.1"],["project_type:mod"]]"#;

    let resp = client
        .get(url)
        .header("User-Agent", "Conduit-Launcher/1.0.0 (contact@alejandrino.dev)")
        .query(&[
            ("query", query.as_str()),
            ("offset", &offset.to_string()),
            ("limit", "20"),
            ("facets", facets),
        ])
        .send()
        .await
        .map_err(|e| format!("Modrinth search query failed: {e}"))?;

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Modrinth JSON: {e}"))?;

    Ok(json)
}

#[tauri::command]
pub async fn install_modrinth_mod(
    instance_dir: String,
    project_id: String,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let client_mods_dir = PathBuf::from(&instance_dir).join(".minecraft").join("mods");
    fs::create_dir_all(&client_mods_dir)
        .map_err(|e| format!("Failed to create client mods directory: {e}"))?;

    let mut visited = HashSet::new();
    install_mod_recursive(&client, &client_mods_dir, &project_id, &mut visited).await
}

async fn install_mod_recursive(
    client: &reqwest::Client,
    mods_dir: &Path,
    project_id: &str,
    visited: &mut HashSet<String>,
) -> Result<String, String> {
    if visited.contains(project_id) {
        return Ok("Already installed".into());
    }
    visited.insert(project_id.to_string());

    let version_url = format!(
        "https://api.modrinth.com/v2/project/{}/version?loaders=[\"neoforge\"]&game_versions=[\"1.21.1\"]",
        project_id
    );

    let resp = client
        .get(&version_url)
        .header("User-Agent", "Conduit-Launcher/1.0.0 (contact@alejandrino.dev)")
        .send()
        .await
        .map_err(|e| format!("Failed fetching mod versions for {project_id}: {e}"))?;

    let versions: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed parsing versions JSON for {project_id}: {e}"))?;

    let version_arr = versions
        .as_array()
        .ok_or_else(|| format!("No compatible NeoForge 1.21.1 version found for {project_id}"))?;

    if version_arr.is_empty() {
        return Err(format!("No NeoForge 1.21.1 versions available for {project_id}"));
    }

    let primary_version = &version_arr[0];
    let files = primary_version["files"]
        .as_array()
        .ok_or_else(|| "Missing files array in version payload".to_string())?;

    let primary_file = files
        .iter()
        .find(|f| f["primary"].as_bool() == Some(true))
        .unwrap_or(&files[0]);

    let download_url = primary_file["url"]
        .as_str()
        .ok_or_else(|| "Missing file download url".to_string())?;

    let filename = primary_file["filename"]
        .as_str()
        .ok_or_else(|| "Missing filename in file descriptor".to_string())?;

    let target_file = mods_dir.join(filename);
    if !target_file.exists() {
        let file_bytes = client
            .get(download_url)
            .header("User-Agent", "Conduit-Launcher/1.0.0 (contact@alejandrino.dev)")
            .send()
            .await
            .map_err(|e| format!("Failed downloading {filename}: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Failed reading bytes of {filename}: {e}"))?;

        fs::write(&target_file, file_bytes)
            .map_err(|e| format!("Failed writing {filename}: {e}"))?;
    }

    // Recursively resolve required dependencies
    if let Some(deps) = primary_version["dependencies"].as_array() {
        for dep in deps {
            if dep["dependency_type"].as_str() == Some("required") {
                if let Some(dep_proj_id) = dep["project_id"].as_str() {
                    let _ = Box::pin(install_mod_recursive(
                        client,
                        mods_dir,
                        dep_proj_id,
                        visited,
                    ))
                    .await;
                }
            }
        }
    }

    Ok(filename.to_string())
}

// -----------------------------------------------------------------------------
// Mod Management Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn list_installed_mods(instance_dir: String) -> Result<Vec<InstalledMod>, String> {
    let mods_dir = PathBuf::from(&instance_dir).join(".minecraft").join("mods");
    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let entries = fs::read_dir(&mods_dir)
        .map_err(|e| format!("Failed reading mods directory: {e}"))?;

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("jar") {
            let file_name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();

            let metadata = p.metadata().ok();
            let size_bytes = metadata.map(|m| m.len()).unwrap_or(0);

            let is_client_only = is_client_only_mod(&p);
            let environment = if is_client_only {
                ModEnvironment::ClientOnly
            } else {
                ModEnvironment::Both
            };

            // Inspect metadata from jar
            let (mod_id, mod_name, version) = extract_mod_metadata(&p);

            result.push(InstalledMod {
                file_name,
                mod_id,
                mod_name,
                version,
                environment,
                size_bytes,
            });
        }
    }

    result.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(result)
}

fn extract_mod_metadata(jar_path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let file = match File::open(jar_path) {
        Ok(f) => f,
        Err(_) => return (None, None, None),
    };

    let mut archive = match zip::ZipArchive::new(BufReader::new(file)) {
        Ok(a) => a,
        Err(_) => return (None, None, None),
    };

    let mut content = String::new();
    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut entry) = archive.by_name(name) {
            let _ = entry.read_to_string(&mut content);
            if !content.is_empty() {
                break;
            }
        }
    }

    if let Ok(val) = toml::from_str::<toml::Value>(&content) {
        if let Some(mods_arr) = val.get("mods").and_then(|m| m.as_array()) {
            if let Some(first) = mods_arr.first() {
                let id = first.get("modId").and_then(|i| i.as_str()).map(String::from);
                let name = first.get("displayName").and_then(|n| n.as_str()).map(String::from);
                let ver = first.get("version").and_then(|v| v.as_str()).map(String::from);
                return (id, name, ver);
            }
        }
    }

    (None, None, None)
}

#[tauri::command]
pub async fn delete_mod(instance_dir: String, file_name: String) -> Result<(), String> {
    let client_path = PathBuf::from(&instance_dir)
        .join(".minecraft")
        .join("mods")
        .join(&file_name);
    let server_path = PathBuf::from(&instance_dir)
        .join("server")
        .join("mods")
        .join(&file_name);

    if client_path.exists() {
        fs::remove_file(client_path).map_err(|e| format!("Failed to delete client mod: {e}"))?;
    }
    if server_path.exists() {
        let _ = fs::remove_file(server_path);
    }

    Ok(())
}
