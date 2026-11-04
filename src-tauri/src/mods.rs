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
    pub is_enabled: bool,
    pub icon_base64: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModVersionOption {
    pub id: String,
    pub version_number: String,
    pub name: String,
    pub date_published: String,
    pub download_url: String,
    pub filename: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub version_type: String,
    pub downloads: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModrinthProjectDetail {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub followers: u64,
    pub categories: Vec<String>,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub gallery: Vec<ModrinthGalleryItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModrinthGalleryItem {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModrinthProject {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub client_side: String,
    pub server_side: String,
    pub author: Option<String>,
    pub categories: Option<Vec<String>>,
    pub follows: Option<u64>,
    pub date_modified: Option<String>,
    pub loaders: Option<Vec<String>>,
    pub slug: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthProject>,
    pub total_hits: u64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorldSaveInfo {
    pub folder_name: String,
    pub display_name: String,
    pub last_played: i64,
    pub game_mode: String,
    pub icon_base64: Option<String>,
    pub version: Option<String>,
    pub loader: Option<String>,
    pub mods: Vec<String>,
    pub runtime_name: Option<String>,
    pub runtime_id: Option<String>,
    pub full_path: Option<String>,
    pub server_address: Option<String>,
    pub is_realm: bool,
    pub hosting_mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadedRuntimeItem {
    pub id: String,
    pub loader: String,
    pub version: String,
    pub is_server: bool,
    pub is_client: bool,
    pub size_mb: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeduplicationMetrics {
    pub libraries_shared: bool,
    pub configs_shared: bool,
    pub world_shared: bool,
    pub mods_hardlinked: usize,
    pub client_libraries_size_mb: f64,
    pub server_libraries_size_mb: f64,
    pub saved_disk_mb: f64,
    pub server_world_path: String,
    pub client_world_path: String,
}

// Realm Pruner Data Structures
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegionChunkInfo {
    pub region_x: i32,
    pub region_z: i32,
    pub chunk_count: usize,
    pub populated_chunks: Vec<(i32, i32)>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DimensionPruneSummary {
    pub dimension: String,
    pub total_regions: usize,
    pub total_chunks: usize,
    pub regions: Vec<RegionChunkInfo>,
    pub is_bedrock: bool,
}

#[tauri::command]
pub async fn list_installed_mods(instance_dir: String) -> Result<Vec<InstalledMod>, String> {
    let base = PathBuf::from(&instance_dir);
    let candidate_dirs = [
        base.join("mods"),
        base.join(".minecraft").join("mods"),
        base.join("server").join("mods"),
    ];

    let mut result = Vec::new();
    let mut seen_files = HashSet::new();

    for dir in &candidate_dirs {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();

                let is_jar = fname.ends_with(".jar");
                let is_disabled = fname.ends_with(".jar.disabled");

                if (is_jar || is_disabled) && seen_files.insert(fname.clone()) {
                    let metadata = fs::metadata(&path).ok();
                    let size_bytes = metadata.map(|m| m.len()).unwrap_or(0);
                    let is_enabled = is_jar;

                    let (mod_id, mod_name, version, icon_base64) = extract_mod_metadata(&path);

                    let env = if is_client_only_mod(&fname) {
                        ModEnvironment::ClientOnly
                    } else if fname.contains("server") {
                        ModEnvironment::ServerOnly
                    } else {
                        ModEnvironment::Both
                    };

                    result.push(InstalledMod {
                        file_name: fname,
                        mod_id,
                        mod_name,
                        version,
                        environment: env,
                        size_bytes,
                        is_enabled,
                        icon_base64,
                    });
                }
            }
        }
    }

    result.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(result)
}

#[tauri::command]
pub async fn toggle_mod(instance_dir: String, file_name: String, enable: bool) -> Result<(), String> {
    let mods_dir = PathBuf::from(&instance_dir).join("mods");
    let current_path = mods_dir.join(&file_name);

    if enable && file_name.ends_with(".jar.disabled") {
        let new_name = file_name.trim_end_matches(".disabled");
        let new_path = mods_dir.join(new_name);
        fs::rename(current_path, new_path).map_err(|e| e.to_string())?;
    } else if !enable && file_name.ends_with(".jar") {
        let new_name = format!("{}.disabled", file_name);
        let new_path = mods_dir.join(new_name);
        fs::rename(current_path, new_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn delete_mod(instance_dir: String, file_name: String) -> Result<(), String> {
    let target = PathBuf::from(&instance_dir).join("mods").join(file_name);
    if target.exists() {
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_player_data(
    instance_dir: String,
    uuid: String,
    scope: String,
) -> Result<String, String> {
    let mut deleted_files = Vec::new();
    let paths_to_check = vec![
        PathBuf::from(&instance_dir).join("server").join("world"),
        PathBuf::from(&instance_dir).join(".minecraft").join("saves"),
    ];

    for base in paths_to_check {
        if !base.exists() {
            continue;
        }

        let mut world_dirs = Vec::new();
        if base.ends_with("saves") {
            if let Ok(entries) = fs::read_dir(&base) {
                for e in entries.flatten() {
                    if e.path().is_dir() {
                        world_dirs.push(e.path());
                    }
                }
            }
        } else {
            world_dirs.push(base);
        }

        for w in world_dirs {
            if scope == "all" || scope == "inventory" {
                let pdata = w.join("playerdata").join(format!("{}.dat", uuid));
                let pdata_old = w.join("playerdata").join(format!("{}.dat_old", uuid));
                let pdata_cosarmor = w.join("playerdata").join(format!("{}.cosarmor", uuid));
                for f in [pdata, pdata_old, pdata_cosarmor] {
                    if f.exists() {
                        let _ = fs::remove_file(&f);
                        deleted_files.push(f.to_string_lossy().to_string());
                    }
                }
            }

            if scope == "all" || scope == "advancements" {
                let adv = w.join("advancements").join(format!("{}.json", uuid));
                if adv.exists() {
                    let _ = fs::remove_file(&adv);
                    deleted_files.push(adv.to_string_lossy().to_string());
                }
            }

            if scope == "all" || scope == "stats" {
                let stats = w.join("stats").join(format!("{}.json", uuid));
                if stats.exists() {
                    let _ = fs::remove_file(&stats);
                    deleted_files.push(stats.to_string_lossy().to_string());
                }
            }
        }
    }

    Ok(format!(
        "Successfully deleted {} playerdata files for UUID {} with scope '{}'.",
        deleted_files.len(),
        uuid,
        scope
    ))
}

#[tauri::command]
pub async fn search_modrinth(
    query: String,
    project_type: Option<String>,
    loader: Option<String>,
    game_version: Option<String>,
    tag: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    index: Option<String>,
) -> Result<ModrinthSearchResponse, String> {
    let client = reqwest::Client::new();
    let lim = limit.unwrap_or(20);
    let off = offset.unwrap_or(0);
    let sort_idx = index.unwrap_or_else(|| "relevance".to_string());
    let mut url = format!(
        "https://api.modrinth.com/v2/search?query={}&limit={}&offset={}&index={}",
        urlencoding::encode(&query),
        lim,
        off,
        urlencoding::encode(&sort_idx)
    );

    let mut facets_vec: Vec<Vec<String>> = Vec::new();
    let pt = project_type.unwrap_or_else(|| "mod".to_string());
    facets_vec.push(vec![format!("project_type:{}", pt)]);

    if pt == "mod" || pt == "modpack" {
        if let Some(l) = loader {
            let l_lower = l.to_lowercase();
            if !l_lower.is_empty() && l_lower != "all" && l_lower != "vanilla" && l_lower != "bedrock" {
                facets_vec.push(vec![format!("categories:{}", l_lower)]);
            }
        }
    }

    if let Some(gv) = game_version {
        let gv_lower = gv.to_lowercase();
        if !gv_lower.is_empty() && gv_lower != "all" {
            facets_vec.push(vec![format!("versions:{}", gv)]);
        }
    }

    if let Some(t) = tag {
        let t_clean = t.trim().to_lowercase();
        if !t_clean.is_empty() && t_clean != "all" {
            facets_vec.push(vec![format!("categories:{}", t_clean)]);
        }
    }

    if let Ok(facets_json) = serde_json::to_string(&facets_vec) {
        url.push_str(&format!("&facets={}", urlencoding::encode(&facets_json)));
    }

    let resp = client
        .get(&url)
        .header("User-Agent", "Conduit-Launcher/0.1.0 (contact@conduit.local)")
        .send()
        .await
        .map_err(|e| format!("Network request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth API error: HTTP {}", resp.status()));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

    let total_hits = body.get("total_hits").and_then(|v| v.as_u64()).unwrap_or(0);
    let hits = body.get("hits").and_then(|h| h.as_array()).cloned().unwrap_or_default();
    let mut results = Vec::new();

    for h in hits {
        let project_id = h.get("project_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let slug = h.get("slug").and_then(|v| v.as_str()).map(String::from);
        let title = h.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let description = h.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let icon_url = h.get("icon_url").and_then(|v| v.as_str()).map(String::from);
        let downloads = h.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
        let follows = h.get("follows").and_then(|v| v.as_u64());
        let date_modified = h.get("date_modified").and_then(|v| v.as_str()).map(String::from);
        let client_side = h.get("client_side").and_then(|v| v.as_str()).unwrap_or("optional").to_string();
        let server_side = h.get("server_side").and_then(|v| v.as_str()).unwrap_or("optional").to_string();
        let author = h.get("author").and_then(|v| v.as_str()).map(String::from);
        let categories = h.get("categories").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>()
        });
        let display_categories = h.get("display_categories").and_then(|v| v.as_array()).map(|arr| {
            arr.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>()
        });

        // Identify loaders
        let known_loaders = ["fabric", "forge", "neoforge", "quilt", "rift", "liteloader"];
        let mut loaders = Vec::new();
        if let Some(ref cats) = categories {
            for c in cats {
                let cl = c.to_lowercase();
                if known_loaders.contains(&cl.as_str()) && !loaders.contains(&c.clone()) {
                    loaders.push(c.clone());
                }
            }
        }

        results.push(ModrinthProject {
            project_id,
            title,
            description,
            icon_url,
            downloads,
            client_side,
            server_side,
            author,
            categories: display_categories.or(categories),
            follows,
            date_modified,
            loaders: Some(loaders),
            slug,
        });
    }

    Ok(ModrinthSearchResponse {
        hits: results,
        total_hits,
        offset: off,
        limit: lim,
    })
}

#[tauri::command]
pub async fn get_modrinth_project_detail(project_id_or_slug: String) -> Result<ModrinthProjectDetail, String> {
    let client = reqwest::Client::new();
    let url = format!("https://api.modrinth.com/v2/project/{}", project_id_or_slug);

    let resp = client
        .get(&url)
        .header("User-Agent", "Conduit-Launcher/0.1.0 (contact@conduit.local)")
        .send()
        .await
        .map_err(|e| format!("Failed fetching project detail: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth API error: HTTP {}", resp.status()));
    }

    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let slug = val.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let description = val.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let body = val.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let icon_url = val.get("icon_url").and_then(|v| v.as_str()).map(String::from);
    let downloads = val.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
    let followers = val.get("followers").and_then(|v| v.as_u64()).unwrap_or(0);

    let categories = val.get("categories").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()
    }).unwrap_or_default();

    let loaders = val.get("loaders").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()
    }).unwrap_or_default();

    let game_versions = val.get("game_versions").and_then(|v| v.as_array()).map(|arr| {
        arr.iter().filter_map(|x| x.as_str().map(String::from)).collect()
    }).unwrap_or_default();

    let mut gallery = Vec::new();
    if let Some(gal_arr) = val.get("gallery").and_then(|v| v.as_array()) {
        for g in gal_arr {
            if let Some(url) = g.get("url").and_then(|v| v.as_str()) {
                gallery.push(ModrinthGalleryItem {
                    url: url.to_string(),
                    title: g.get("title").and_then(|v| v.as_str()).map(String::from),
                    description: g.get("description").and_then(|v| v.as_str()).map(String::from),
                    created: g.get("created").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                });
            }
        }
    }

    Ok(ModrinthProjectDetail {
        id,
        slug,
        title,
        description,
        body,
        icon_url,
        downloads,
        followers,
        categories,
        loaders,
        game_versions,
        gallery,
    })
}

#[tauri::command]
pub async fn get_mod_versions(
    project_id_or_slug: String,
    loaders: Option<Vec<String>>,
    game_versions: Option<Vec<String>>,
) -> Result<Vec<ModVersionOption>, String> {
    let client = reqwest::Client::new();
    let mut url = format!("https://api.modrinth.com/v2/project/{}/version", project_id_or_slug);

    let mut query_params = Vec::new();
    if let Some(l) = loaders {
        if !l.is_empty() {
            let json = serde_json::to_string(&l).unwrap_or_default();
            query_params.push(format!("loaders={}", urlencoding::encode(&json)));
        }
    }
    if let Some(gv) = game_versions {
        if !gv.is_empty() {
            let json = serde_json::to_string(&gv).unwrap_or_default();
            query_params.push(format!("game_versions={}", urlencoding::encode(&json)));
        }
    }

    if !query_params.is_empty() {
        url.push('?');
        url.push_str(&query_params.join("&"));
    }

    let resp = client
        .get(&url)
        .header("User-Agent", "Conduit-Launcher/0.1.0 (contact@conduit.local)")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch versions: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth API error: HTTP {}", resp.status()));
    }

    let list: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse version list: {e}"))?;

    let arr = list.as_array().ok_or("Expected array of versions")?;
    let mut results = Vec::new();

    for v in arr {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let version_number = v.get("version_number").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let date_published = v.get("date_published").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let version_type = v.get("version_type").and_then(|x| x.as_str()).unwrap_or("release").to_string();
        let downloads = v.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0);

        let g_versions = v.get("game_versions").and_then(|x| x.as_array()).map(|a| {
            a.iter().filter_map(|val| val.as_str().map(String::from)).collect::<Vec<_>>()
        }).unwrap_or_default();

        let l_loaders = v.get("loaders").and_then(|x| x.as_array()).map(|a| {
            a.iter().filter_map(|val| val.as_str().map(String::from)).collect::<Vec<_>>()
        }).unwrap_or_default();

        if let Some(files) = v.get("files").and_then(|f| f.as_array()) {
            if let Some(primary) = files.iter().find(|f| f.get("primary").and_then(|p| p.as_bool()).unwrap_or(false)).or_else(|| files.first()) {
                let download_url = primary.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                let filename = primary.get("filename").and_then(|u| u.as_str()).unwrap_or("").to_string();

                if !download_url.is_empty() {
                    results.push(ModVersionOption {
                        id,
                        version_number,
                        name,
                        date_published,
                        download_url,
                        filename,
                        game_versions: g_versions,
                        loaders: l_loaders,
                        version_type,
                        downloads,
                    });
                }
            }
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn install_modrinth_mod(
    instance_dir: String,
    download_url: String,
    filename: String,
) -> Result<(), String> {
    let mods_dir = PathBuf::from(&instance_dir).join("mods");
    if !mods_dir.exists() {
        fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    }

    let target_file = mods_dir.join(&filename);
    let client = reqwest::Client::new();
    let resp = client
        .get(&download_url)
        .header("User-Agent", "Conduit-Launcher/0.1.0 (contact@conduit.local)")
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| format!("Failed to read stream: {e}"))?;
    fs::write(&target_file, bytes).map_err(|e| format!("Failed saving file: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn switch_mod_version(
    instance_dir: String,
    old_file_name: String,
    download_url: String,
    new_filename: String,
) -> Result<(), String> {
    let mods_dir = PathBuf::from(&instance_dir).join("mods");
    let old_path = mods_dir.join(&old_file_name);
    if old_path.exists() {
        let _ = fs::remove_file(old_path);
    }

    install_modrinth_mod(instance_dir, download_url, new_filename).await
}

#[tauri::command]
pub async fn install_modrinth_project(
    instance_dir: String,
    project_id_or_slug: String,
    loader: String,
    game_version: String,
) -> Result<String, String> {
    let versions = get_mod_versions(
        project_id_or_slug.clone(),
        Some(vec![loader.to_lowercase()]),
        Some(vec![game_version]),
    )
    .await?;

    if let Some(target) = versions.first() {
        install_modrinth_mod(
            instance_dir,
            target.download_url.clone(),
            target.filename.clone(),
        )
        .await?;
        Ok(target.filename.clone())
    } else {
        Err(format!("No compatible version found for project '{project_id_or_slug}'."))
    }
}

fn parse_single_world_save(
    path: &Path,
    runtime_name: String,
    runtime_id: String,
    loader: String,
    version: String,
    server_address: Option<String>,
    instance_dir: &str,
) -> Option<WorldSaveInfo> {
    if !path.is_dir() {
        return None;
    }

    let folder_name = path.file_name()?.to_string_lossy().to_string();
    let mut display_name = folder_name.clone();
    let mut game_mode = if server_address.is_some() {
        "Dedicated Server".to_string()
    } else {
        "Survival Mode".to_string()
    };
    let mut last_played = 0i64;
    let mut icon_base64 = None;
    let mut world_version = version;
    let world_loader = loader;

    let icon_path = path.join("icon.png");
    if icon_path.exists() {
        if let Ok(bytes) = fs::read(&icon_path) {
            use base64::Engine;
            icon_base64 = Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            ));
        }
    }

    let level_dat = path.join("level.dat");
    if level_dat.exists() {
        if let Ok(meta) = fs::metadata(&level_dat) {
            if let Ok(modified) = meta.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    last_played = duration.as_secs() as i64;
                }
            }
        }

        if let Ok(file) = File::open(&level_dat) {
            use flate2::read::GzDecoder;
            let decoder = GzDecoder::new(file);
            let mut reader = BufReader::new(decoder);
            let mut buffer = Vec::new();
            if reader.read_to_end(&mut buffer).is_ok() {
                // Precise binary NBT string extraction for LevelName:
                // TAG_String id 8, 2-byte name len (9), b"LevelName", followed by 2-byte value len
                if let Some(pos) = buffer.windows(9).position(|w| w == b"LevelName") {
                    let val_pos = pos + 9;
                    if val_pos + 2 <= buffer.len() {
                        let val_len = u16::from_be_bytes([buffer[val_pos], buffer[val_pos + 1]]) as usize;
                        if val_len > 0 && val_len <= 128 && val_pos + 2 + val_len <= buffer.len() {
                            if let Ok(raw_name) = std::str::from_utf8(&buffer[val_pos + 2..val_pos + 2 + val_len]) {
                                let trimmed = raw_name.trim();
                                if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
                                    if trimmed.contains('/') || trimmed.contains('\\') {
                                        if let Some(base) = Path::new(trimmed).file_name() {
                                            display_name = base.to_string_lossy().to_string();
                                        }
                                    } else {
                                        display_name = trimmed.to_string();
                                    }
                                }
                            }
                        }
                    }
                }

                // Sanitize display_name: never allow corrupted substrings
                if display_name.to_lowercase().contains("forgelifecycle")
                    || display_name.to_lowercase().contains("minecraftsaves")
                    || display_name.contains('/')
                    || display_name.contains('\\')
                    || display_name.is_empty()
                {
                    display_name = folder_name.clone();
                }

                if display_name.starts_with("New-World-") {
                    display_name = display_name
                        .replace("-_", " (")
                        .replace("_-", ")")
                        .replace('_', " ");
                }

                let text = String::from_utf8_lossy(&buffer);
                if text.contains("GameType\u{0001}") || text.to_lowercase().contains("creative") {
                    game_mode = "Creative Mode".to_string();
                } else if text.contains("hardcore\u{0001}") {
                    game_mode = "Hardcore Mode".to_string();
                }
                if text.contains("1.21.1") {
                    world_version = "1.21.1".to_string();
                } else if text.contains("1.20.1") {
                    world_version = "1.20.1".to_string();
                }
            }
        }
    }

    let mut mods_list = Vec::new();
    let candidate_mods_dirs = [
        PathBuf::from(instance_dir).join("mods"),
        PathBuf::from(instance_dir).join(".minecraft").join("mods"),
        PathBuf::from(instance_dir).join("server").join("mods"),
    ];
    for m_dir in &candidate_mods_dirs {
        if m_dir.exists() {
            if let Ok(mod_entries) = fs::read_dir(m_dir) {
                for me in mod_entries.flatten() {
                    let mfname = me.file_name().to_string_lossy().to_string();
                    if mfname.ends_with(".jar") {
                        let clean_name = mfname
                            .trim_end_matches(".jar")
                            .split(&['-', '_'][..])
                            .next()
                            .unwrap_or(&mfname)
                            .to_string();
                        if !mods_list.contains(&clean_name) {
                            mods_list.push(clean_name);
                        }
                    }
                }
            }
        }
    }

    // Automatically convert this world into a first-class Conduit Realm with on-demand config
    let realm_meta_path = path.join("conduit_realm.json");
    let mut hosting_mode = "on_demand".to_string();
    if realm_meta_path.exists() {
        if let Ok(data) = fs::read_to_string(&realm_meta_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(m) = json.get("hosting_mode").and_then(|v| v.as_str()) {
                    hosting_mode = m.to_string();
                }
            }
        }
    } else {
        let meta_obj = serde_json::json!({
            "is_conduit_realm": true,
            "hosting_mode": "on_demand",
            "idle_timeout_minutes": 10,
            "gateway_port": 25565,
            "backend_port": 25566,
            "converted_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        });
        let _ = fs::write(&realm_meta_path, meta_obj.to_string());
    }

    Some(WorldSaveInfo {
        folder_name,
        display_name,
        last_played,
        game_mode,
        icon_base64,
        version: Some(world_version),
        loader: Some(world_loader),
        mods: mods_list,
        runtime_name: Some(runtime_name),
        runtime_id: Some(runtime_id),
        full_path: Some(path.to_string_lossy().to_string()),
        server_address,
        is_realm: true,
        hosting_mode: Some(hosting_mode),
    })
}

#[tauri::command]
pub async fn list_worlds(instance_dir: String) -> Result<Vec<WorldSaveInfo>, String> {
    let mut worlds = Vec::new();

    // 1. Scan Conduit instance's active saves
    let saves_dir = PathBuf::from(&instance_dir).join(".minecraft").join("saves");
    if saves_dir.exists() {
        if let Ok(entries) = fs::read_dir(&saves_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let (rt_name, rt_id, loader, ver) = if let Ok(link_target) = fs::read_link(&path) {
                        let target_str = link_target.to_string_lossy();
                        let inferred_name = link_target
                            .parent()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| fname.clone());
                        let inferred_loader = if target_str.to_lowercase().contains("fabric") {
                            "Fabric".to_string()
                        } else if target_str.to_lowercase().contains("neoforge") {
                            "NeoForge".to_string()
                        } else if target_str.to_lowercase().contains("forge") {
                            "Forge".to_string()
                        } else {
                            "Vanilla".to_string()
                        };
                        let inferred_id = inferred_name.to_lowercase().replace(' ', "-");
                        (inferred_name, inferred_id, inferred_loader, "1.21.1".to_string())
                    } else {
                        let inferred_loader = if fname.to_lowercase().contains("fabric") {
                            "Fabric".to_string()
                        } else if fname.to_lowercase().contains("neoforge") {
                            "NeoForge".to_string()
                        } else if fname.to_lowercase().contains("vanilla") || fname.to_lowercase().contains("world1") {
                            "Vanilla".to_string()
                        } else {
                            "NeoForge".to_string()
                        };
                        let inferred_name = fname.replace('_', " ").replace('-', " ");
                        let inferred_id = fname.to_lowercase().replace(' ', "-");
                        (inferred_name, inferred_id, inferred_loader, "1.21.1".to_string())
                    };

                    if let Some(w) = parse_single_world_save(&path, rt_name, rt_id, loader, ver, None, &instance_dir) {
                        worlds.push(w);
                    }
                }
            }
        }
    }

    worlds.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    Ok(worlds)
}

#[tauri::command]
pub async fn import_external_saves(instance_dir: String) -> Result<usize, String> {
    let saves_dir = PathBuf::from(&instance_dir).join(".minecraft").join("saves");
    fs::create_dir_all(&saves_dir).map_err(|e| e.to_string())?;

    let mut imported = 0;

    if let Ok(appdata) = std::env::var("APPDATA") {
        let profiles_dir = PathBuf::from(appdata).join("ModrinthApp").join("profiles");
        if profiles_dir.exists() {
            if let Ok(profiles) = fs::read_dir(&profiles_dir) {
                for prof_entry in profiles.flatten() {
                    let prof_path = prof_entry.path();
                    let prof_saves = prof_path.join("saves");
                    if prof_saves.exists() {
                        if let Ok(save_entries) = fs::read_dir(&prof_saves) {
                            for s_entry in save_entries.flatten() {
                                let s_path = s_entry.path();
                                if s_path.is_dir() {
                                    let s_name = s_entry.file_name().to_string_lossy().to_string();
                                    // Keep real world names intact
                                    let link_target = saves_dir.join(&s_name);
                                    if !link_target.exists() {
                                        #[cfg(target_os = "windows")]
                                        {
                                            use std::os::windows::process::CommandExt;
                                            let _ = std::process::Command::new("cmd")
                                                .args(["/C", "mklink", "/J", &link_target.to_string_lossy(), &s_path.to_string_lossy()])
                                                .creation_flags(0x08000000)
                                                .status();
                                            imported += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(imported)
}

#[tauri::command]
pub async fn list_downloaded_runtimes(instance_dir: String) -> Result<Vec<DownloadedRuntimeItem>, String> {
    let mut runtimes = Vec::new();
    let versions_dir = PathBuf::from(&instance_dir).join(".minecraft").join("versions");

    if versions_dir.exists() {
        if let Ok(entries) = fs::read_dir(versions_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let loader = if folder_name.to_lowercase().contains("neoforge") {
                        "NeoForge"
                    } else if folder_name.to_lowercase().contains("fabric") {
                        "Fabric"
                    } else if folder_name.to_lowercase().contains("quilt") {
                        "Quilt"
                    } else if folder_name.to_lowercase().contains("forge") {
                        "Forge"
                    } else if folder_name.to_lowercase().contains("bedrock") {
                        "Bedrock"
                    } else {
                        "Vanilla"
                    };

                    let version = if folder_name.contains("1.21.1") {
                        "1.21.1"
                    } else if folder_name.contains("1.21") {
                        "1.21"
                    } else if folder_name.contains("1.20.1") {
                        "1.20.1"
                    } else {
                        "1.21.1"
                    };

                    let mut size_bytes = 0u64;
                    if let Ok(files) = fs::read_dir(&path) {
                        for f in files.flatten() {
                            if let Ok(m) = f.metadata() {
                                size_bytes += m.len();
                            }
                        }
                    }

                    runtimes.push(DownloadedRuntimeItem {
                        id: folder_name,
                        loader: loader.to_string(),
                        version: version.to_string(),
                        is_server: false,
                        is_client: true,
                        size_mb: (size_bytes as f64) / (1024.0 * 1024.0),
                    });
                }
            }
        }
    }

    let srv_jar = PathBuf::from(&instance_dir).join("server").join("server.jar");
    if srv_jar.exists() {
        let size_bytes = fs::metadata(&srv_jar).map(|m| m.len()).unwrap_or(0);
        runtimes.push(DownloadedRuntimeItem {
            id: "server-dedicated".to_string(),
            loader: "NeoForge Dedicated".to_string(),
            version: "1.21.1".to_string(),
            is_server: true,
            is_client: false,
            size_mb: (size_bytes as f64) / (1024.0 * 1024.0),
        });
    }

    Ok(runtimes)
}

#[tauri::command]
pub async fn delete_downloaded_runtime(instance_dir: String, runtime_id: String) -> Result<(), String> {
    let client_target = PathBuf::from(&instance_dir).join(".minecraft").join("versions").join(&runtime_id);
    if client_target.exists() {
        fs::remove_dir_all(&client_target).map_err(|e| format!("Failed to delete client runtime: {e}"))?;
    }
    if runtime_id == "server-dedicated" {
        let srv_jar = PathBuf::from(&instance_dir).join("server").join("server.jar");
        if srv_jar.exists() {
            let _ = fs::remove_file(srv_jar);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_realm(instance_dir: String, folder_name: String) -> Result<(), String> {
    let clean = folder_name.trim();
    if clean.is_empty() || clean == "." || clean == ".." {
        return Err("Invalid realm folder name".to_string());
    }

    let candidate_paths = vec![
        PathBuf::from(&instance_dir).join(".minecraft").join("saves").join(clean),
        PathBuf::from(&instance_dir).join("server").join(clean),
        PathBuf::from(&instance_dir).join("server").join("world"),
    ];

    let mut deleted = false;
    for path in candidate_paths {
        if path.exists() {
            if is_junction_or_symlink(&path) {
                let _ = remove_junction(&path);
                deleted = true;
            } else if path.file_name().and_then(|n| n.to_str()) == Some(clean) {
                let _ = fs::remove_dir_all(&path);
                deleted = true;
            }
        }
    }

    if deleted {
        Ok(())
    } else {
        Err(format!("Realm '{}' was not found on disk.", clean))
    }
}

#[tauri::command]
pub async fn delete_instance(instance_dir: String, instance_id: String) -> Result<(), String> {
    let clean_id = instance_id.trim();
    if clean_id.is_empty() || clean_id == "." || clean_id == ".." {
        return Err("Invalid instance ID".to_string());
    }

    // Check under instances/<instance_id>
    let custom_instance_path = PathBuf::from(&instance_dir).join("..").join("instances").join(clean_id);
    if custom_instance_path.exists() {
        let _ = fs::remove_dir_all(custom_instance_path);
    }

    let direct_instances_path = PathBuf::from("instances").join(clean_id);
    if direct_instances_path.exists() {
        let _ = fs::remove_dir_all(direct_instances_path);
    }

    Ok(())
}

#[tauri::command]
pub async fn get_realm_prune_info(instance_dir: String, world_folder: String, dimension: String) -> Result<DimensionPruneSummary, String> {
    let world_base = PathBuf::from(&instance_dir).join(".minecraft").join("saves").join(&world_folder);
    
    // Check if Bedrock LevelDB save
    let db_path = world_base.join("db");
    if db_path.exists() {
        return Ok(DimensionPruneSummary {
            dimension: dimension.clone(),
            total_regions: 1,
            total_chunks: 64,
            regions: vec![RegionChunkInfo {
                region_x: 0,
                region_z: 0,
                chunk_count: 64,
                populated_chunks: (0..8).flat_map(|x| (0..8).map(move |z| (x, z))).collect(),
            }],
            is_bedrock: true,
        });
    }

    // Java Region directory
    let region_dir = match dimension.as_str() {
        "the_nether" | "DIM-1" => world_base.join("DIM-1").join("region"),
        "the_end" | "DIM1" => world_base.join("DIM1").join("region"),
        _ => world_base.join("region"),
    };

    if !region_dir.exists() {
        return Ok(DimensionPruneSummary {
            dimension,
            total_regions: 0,
            total_chunks: 0,
            regions: Vec::new(),
            is_bedrock: false,
        });
    }

    let mut regions = Vec::new();
    let mut total_chunks = 0;

    if let Ok(entries) = fs::read_dir(&region_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            // Format: r.X.Z.mca
            if fname.starts_with("r.") && fname.ends_with(".mca") {
                let parts: Vec<&str> = fname.split('.').collect();
                if parts.len() == 4 {
                    let rx = parts[1].parse::<i32>().unwrap_or(0);
                    let rz = parts[2].parse::<i32>().unwrap_or(0);

                    // Sample populated chunks to avoid sending massive IPC payloads over 50MB
                    let mut populated = Vec::new();
                    let mut populated_count = 0;
                    if let Ok(mut f) = File::open(&path) {
                        let mut table = [0u8; 4096];
                        if f.read_exact(&mut table).is_ok() {
                            for i in 0..1024 {
                                let offset_high = table[i * 4] as u32;
                                let offset_mid = table[i * 4 + 1] as u32;
                                let offset_low = table[i * 4 + 2] as u32;
                                let sector_count = table[i * 4 + 3];
                                let offset = (offset_high << 16) | (offset_mid << 8) | offset_low;
                                if offset != 0 && sector_count != 0 {
                                    populated_count += 1;
                                    let local_cx = (i % 32) as i32;
                                    let local_cz = (i / 32) as i32;
                                    let global_cx = rx * 32 + local_cx;
                                    let global_cz = rz * 32 + local_cz;
                                    if populated.len() < 16 {
                                        populated.push((global_cx, global_cz));
                                    }
                                }
                            }
                        }
                    }

                    total_chunks += populated_count;
                    regions.push(RegionChunkInfo {
                        region_x: rx,
                        region_z: rz,
                        chunk_count: populated_count,
                        populated_chunks: populated,
                    });
                }
            }
        }
    }

    Ok(DimensionPruneSummary {
        dimension,
        total_regions: regions.len(),
        total_chunks,
        regions,
        is_bedrock: false,
    })
}

#[tauri::command]
pub async fn execute_realm_prune(
    instance_dir: String,
    world_folder: String,
    dimension: String,
    chunks_to_prune: Vec<(i32, i32)>,
    mode: String, // "delete_selected" | "keep_selected"
) -> Result<String, String> {
    let prune_set: HashSet<(i32, i32)> = chunks_to_prune.into_iter().collect();
    let world_base = PathBuf::from(&instance_dir).join(".minecraft").join("saves").join(&world_folder);
    
    let region_dir = match dimension.as_str() {
        "the_nether" | "DIM-1" => world_base.join("DIM-1").join("region"),
        "the_end" | "DIM1" => world_base.join("DIM1").join("region"),
        _ => world_base.join("region"),
    };

    if !region_dir.exists() {
        return Ok("No region directory found to prune.".to_string());
    }

    let mut chunks_reset = 0;
    if let Ok(entries) = fs::read_dir(&region_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with("r.") && fname.ends_with(".mca") {
                let parts: Vec<&str> = fname.split('.').collect();
                if parts.len() == 4 {
                    let rx = parts[1].parse::<i32>().unwrap_or(0);
                    let rz = parts[2].parse::<i32>().unwrap_or(0);

                    if let Ok(mut bytes) = fs::read(&path) {
                        if bytes.len() >= 4096 {
                            let mut modified = false;
                            for i in 0..1024 {
                                let local_cx = (i % 32) as i32;
                                let local_cz = (i / 32) as i32;
                                let global_cx = rx * 32 + local_cx;
                                let global_cz = rz * 32 + local_cz;

                                let should_wipe = if mode == "delete_selected" {
                                    prune_set.contains(&(global_cx, global_cz))
                                } else {
                                    !prune_set.contains(&(global_cx, global_cz))
                                };

                                if should_wipe {
                                    let offset = i * 4;
                                    if bytes[offset] != 0 || bytes[offset + 1] != 0 || bytes[offset + 2] != 0 || bytes[offset + 3] != 0 {
                                        bytes[offset] = 0;
                                        bytes[offset + 1] = 0;
                                        bytes[offset + 2] = 0;
                                        bytes[offset + 3] = 0;
                                        chunks_reset += 1;
                                        modified = true;
                                    }
                                }
                            }
                            if modified {
                                let _ = fs::write(&path, &bytes);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(format!("Successfully pruned and reset {} chunks in Realm '{}' [{}].", chunks_reset, world_folder, dimension))
}

#[tauri::command]
pub async fn create_new_realm(
    instance_dir: String,
    realm_name: String,
    game_mode: String,
    seed: String,
) -> Result<String, String> {
    let clean_name: String = realm_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let folder_name = clean_name.trim().replace(' ', "_");
    let target_dir = PathBuf::from(&instance_dir).join(".minecraft").join("saves").join(&folder_name);

    if target_dir.exists() {
        return Err(format!("Realm with folder name '{}' already exists.", folder_name));
    }

    fs::create_dir_all(&target_dir).map_err(|e| format!("Failed to create realm directory: {e}"))?;
    fs::create_dir_all(target_dir.join("region")).map_err(|e| e.to_string())?;

    // Create a descriptor metadata marker
    let desc = format!("Realm: {}\nSeed: {}\nMode: {}\nCreated via Conduit Realm Engine\n", clean_name, seed, game_mode);
    let _ = fs::write(target_dir.join("conduit_realm.txt"), desc);

    Ok(folder_name)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateServerPayload {
    pub server_type: String,       // "Vanilla" | "Paper" | "NeoForge" | "Fabric" | "Purpur" | "Modpack"
    pub name: String,              // "Squid-Server-#02"
    pub save_location: String,     // world / server path
    pub ram_mb: u64,               // 3584
    pub java_path: String,         // java path
    pub mc_version: String,        // "1.21.1"
    pub seed: String,              // optional seed
    pub world_type: String,        // "default", "flat", etc.
    pub game_mode: String,         // "survival", "creative", etc.
    pub is_imported_world: bool,
    pub imported_world_path: Option<String>,
    pub enable_tunnel: bool,
    pub enable_backups: bool,
    pub hosting_mode: String,      // "on_demand" | "sleep_when_empty" | "always_online"
    pub instance_dir: Option<String>,
}

#[tauri::command]
pub async fn create_server_wizard(
    payload: CreateServerPayload,
) -> Result<String, String> {
    let clean_folder: String = payload.name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let folder_name = clean_folder.trim().to_string();

    let instance_path = payload.instance_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./instance"));

    let instance_saves_dir = instance_path.join(".minecraft").join("saves");
    let _ = fs::create_dir_all(&instance_saves_dir);
    let realm_world_dir = instance_saves_dir.join(&folder_name);

    // 1. World Creation or Zero-Duplication NTFS Junction Import
    if payload.is_imported_world {
        if let Some(ref imp_path_str) = payload.imported_world_path {
            let imp_path = PathBuf::from(imp_path_str);
            if imp_path.exists() && !realm_world_dir.exists() {
                create_directory_junction(&realm_world_dir, &imp_path)?;
            }
        }
    } else {
        if !realm_world_dir.exists() {
            fs::create_dir_all(&realm_world_dir).map_err(|e| format!("Failed to create realm directory: {e}"))?;
            let _ = fs::create_dir_all(realm_world_dir.join("region"));
            let desc = format!("Realm: {}\nSeed: {}\nMode: {}\nEngine: {} {}\n", payload.name, payload.seed, payload.game_mode, payload.server_type, payload.mc_version);
            let _ = fs::write(realm_world_dir.join("conduit_realm.txt"), desc);
        }
    }

    // 2. Write conduit_realm.json metadata in world save
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let port = if payload.hosting_mode == "always_online" { 25565 } else { 25566 };

    let meta_obj = serde_json::json!({
        "is_conduit_realm": true,
        "server_name": payload.name,
        "folder_name": folder_name,
        "server_type": payload.server_type,
        "version": payload.mc_version,
        "ram_mb": payload.ram_mb,
        "java_path": payload.java_path,
        "hosting_mode": payload.hosting_mode,
        "seed": payload.seed,
        "world_type": payload.world_type,
        "game_mode": payload.game_mode,
        "idle_timeout_minutes": 10,
        "gateway_port": 25565,
        "backend_port": port,
        "enable_tunnel": payload.enable_tunnel,
        "enable_backups": payload.enable_backups,
        "created_at": now
    });
    let _ = fs::write(realm_world_dir.join("conduit_realm.json"), serde_json::to_string_pretty(&meta_obj).unwrap_or_default());

    // 3. Ensure Instance Server Shared Environment (Libraries & Mods with Zero Duplication)
    let server_dir = instance_path.join("server");
    let _ = fs::create_dir_all(&server_dir);
    let _ = fs::write(server_dir.join("eula.txt"), "eula=true\n");

    let props_path = server_dir.join("server.properties");
    if !props_path.exists() {
        let gmode = payload.game_mode.to_lowercase();
        let wtype = payload.world_type.to_lowercase();
        let props_content = format!(
            "#Minecraft server properties\nserver-port={}\nquery.port={}\nlevel-name=world\ngamemode={}\ndifficulty=easy\nlevel-type={}\nlevel-seed={}\nmotd=\\u00A76\\u00A7lCONDUIT \\u00A78\\u00BB \\u00A7f{}\nonline-mode=false\nenable-command-block=true\nmax-players=20\n",
            port, port, gmode, wtype, payload.seed, payload.name
        );
        let _ = fs::write(&props_path, props_content);
    }

    let _ = fs::write(server_dir.join("user_jvm_args.txt"), format!("-Xms1024M\n-Xmx{}M\n", payload.ram_mb));

    // Ensure zero-duplication junctions for libraries, config, and hardlinked mods
    let _ = ensure_zero_duplication_junctions(&instance_path);
    let _ = sync_server_mods(&instance_path);

    // Dynamically attach server/world junction to the newly created realm
    let server_world = server_dir.join("world");
    if server_world.exists() {
        if is_junction_or_symlink(&server_world) {
            let _ = remove_junction(&server_world);
            let _ = create_directory_junction(&server_world, &realm_world_dir);
        }
    } else {
        let _ = create_directory_junction(&server_world, &realm_world_dir);
    }

    // 4. Also maintain legacy SquidServers interoperability if save_location is customized
    let target_dir = PathBuf::from(&payload.save_location);
    if target_dir != realm_world_dir && !target_dir.as_os_str().is_empty() {
        let _ = fs::create_dir_all(&target_dir);
        let _ = fs::write(target_dir.join("conduit_realm.json"), serde_json::to_string_pretty(&meta_obj).unwrap_or_default());
    }

    // 7. Sync with SquidServers servers.json so SquidServers is 100% interoperable
    if let Ok(appdata) = std::env::var("APPDATA") {
        let sq_servers_json = PathBuf::from(&appdata).join("squidservers").join("servers.json");
        let mut servers_list: Vec<serde_json::Value> = if sq_servers_json.exists() {
            fs::read_to_string(&sq_servers_json)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let server_id = format!("server-{}-{}", now * 1000, &payload.name.to_lowercase().replace(' ', "-"));
        let new_entry = serde_json::json!({
            "id": server_id,
            "name": payload.name,
            "version": payload.mc_version,
            "status": "offline",
            "ram": payload.ram_mb,
            "lastActive": (now * 1000) as i64,
            "createdAt": "2026-11-04T00:00:00.000Z",
            "serverPath": payload.save_location,
            "serverType": payload.server_type.to_lowercase(),
            "javaPath": payload.java_path,
            "tunnels": [
                {
                    "tunnelId": "c2b4398b-f944-4e63-bad4-8b9f4682b79c",
                    "mainJava": true,
                    "mainBedrock": false
                }
            ],
            "enableNogui": true,
            "jvmArgs": [],
            "selectedPlugins": [],
            "autoBackupEnabled": payload.enable_backups,
            "backupFrequency": "every6hours",
            "maxBackupsToKeep": 3,
            "squidserversContentEnabled": true,
            "required_mods": [],
        });

        // Replace existing entry if same name or append
        if let Some(pos) = servers_list.iter().position(|s| s.get("name").and_then(|n| n.as_str()) == Some(&payload.name)) {
            servers_list[pos] = new_entry;
        } else {
            servers_list.push(new_entry);
        }

        if let Ok(updated_data) = serde_json::to_string_pretty(&servers_list) {
            let _ = fs::write(&sq_servers_json, updated_data);
        }
    }

    Ok(payload.name)
}

#[tauri::command]
pub async fn get_deduplication_metrics(instance_dir: String) -> Result<DeduplicationMetrics, String> {
    let server_dir = PathBuf::from(&instance_dir).join("server");
    let client_dir = PathBuf::from(&instance_dir).join(".minecraft");

    let server_lib = server_dir.join("libraries");
    let client_lib = client_dir.join("libraries");

    let server_world = server_dir.join("world");
    let client_saves = client_dir.join("saves");

    let libraries_shared = is_junction_or_symlink(&server_lib);
    let configs_shared = is_junction_or_symlink(&server_dir.join("config"));
    let world_shared = is_junction_or_symlink(&server_world);

    let client_libraries_size_mb = get_dir_size_mb(&client_lib);
    let server_libraries_size_mb = if libraries_shared { 0.0 } else { get_dir_size_mb(&server_lib) };
    let saved_disk_mb = if libraries_shared { client_libraries_size_mb } else { 0.0 };

    Ok(DeduplicationMetrics {
        libraries_shared,
        configs_shared,
        world_shared,
        mods_hardlinked: 0,
        client_libraries_size_mb,
        server_libraries_size_mb,
        saved_disk_mb,
        server_world_path: server_world.to_string_lossy().to_string(),
        client_world_path: client_saves.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn verify_and_repair_junctions(
    instance_dir: String,
    target_world_folder: String,
) -> Result<String, String> {
    let server_dir = PathBuf::from(&instance_dir).join("server");
    let client_dir = PathBuf::from(&instance_dir).join(".minecraft");

    let target_client_world = client_dir.join("saves").join(&target_world_folder);
    if !target_client_world.exists() {
        return Err(format!("Client world save does not exist: {:?}", target_client_world));
    }

    let server_world = server_dir.join("world");
    if server_world.exists() {
        if is_junction_or_symlink(&server_world) {
            let _ = remove_junction(&server_world);
        } else {
            let backup_path = server_dir.join(format!("world_backup_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));
            let _ = fs::rename(&server_world, backup_path);
        }
    }

    create_directory_junction(&server_world, &target_client_world)?;

    let server_lib = server_dir.join("libraries");
    let client_lib = client_dir.join("libraries");
    if client_lib.exists() && (!server_lib.exists() || !is_junction_or_symlink(&server_lib)) {
        if server_lib.exists() {
            let _ = fs::remove_dir_all(&server_lib);
        }
        let _ = create_directory_junction(&server_lib, &client_lib);
    }

    Ok(format!("Junction cleanly attached: {:?} -> {:?}", server_world, target_client_world))
}

fn extract_mod_metadata(jar_path: &Path) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let file = match File::open(jar_path) {
        Ok(f) => f,
        Err(_) => return (None, None, None, None),
    };

    let mut archive = match zip::ZipArchive::new(BufReader::new(file)) {
        Ok(a) => a,
        Err(_) => return (None, None, None, None),
    };

    let mut icon_base64 = None;
    for icon_candidate in ["logo.png", "icon.png", "assets/logo.png", "assets/icon.png"] {
        if let Ok(mut entry) = archive.by_name(icon_candidate) {
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() {
                use base64::Engine;
                icon_base64 = Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf)));
                break;
            }
        }
    }

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
                return (id, name, ver, icon_base64);
            }
        }
    }

    if let Ok(mut entry) = archive.by_name("fabric.mod.json") {
        let mut f_content = String::new();
        if entry.read_to_string(&mut f_content).is_ok() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&f_content) {
                let id = val.get("id").and_then(|i| i.as_str()).map(String::from);
                let name = val.get("name").and_then(|n| n.as_str()).map(String::from);
                let ver = val.get("version").and_then(|v| v.as_str()).map(String::from);
                return (id, name, ver, icon_base64);
            }
        }
    }

    (None, None, None, icon_base64)
}

fn get_dir_size_mb(path: &Path) -> f64 {
    if !path.exists() {
        return 0.0;
    }
    let mut total_bytes = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total_bytes += (get_dir_size_mb(&entry.path()) * 1024.0 * 1024.0) as u64;
                } else {
                    total_bytes += meta.len();
                }
            }
        }
    }
    (total_bytes as f64) / (1024.0 * 1024.0)
}

#[cfg(windows)]
pub fn is_junction_or_symlink(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    if let Ok(meta) = fs::symlink_metadata(path) {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
    }
    false
}

#[cfg(not(windows))]
pub fn is_junction_or_symlink(path: &Path) -> bool {
    if let Ok(meta) = fs::symlink_metadata(path) {
        meta.file_type().is_symlink()
    } else {
        false
    }
}

#[cfg(windows)]
pub fn create_directory_junction(junction_path: &Path, target_path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(junction_path.as_os_str())
        .arg(target_path.as_os_str())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: Prevents command prompt flashing
        .output()
        .map_err(|e| format!("Failed to spawn mklink: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn create_directory_junction(junction_path: &Path, target_path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target_path, junction_path).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn remove_junction(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("cmd")
        .args(["/C", "rmdir"])
        .arg(path.as_os_str())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: Prevents command prompt flashing
        .output()
        .map_err(|e| format!("Failed to remove junction: {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn remove_junction(path: &Path) -> Result<(), String> {
    fs::remove_dir(path).map_err(|e| e.to_string())
}

pub fn ensure_zero_duplication_junctions(instance_dir: &Path) -> Result<(), String> {
    let server_dir = instance_dir.join("server");
    let client_dir = instance_dir.join(".minecraft");

    if !server_dir.exists() {
        let _ = fs::create_dir_all(&server_dir);
    }
    if !client_dir.exists() {
        let _ = fs::create_dir_all(&client_dir);
    }

    let client_libs = client_dir.join("libraries");
    let server_libs = server_dir.join("libraries");
    if client_libs.exists() && (!server_libs.exists() || !is_junction_or_symlink(&server_libs) || !server_libs.join("net").exists()) {
        if server_libs.exists() {
            if is_junction_or_symlink(&server_libs) {
                let _ = remove_junction(&server_libs);
            } else {
                let _ = fs::remove_dir_all(&server_libs);
            }
        }
        let _ = create_directory_junction(&server_libs, &client_libs);
    }

    let client_cfg = client_dir.join("config");
    let server_cfg = server_dir.join("config");
    if client_cfg.exists() && (!server_cfg.exists() || !is_junction_or_symlink(&server_cfg)) {
        if server_cfg.exists() {
            if is_junction_or_symlink(&server_cfg) {
                let _ = remove_junction(&server_cfg);
            } else {
                let _ = fs::remove_dir_all(&server_cfg);
            }
        }
        let _ = create_directory_junction(&server_cfg, &client_cfg);
    }

    let client_mods = client_dir.join("mods");
    let root_mods = instance_dir.join("mods");
    if root_mods.exists() && (!client_mods.exists() || !is_junction_or_symlink(&client_mods)) {
        if client_mods.exists() {
            let _ = fs::remove_dir_all(&client_mods);
        }
        let target = if let Ok(link_target) = fs::read_link(&root_mods) {
            link_target
        } else {
            root_mods.clone()
        };
        let _ = create_directory_junction(&client_mods, &target);
    }

    Ok(())
}

pub fn is_client_only_mod(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    let client_keywords = [
        "client", "sodium", "iris", "oculus", "rubidium", "embeddium", "chloride",
        "seleafculling", "entityculling", "immediatelyfast", "moreculling", "dynamic_fps",
        "dynamicfps", "badoptimizations", "borderless", "betterthirdperson", "controlling",
        "searchables", "xaerominimap", "xaeroworldmap", "xaero", "voxelmap", "journeymap",
        "sound-physics", "sound_physics", "presencefootsteps", "presence_footsteps", "pfsable",
        "ambient_sounds", "wavey_capes", "skinlayers3d", "notenoughanimations", "crumbs", "voxy",
        "shulkerboxtooltip", "armorposer", "better_modlist", "colorwheel",
        "modmenu", "reeses", "zoomify", "continuity", "indium",
        "ferritecore-client", "jei-client", "emi-client", "optifine",
        "particular", "freecam", "ok_zoomer", "entity_model_features", "entity_texture_features",
        "inventoryprofilesnext", "libipn", "lighty", "bridgingmod", "appleskin", "enchdesc"
    ];
    for kw in client_keywords {
        if lower.contains(kw) {
            return true;
        }
    }
    false
}

pub fn sync_server_mods(instance_dir: &Path) -> Result<(), String> {
    let client_mods = instance_dir.join("mods");
    let server_mods = instance_dir.join("server").join("mods");

    if !client_mods.exists() {
        return Ok(());
    }
    if !server_mods.exists() {
        fs::create_dir_all(&server_mods).map_err(|e| e.to_string())?;
    }

    // Purge any client-only mods from server/mods
    if let Ok(entries) = fs::read_dir(&server_mods) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if is_client_only_mod(&fname) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&client_mods) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.ends_with(".jar") && !is_client_only_mod(&fname) {
                let target = server_mods.join(&fname);
                if !target.exists() {
                    let _ = fs::hard_link(&path, &target).or_else(|_| fs::copy(&path, &target).map(|_| ()));
                }
            }
        }
    }

    Ok(())
}
